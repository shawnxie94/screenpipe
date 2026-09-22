// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { expect, test } from "bun:test";
import {
  buildActivityReviewAgentPrompt,
  parseActivityHistoryResponse,
} from "./activity-review-prompt";

const range = {
  start: new Date("2026-09-22T10:00:00Z"),
  end: new Date("2026-09-22T11:00:00Z"),
};

const v2Response = JSON.stringify({
  schema_version: 2,
  entries: [
    {
      id: "research-1",
      kind: "work",
      activity_type: "research",
      project_refs: ["project-a"],
      confidence: 0.82,
      semantic_status: "inferred",
      meeting_id: null,
      start_at: "2026-09-22T10:05:00Z",
      end_at: "2026-09-22T10:20:00Z",
      title: "研究方案",
      summary: "比较了候选方案。",
      outcomes: [
        {
          type: "decision",
          status: "inferred",
          confidence: 0.7,
          provenance: "model-from-evidence",
        },
      ],
      evidence: [
        {
          kind: "screen",
          source_type: "frame",
          source_id: 42,
          occurred_at: "2026-09-22T10:10:00Z",
          at: "2026-09-22T10:10:00Z",
          frame_id: 42,
          meeting_id: null,
          app_name: "Codex",
          label: "画面显示了方案比较",
        },
      ],
    },
  ],
});

test("parses v2 semantic fields and provenance", () => {
  const document = parseActivityHistoryResponse(v2Response, range);
  expect(document.schema_version).toBe(2);
  expect(document.entries[0]).toMatchObject({
    activity_type: "research",
    project_refs: ["project-a"],
    confidence: 0.82,
    semantic_status: "inferred",
    outcomes: [
      {
        type: "decision",
        status: "inferred",
        confidence: 0.7,
        provenance: "model-from-evidence",
      },
    ],
  });
  expect(document.entries[0].evidence[0]).toMatchObject({
    source_type: "frame",
    source_id: 42,
    occurred_at: "2026-09-22T10:10:00.000Z",
  });
});

test("rejects v2 entries without the semantic contract", () => {
  expect(() =>
    parseActivityHistoryResponse(
      JSON.stringify({
        schema_version: 2,
        entries: [
          {
            id: "legacy",
            kind: "work",
            start_at: "2026-09-22T10:05:00Z",
            end_at: "2026-09-22T10:20:00Z",
            title: "旧结构",
            summary: "缺少语义字段。",
            evidence: [],
          },
        ],
      }),
      range,
    ),
  ).toThrow("not enough trustworthy evidence");
});

test("documents the v2 output contract in the prompt", () => {
  const prompt = buildActivityReviewAgentPrompt({
    start: "2026-09-22T10:00:00Z",
    end: "2026-09-22T11:00:00Z",
    label: "测试范围",
  });
  expect(prompt).toContain('"schema_version": 2');
  expect(prompt).toContain('"activity_type": "research"');
  expect(prompt).toContain('"project_refs": []');
  expect(prompt).toContain('"semantic_status": "inferred"');
  expect(prompt).toContain('"source_type": "frame"');
});
