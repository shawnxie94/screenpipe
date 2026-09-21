// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  mapHybridResponse,
  mapRecordsResponse,
  queryHighlightTokens,
  type SearchMatch,
  useKeywordSearchStore,
  visibleMatchingPositions,
} from "../use-keyword-search-store";
import { localFetch } from "@/lib/api";

vi.mock("@/lib/api", () => ({
  localFetch: vi.fn(),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => { resolve = res; });
  return { promise, resolve };
}

function jsonResponse(body: unknown) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function ocr(frameId: number, text: string, textSource: SearchMatch["text_source"] = "ocr") {
  return {
    type: "OCR",
    content: {
      frame_id: frameId,
      timestamp: `2026-07-30T03:27:${String(frameId).padStart(2, "0")}.000Z`,
      app_name: "Cursor",
      window_name: "Search",
      text,
      browser_url: "",
      text_source: textSource,
      text_positions: [{
        text,
        confidence: 1,
        bounds: { left: 0.1, top: 0.1, width: 0.2, height: 0.05 },
      }],
    },
  };
}

describe("unified search response mapping", () => {
  it("maps records envelope into screen, input, and unified results", () => {
    const mapped = mapRecordsResponse({
      data: [
        ocr(7, "screenpipe"),
        {
          type: "Input",
          content: {
            id: 8,
            timestamp: "2026-07-30T03:28:00.000Z",
            event_type: "clipboard",
            text_content: "screenpipe copied",
            app_name: "Cursor",
            window_title: "Search",
          },
        },
        {
          type: "Audio",
          content: {
            chunk_id: 9,
            timestamp: "2026-07-30T03:29:00.000Z",
            transcription: "meeting transcript",
            device_name: "MacBook",
          },
        },
      ],
    });

    expect(mapped.matches.map((item) => item.frame_id)).toEqual([7]);
    expect(mapped.uiEvents[0]).toMatchObject({ id: 8, text_content: "screenpipe copied" });
    expect(mapped.unifiedResults).toEqual([
      expect.objectContaining({ source_type: "audio", source_pk: "9", text: "meeting transcript" }),
    ]);
  });

  it("maps records envelope into documents and connections", () => {
    const mapped = mapRecordsResponse({
      data: [
        {
          type: "Document",
          content: {
            sha256: "doc-1",
            file_name: "release-notes.md",
            imported_at: "2026-07-30T03:29:30.000Z",
            snippet: "unified search rollout",
            text: "unified search rollout",
          },
        },
        {
          type: "Connection",
          content: {
            object_id: "meeting-1",
            provider: "feishu",
            object_kind: "meeting",
            event_at: "2026-07-30T03:29:00.000Z",
            body_text: "search review",
            title: "Search review",
          },
        },
      ],
    });

    expect(mapped.unifiedResults).toEqual([
      expect.objectContaining({
        source_type: "document",
        source_pk: "doc-1",
        timestamp: "2026-07-30T03:29:30.000Z",
        text: "unified search rollout",
      }),
      expect.objectContaining({
        source_type: "connection",
        source_pk: "meeting-1",
        timestamp: "2026-07-30T03:29:00.000Z",
        text: "search review",
      }),
    ]);
  });

  it("maps hybrid results and keeps non-OCR hits in the all-scope envelope", () => {
    const mapped = mapHybridResponse({
      degraded: false,
      legs_used: ["frames", "audio", "input", "documents", "connections", "dense"],
      results: [
        { source_type: "ocr", source_pk: "12", score: 1, legs: ["frames", "dense"], ts: "2026-07-30T03:30:00.000Z", app: "Cursor", text: "relevance" },
        { source_type: "input", source_pk: "13", score: 0.9, legs: ["input"], ts: "2026-07-30T03:30:30.000Z", app: "Cursor", window_name: "Search", text: "screenpipe copied" },
        { source_type: "connection", source_pk: "meeting-1", score: 0.8, legs: ["connections"], ts: "2026-07-30T03:31:00.000Z", app: "feishu", text: "search review" },
      ],
    });

    expect(mapped.matches.map((item) => item.frame_id)).toEqual([12]);
    expect(mapped.unifiedResults.map((item) => item.source_type)).toEqual(["input", "connection"]);
    expect(mapped.legsUsed).toEqual(["frames", "audio", "input", "documents", "connections", "dense"]);
    expect(mapped.degraded).toBe(false);
  });
});

describe("useKeywordSearchStore search scheduling", () => {
  beforeEach(() => {
    vi.mocked(localFetch).mockReset();
    useKeywordSearchStore.getState().resetSearch();
  });

  it("uses the unified records endpoint and preserves OCR results", async () => {
    const calls: string[] = [];
    vi.mocked(localFetch).mockImplementation((input) => {
      calls.push(String(input));
      return Promise.resolve(jsonResponse({ data: [ocr(1, "screenpipe")] }));
    });

    await useKeywordSearchStore.getState().searchKeywords("screenpipe", {
      limit: 24,
      offset: 0,
      analytics_surface: "standalone",
    });

    expect(calls).toHaveLength(1);
    expect(calls[0]).toContain("/search/records?");
    expect(calls[0]).toContain("content_type=ocr");
    expect(calls[0]).toContain("mode=keyword");
    expect(useKeywordSearchStore.getState().searchResults.map((item) => item.frame_id)).toEqual([1]);
  });

  it("requests relevance mode for the all scope", async () => {
    const calls: string[] = [];
    vi.mocked(localFetch).mockImplementation((input) => {
      calls.push(String(input));
      return Promise.resolve(jsonResponse({
        degraded: false,
        legs_used: ["frames", "dense"],
        results: [{ source_type: "audio", source_pk: "4", score: 1, legs: ["dense"], ts: "2026-07-30T03:30:00.000Z", text: "meeting" }],
      }));
    });

    await useKeywordSearchStore.getState().searchKeywords("meeting", {
      content_type: "all",
      mode: "relevance",
      limit: 24,
    });

    expect(calls[0]).toContain("content_type=all");
    expect(calls[0]).toContain("mode=relevance");
    expect(useKeywordSearchStore.getState().unifiedResults[0]).toMatchObject({ source_type: "audio", source_pk: "4" });
  });

  it("aborts the previous query and ignores its late response", async () => {
    const oldResponse = deferred<Response>();
    const newResponse = deferred<Response>();
    let oldSignal: AbortSignal | undefined;
    vi.mocked(localFetch).mockImplementation((input, init) => {
      const url = String(input);
      if (url.includes("old-query")) {
        oldSignal = init?.signal;
        return oldResponse.promise;
      }
      if (url.includes("new-query")) return newResponse.promise;
      return Promise.resolve(jsonResponse({ data: [] }));
    });

    const oldSearch = useKeywordSearchStore.getState().searchKeywords("old-query");
    const newSearch = useKeywordSearchStore.getState().searchKeywords("new-query");
    expect(oldSignal?.aborted).toBe(true);
    newResponse.resolve(jsonResponse({ data: [ocr(2, "new-query")] }));
    await newSearch;
    oldResponse.resolve(jsonResponse({ data: [ocr(1, "old-query")] }));
    await oldSearch;
    expect(useKeywordSearchStore.getState().searchResults.map((item) => item.frame_id)).toEqual([2]);
  });

  it("filters only accessibility candidates without re-verifying screenshot OCR", async () => {
    vi.mocked(localFetch).mockResolvedValue(jsonResponse({
      data: [ocr(1, "retention", "accessibility"), ocr(2, "retention", "ocr")],
    }));
    await useKeywordSearchStore.getState().searchKeywords("retention");
    expect(useKeywordSearchStore.getState().searchResults.map((item) => item.frame_id)).toEqual([1, 2]);
  });
});

describe("visibleMatchingPositions", () => {
  it("normalizes quoted query terms", () => {
    expect(queryHighlightTokens(`"offset" 'code'`)).toEqual(["offset", "code"]);
  });

  it("matches visible word prefixes without matching inside another word", () => {
    const positions = [
      { text: "concatenate", confidence: 1, bounds: { left: 0.1, top: 0.1, width: 0.2, height: 0.05 } },
      { text: "category", confidence: 1, bounds: { left: 0.3, top: 0.1, width: 0.1, height: 0.05 } },
      { text: "cat", confidence: 1, bounds: { left: 0.6, top: 0.1, width: 0.05, height: 0.05 } },
    ];
    expect(visibleMatchingPositions(positions, "cat")).toEqual([positions[1], positions[2]]);
  });
});
