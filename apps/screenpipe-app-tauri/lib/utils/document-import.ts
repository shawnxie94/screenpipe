// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

// Local document manual import: turn a picked/dropped file into a persistent,
// searchable local evidence source via the engine's /documents API.
//
// Flow per file: read bytes → SHA-256 → upload bytes (content-addressed
// managed copy) → extract text with the shared chat-attachment parsers →
// submit text for indexing. Any failure along the way is recorded through
// /documents/import-failed so the failure is visible and retryable — the
// engine owns the authoritative size/type guards, this side keeps its
// pre-flight checks only to fail fast without shipping bytes.

import { localFetch } from "@/lib/api";
import {
  extractDocument,
  extFromName,
  isSupportedDocExt,
} from "@/lib/pi/extract-document";

export interface LocalDocumentImportInput {
  name: string;
  loadBytes: () => Promise<Uint8Array>;
  /** Source path when the drop/picker provided one (Tauri). Metadata only. */
  originalPath?: string;
}

export type LocalDocumentImportStatus =
  | "imported"
  | "duplicate"
  | "failed";

export interface LocalDocumentImportResult {
  name: string;
  status: LocalDocumentImportStatus;
  reason?: string;
  /** Content hash once known — lets callers (directory auto-ingest) link the
   *  file location back to the managed document record. */
  sha256?: string;
}

export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    bytes.buffer.slice(
      bytes.byteOffset,
      bytes.byteOffset + bytes.byteLength,
    ) as ArrayBuffer,
  );
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function recordFailed(
  input: LocalDocumentImportInput,
  reason: string,
  sha256?: string,
  sizeBytes?: number,
): Promise<void> {
  try {
    await localFetch("/documents/import-failed", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        sha256,
        filename: input.name,
        ext: extFromName(input.name),
        size_bytes: sizeBytes,
        original_path: input.originalPath,
        reason,
      }),
    });
  } catch {
    // The engine may not be reachable; the toast still surfaces the reason.
  }
}

export async function importLocalDocument(
  input: LocalDocumentImportInput,
): Promise<LocalDocumentImportResult> {
  const ext = extFromName(input.name);
  if (!isSupportedDocExt(ext)) {
    const reason = `不支持的文件类型 .${ext || "?"}`;
    await recordFailed(input, reason);
    return { name: input.name, status: "failed", reason };
  }

  let bytes: Uint8Array;
  try {
    bytes = await input.loadBytes();
  } catch (err) {
    const reason = `无法读取文件：${err instanceof Error ? err.message : String(err)}`;
    await recordFailed(input, reason);
    return { name: input.name, status: "failed", reason };
  }

  if (bytes.byteLength === 0) {
    const reason = "文件是空的";
    await recordFailed(input, reason, undefined, 0);
    return { name: input.name, status: "failed", reason };
  }

  const sha256 = await sha256Hex(bytes);

  // Upload the bytes first so the managed copy exists even if text
  // extraction later fails — the failure stays visible against a stored
  // document instead of vanishing with the read-only chat attachment.
  let uploadResp: Response;
  try {
    const params = new URLSearchParams({ filename: input.name });
    if (input.originalPath) params.set("original_path", input.originalPath);
    uploadResp = await localFetch(`/documents/import?${params}`, {
      method: "POST",
      headers: { "Content-Type": "application/octet-stream" },
      body: bytes as unknown as BodyInit,
    });
  } catch (err) {
    const reason = `上传失败：${err instanceof Error ? err.message : String(err)}`;
    await recordFailed(input, reason, sha256, bytes.byteLength);
    return { name: input.name, status: "failed", reason };
  }
  if (!uploadResp.ok) {
    const reason = await uploadErrorMessage(uploadResp);
    await recordFailed(input, reason, sha256, bytes.byteLength);
    return { name: input.name, status: "failed", reason };
  }
  const uploaded = (await uploadResp.json()) as {
    sha256: string;
    status: "imported" | "duplicate";
  };

  // Extract and submit text. A duplicate upload still re-submits text: if a
  // previous attempt failed mid-way, this retries the indexing to done.
  let text: string;
  let truncated: boolean;
  try {
    const doc = await extractDocument(input.name, bytes);
    text = doc.text;
    truncated = doc.truncated;
  } catch (err) {
    const reason = `解析失败：${err instanceof Error ? err.message : String(err)}`;
    await recordFailed(input, reason, uploaded.sha256, bytes.byteLength);
    return { name: input.name, status: "failed", reason };
  }
  if (!text.trim()) {
    const reason = "未解析出可搜索的文字（文件为空或仅含图像/扫描内容）";
    await recordFailed(input, reason, uploaded.sha256, bytes.byteLength);
    return { name: input.name, status: "failed", reason };
  }

  try {
    const textResp = await localFetch("/documents/import-text", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        sha256: uploaded.sha256,
        text,
        truncated,
      }),
    });
    if (!textResp.ok) {
      const reason = await uploadErrorMessage(textResp);
      await recordFailed(input, reason, uploaded.sha256, bytes.byteLength);
      return { name: input.name, status: "failed", reason };
    }
  } catch (err) {
    const reason = `写入可搜索文本失败：${err instanceof Error ? err.message : String(err)}`;
    await recordFailed(input, reason, uploaded.sha256, bytes.byteLength);
    return { name: input.name, status: "failed", reason };
  }

  return { name: input.name, status: uploaded.status, sha256: uploaded.sha256 };
}

