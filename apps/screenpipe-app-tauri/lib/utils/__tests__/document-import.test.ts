// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  importLocalDocument,
  sha256Hex,
} from "@/lib/utils/document-import";

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

    expect(result).toEqual({ name: "notes.md", status: "imported" });
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

  it("computes stable sha-256", async () => {
    expect(await sha256Hex(bytes)).toBe(
      await sha256Hex(new TextEncoder().encode("hello import slice")),
    );
    expect(await sha256Hex(bytes)).toHaveLength(64);
  });
});
