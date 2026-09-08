// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ localFetch: vi.fn() }));
vi.mock("@/lib/api", () => ({ localFetch: mocks.localFetch }));

import {
  controlTaskRun,
  listTaskEvents,
  listTaskRuns,
  startTaskRun,
  updateTaskDefinition,
} from "./api";

function response(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

describe("unified task API", () => {
  beforeEach(() => mocks.localFetch.mockReset());

  it("starts a run with bounded, reference-only input", async () => {
    mocks.localFetch.mockResolvedValue(response({ run_id: "run-1", created: true }));

    await expect(
      startTaskRun("activity.summary", {
        input_refs: { start_time: "2026-09-08T00:00:00Z" },
        config_snapshot: { schema: "activity-history-pi-v9" },
      }),
    ).resolves.toEqual({ run_id: "run-1", created: true });
    expect(mocks.localFetch).toHaveBeenCalledWith(
      "/tasks/definitions/activity.summary/runs",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("keeps run summaries and replay events separate", async () => {
    mocks.localFetch
      .mockResolvedValueOnce(
        response({ runs: [{ run_id: "run-1", state: "running", root_run_id: "run-1", revision: 2 }] }),
      )
      .mockResolvedValueOnce(
        response({ events: [{ run_id: "run-1", seq: 3, phase: "running", event_type: "checkpoint", timestamp: "2026-09-08T00:00:01Z", safe_metadata: {}, output_refs: {} }] }),
      );

    await expect(listTaskRuns(250)).resolves.toEqual([
      { run_id: "run-1", state: "running", root_run_id: "run-1", revision: 2 },
    ]);
    await expect(listTaskEvents("run-1", 2, 1200)).resolves.toHaveLength(1);
    expect(mocks.localFetch).toHaveBeenLastCalledWith(
      "/tasks/runs/run-1/events?after_seq=2&limit=1000",
    );
  });

  it("returns the server's CAS-updated run after control", async () => {
    const run = { run_id: "run-1", state: "paused", revision: 3 };
    mocks.localFetch.mockResolvedValue(response({ run }));
    await expect(controlTaskRun("run-1", "pause", 2)).resolves.toEqual(run);
    expect(mocks.localFetch).toHaveBeenCalledWith(
      "/tasks/runs/run-1/control",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("updates a definition with an explicit revision precondition", async () => {
    const definition = {
      definition_id: "activity.summary",
      revision: 2,
      enabled: false,
    };
    mocks.localFetch.mockResolvedValue(response({ definition }));

    await expect(
      updateTaskDefinition("activity.summary", {
        expected_revision: 1,
        enabled: false,
      }),
    ).resolves.toEqual(definition);
    expect(mocks.localFetch).toHaveBeenCalledWith(
      "/tasks/definitions/activity.summary",
      expect.objectContaining({
        method: "PATCH",
        body: JSON.stringify({ expected_revision: 1, enabled: false }),
      }),
    );
  });
});
