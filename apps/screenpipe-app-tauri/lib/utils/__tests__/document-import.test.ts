// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  importAudioDocument,
  importLocalDocument,
  importVideoDocument,
  sha256Hex,
  summarizeImportResults,
} from "@/lib/utils/document-import";
import { isSupportedAudioExt, isSupportedVideoExt } from "@/lib/pi/extract-document";

// jsdom lacks crypto.subtle — node's webcrypto covers it.
vi.stubGlobal("crypto", globalThis.crypto);

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

function okJson(body: unknown) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}
function errJson(status: number, message: string) {
  return new Response(JSON.stringify({ message }), { status });
}

const bytes = new TextEncoder().encode("hello import slice");
const input = (over: Partial<Parameters<typeof importLocalDocument>[0]> = {}) => ({
  name: "notes.md",
  loadBytes: async () => bytes,
  ...over,
});

beforeEach(() => {
  fetchMock.mockReset();
});

describe("importLocalDocument", () => {
  it("uploads bytes, submits extracted text, reports imported", async () => {
    fetchMock
      .mockResolvedValueOnce(
        okJson({ sha256: "abc", status: "imported" }),
      )
      .mockResolvedValueOnce(okJson({ sha256: "abc", state: "ready" }));

    const result = await importLocalDocument(input());

    expect(result).toEqual({
      name: "notes.md",
      status: "imported",
      sha256: "abc",
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    const [uploadPath, uploadInit] = fetchMock.mock.calls[0];
    expect(uploadPath).toContain("/documents/import?filename=notes.md");
    expect(uploadInit.method).toBe("POST");
    const [textPath, textInit] = fetchMock.mock.calls[1];
    expect(textPath).toContain("/documents/import-text");
    const body = JSON.parse(textInit.body);
    expect(body.sha256).toBe("abc");
    expect(body.text).toContain("hello import slice");
  });

  it("reports duplicate without failing", async () => {
    fetchMock
      .mockResolvedValueOnce(okJson({ sha256: "abc", status: "duplicate" }))
      .mockResolvedValueOnce(okJson({ sha256: "abc", state: "ready" }));
    const result = await importLocalDocument(input());
    expect(result.status).toBe("duplicate");
  });

  it("rejects unsupported extensions with a visible reason", async () => {
    const result = await importLocalDocument(input({ name: "photo.png" }));
    expect(result.status).toBe("failed");
    expect(result.reason).toContain("不支持");
    expect(fetchMock.mock.calls.some(([p]) => String(p).includes("/documents/import-failed"))).toBe(true);
  });

  it("records engine-side failure reasons", async () => {
    fetchMock
      .mockResolvedValueOnce(errJson(413, "notes.md 过大（30.0 MB），最大支持 25 MB"))
      .mockResolvedValueOnce(okJson({ recorded: true }));
    const result = await importLocalDocument(input());
    expect(result.status).toBe("failed");
    expect(result.reason).toContain("过大");
  });

  it("records an empty-text failure instead of silently succeeding", async () => {
    fetchMock
      .mockResolvedValueOnce(okJson({ sha256: "abc", status: "imported" }))
      .mockResolvedValueOnce(okJson({ recorded: true }));
    const empty = new Uint8Array(4);
    // A text-family file that decodes to whitespace exercises the empty-text
    // path; binary .pdf would fail earlier in the parser.
    const result = await importLocalDocument(
      input({ name: "scanned.txt", loadBytes: async () => empty }),
    );
    expect(result.status).toBe("failed");
    expect(result.reason).toContain("文字");
    const [failedPath, failedInit] = fetchMock.mock.calls[1];
    expect(failedPath).toContain("/documents/import-failed");
    expect(JSON.parse(failedInit.body).sha256).toBe("abc");
  });

  it("reports the stored sha256 so callers can link locations", async () => {
    fetchMock
      .mockResolvedValueOnce(okJson({ sha256: "dup-sha", status: "duplicate" }))
      .mockResolvedValueOnce(okJson({ sha256: "dup-sha", state: "ready" }));
    const result = await importLocalDocument(input());
    expect(result.sha256).toBe("dup-sha");
  });

  it("computes stable sha-256", async () => {
    expect(await sha256Hex(bytes)).toBe(
      await sha256Hex(new TextEncoder().encode("hello import slice")),
    );
    expect(await sha256Hex(bytes)).toHaveLength(64);
  });
});

describe("summarizeImportResults", () => {
  it("splits a mixed batch into failures and success count", () => {
    const results = [
      { name: "a.md", status: "imported" as const },
      { name: "b.png", status: "failed" as const, reason: "不支持的文件类型 .png" },
      { name: "c.md", status: "duplicate" as const },
      { name: "d.pdf", status: "failed" as const, reason: "解析失败" },
    ];
    expect(summarizeImportResults(results)).toEqual({
      failed: results.filter((r) => r.status === "failed"),
      succeeded: 2,
    });
  });

  it("reports a clean batch with no failures", () => {
    expect(
      summarizeImportResults([{ name: "a.md", status: "imported" }]),
    ).toEqual({ failed: [], succeeded: 1 });
  });
});

describe("importAudioDocument", () => {
  const audioInput = (over: Partial<Parameters<typeof importAudioDocument>[0]> = {}) => ({
    name: "录音.m4a",
    originalPath: "/tmp/rec.m4a",
    loadBytes: async () => bytes,
    ...over,
  });

  it("uploads to import-audio and reports imported without a text submit", async () => {
    fetchMock.mockResolvedValueOnce(
      okJson({ sha256: "aud1", status: "imported", segments: 12, duration_secs: 95.5 }),
    );

    const result = await importAudioDocument(audioInput());

    expect(result).toEqual({ name: "录音.m4a", status: "imported", sha256: "aud1" });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0];
    expect(path).toContain("/documents/import-audio?filename=");
    expect(path).toContain("original_path=%2Ftmp%2Frec.m4a");
    expect(init.method).toBe("POST");
  });

  it("reports duplicates without recording a failure", async () => {
    fetchMock.mockResolvedValueOnce(okJson({ sha256: "aud1", status: "duplicate" }));

    const result = await importAudioDocument(audioInput());
    expect(result.status).toBe("duplicate");
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("surfaces the engine failure reason (e.g. empty transcription)", async () => {
    fetchMock.mockResolvedValueOnce(errJson(422, "未转写出任何语音内容"));

    const result = await importAudioDocument(audioInput());
    expect(result.status).toBe("failed");
    expect(result.reason).toBe("未转写出任何语音内容");
    // 失败也走 /documents/import-failed，保证用户可见
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[1][0]).toContain("/documents/import-failed");
  });

  it("rejects empty audio without a network call", async () => {
    const result = await importAudioDocument(
      audioInput({ loadBytes: async () => new Uint8Array() }),
    );
    expect(result.status).toBe("failed");
    expect(result.reason).toBe("文件是空的");
    // 空文件也要在 /documents/import-failed 留一条可见失败
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toContain("/documents/import-failed");
  });
});

describe("isSupportedAudioExt", () => {
  it("accepts the phased-plan audio formats and nothing else", () => {
    for (const ext of ["mp3", "wav", "m4a", "webm"]) {
      expect(isSupportedAudioExt(ext)).toBe(true);
    }
    for (const ext of ["mp4", "mov", "pdf", "md", ""]) {
      expect(isSupportedAudioExt(ext)).toBe(false);
    }
  });
});

describe("importVideoDocument", () => {
  const videoInput = (over: Partial<Parameters<typeof importVideoDocument>[0]> = {}) => ({
    name: "demo.mp4",
    originalPath: "/tmp/demo.mp4",
    loadBytes: async () => bytes,
    ...over,
  });

  it("uploads to import-video and reports imported", async () => {
    fetchMock.mockResolvedValueOnce(
      okJson({ sha256: "vid1", status: "imported", segments: 5, duration_secs: 120.5 }),
    );

    const result = await importVideoDocument(videoInput());
    expect(result).toEqual({ name: "demo.mp4", status: "imported", sha256: "vid1" });
    const [path, init] = fetchMock.mock.calls[0];
    expect(path).toContain("/documents/import-video?filename=demo.mp4");
    expect(init.method).toBe("POST");
  });
});

describe("isSupportedVideoExt", () => {
  it("accepts video formats and rejects audio/document extensions", () => {
    for (const ext of ["mp4", "mov", "mkv"]) {
      expect(isSupportedVideoExt(ext)).toBe(true);
    }
    for (const ext of ["webm", "mp3", "m4a", "pdf", ""]) {
      expect(isSupportedVideoExt(ext)).toBe(false);
    }
  });
});