/** Split batch results into the toast summary the callers render. */
export function summarizeImportResults(results: LocalDocumentImportResult[]): {
  failed: LocalDocumentImportResult[];
  succeeded: number;
} {
  const failed = results.filter((r) => r.status === "failed");
  return { failed, succeeded: results.length - failed.length };
}

async function uploadErrorMessage(resp: Response): Promise<string> {
  try {
    const data = (await resp.json()) as { message?: string };
    if (data?.message) return data.message;
  } catch {
    // non-JSON error body
  }
  return `导入失败（HTTP ${resp.status}）`;
}

/** Audio import (mp3/wav/m4a/webm): the engine stores the managed copy and
 *  transcribes locally, so this call stays in flight until transcription
 *  finishes — long files legitimately take minutes. Reuses the same result
 *  shape so callers can batch audio and document imports together. */
export async function importAudioDocument(
  input: LocalDocumentImportInput,
): Promise<LocalDocumentImportResult> {
  return importMediaDocument("import-audio", input);
}

/** Video import (mp4/mov/mkv): engine extracts the audio track for
 *  transcription and OCRs sparse keyframes — one round trip, same shape. */
export async function importVideoDocument(
  input: LocalDocumentImportInput,
): Promise<LocalDocumentImportResult> {
  return importMediaDocument("import-video", input);
}

async function importMediaDocument(
  endpoint: "import-audio" | "import-video",
  input: LocalDocumentImportInput,
): Promise<LocalDocumentImportResult> {
  let bytes: Uint8Array;
  try {
    bytes = await input.loadBytes();
  } catch (err) {
    const reason = `无法读取文件：${err instanceof Error ? err.message : String(err)}`;
    await recordFailed(input, reason);
    return { name: input.name, status: "failed", reason };
  }
  if (bytes.byteLength === 0) {
    await recordFailed(input, "文件是空的", undefined, 0);
    return { name: input.name, status: "failed", reason: "文件是空的" };
  }

  try {
    const params = new URLSearchParams({ filename: input.name });
    if (input.originalPath) params.set("original_path", input.originalPath);
    const resp = await localFetch(`/documents/${endpoint}?${params}`, {
      method: "POST",
      headers: { "Content-Type": "application/octet-stream" },
      body: bytes as unknown as BodyInit,
      // Transcription is engine-side and synchronous; 20 min covers a
      // 500 MB video upload plus its transcription without hanging forever.
      signal: AbortSignal.timeout(20 * 60_000),
    });
    const data = (await resp.json().catch(() => null)) as {
      sha256?: string;
      status?: string;
      message?: string;
    } | null;
    if (!resp.ok) {
      const reason = data?.message ?? `导入失败（HTTP ${resp.status}）`;
      await recordFailed(input, reason, data?.sha256, bytes.byteLength);
      return { name: input.name, status: "failed", reason, sha256: data?.sha256 };
    }
    return {
      name: input.name,
      status: data?.status === "duplicate" ? "duplicate" : "imported",
      sha256: data?.sha256,
    };
  } catch (err) {
    const reason = `转写请求失败：${err instanceof Error ? err.message : String(err)}`;
    await recordFailed(input, reason);
    return { name: input.name, status: "failed", reason };
  }
}
