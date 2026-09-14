// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  settings: {} as Record<string, unknown>,
  updateSettings: vi.fn(),
  localFetch: vi.fn(),
  toast: vi.fn(),
}));

vi.mock("@/lib/hooks/use-settings", () => ({
  useSettings: () => ({
    settings: mocks.settings,
    updateSettings: mocks.updateSettings,
  }),
}));

vi.mock("@/components/ui/use-toast", () => ({
  useToast: () => ({ toast: mocks.toast }),
}));

vi.mock("@/lib/api", () => ({
  localFetch: (...args: unknown[]) => mocks.localFetch(...args),
}));

import {
  ActivitiesSettings,
  clampKnowledgeFieldValue,
  KNOWLEDGE_NUMBER_FIELDS,
  KnowledgeJobsErrorCard,
  type KnowledgeJob,
} from "../activities-settings";

describe("knowledge pipeline setting fields (plan §4.2)", () => {
  beforeEach(() => {
    mocks.settings = {};
    mocks.updateSettings.mockReset();
    mocks.localFetch.mockReset();
    mocks.localFetch.mockImplementation(() =>
      Promise.resolve({ ok: true, json: () => Promise.resolve({ jobs: [] }) }),
    );
  });

  afterEach(() => cleanup());

  it("defines exactly the 13 configured keys", () => {
    expect(KNOWLEDGE_NUMBER_FIELDS).toHaveLength(13);
    expect(new Set(KNOWLEDGE_NUMBER_FIELDS.map((f) => f.key)).size).toBe(13);
  });

  it("mirrors the engine ranges for every key", () => {
    const ranges: Record<string, [number, number]> = {
      // key: [min, max] — must mirror cadence.rs (defaults live engine-side)
      knowledgeReconcileMinutes: [1, 60],
      knowledgeReconcileWindowHours: [1, 48],
      knowledgeSummarizeMinutes: [5, 1440],
      knowledgeWorkUnitMinutes: [5, 1440],
      knowledgeDistillHours: [1, 168],
      knowledgeDistillCooldownDays: [1, 90],
      activityMergeGapMinutes: [1, 120],
      activityMinDwellSeconds: [5, 300],
      knowledgeDiscoveryLookbackHours: [1, 168],
      knowledgeDiscoveryBatch: [1, 500],
      knowledgeCallTimeoutSeconds: [10, 600],
      knowledgeStepTimeoutSeconds: [30, 1800],
      knowledgeMaxModelCallsPerJob: [1, 10],
    };
    for (const field of KNOWLEDGE_NUMBER_FIELDS) {
      const [min, max] = ranges[field.key];
      expect([field.key, field.min, field.max]).toEqual([field.key, min, max]);
    }
  });

  it("clamps values into the allowed range and rejects NaN", () => {
    const field = KNOWLEDGE_NUMBER_FIELDS.find(
      (f) => f.key === "knowledgeReconcileMinutes",
    )!;
    expect(clampKnowledgeFieldValue(field, 5)).toBe(5);
    expect(clampKnowledgeFieldValue(field, 0)).toBe(1);
    expect(clampKnowledgeFieldValue(field, 999)).toBe(60);
    expect(clampKnowledgeFieldValue(field, Number.NaN)).toBe(field.min);
  });

  it("renders every field and commits clamped edits on blur", () => {
    render(<ActivitiesSettings />);
    for (const field of KNOWLEDGE_NUMBER_FIELDS) {
      expect(screen.getByTestId(`knowledge-setting-${field.key}`)).toBeTruthy();
    }

    const input = screen.getByTestId(
      "knowledge-setting-knowledgeReconcileMinutes",
    ) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "999" } });
    fireEvent.blur(input);
    expect(mocks.updateSettings).toHaveBeenCalledWith({
      knowledgeReconcileMinutes: 60,
    });
    expect(
      (screen.getByTestId("knowledge-setting-knowledgeReconcileMinutes") as HTMLInputElement)
        .value,
    ).toBe("60");
  });
});

describe("knowledge jobs error card (plan §4.2.5)", () => {
  const failedJob: KnowledgeJob = {
    id: 42,
    kind: "extract",
    state: "failed",
    attempts: 3,
    last_error_code: "model_error",
    last_error_message: "模型调用超时：网关 504",
    updated_at: "2026-09-12T08:00:00Z",
  };

  beforeEach(() => {
    mocks.localFetch.mockReset();
    mocks.toast.mockReset();
  });

  afterEach(() => cleanup());

  it("shows the last error message and retries via the control endpoint", async () => {
    mocks.localFetch.mockImplementation((path: unknown) => {
      if (String(path).startsWith("/knowledge/jobs?")) {
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ jobs: [failedJob] }),
        });
      }
      return Promise.resolve({ ok: true, json: () => Promise.resolve({ ok: true }) });
    });

    render(<KnowledgeJobsErrorCard />);
    const message = await screen.findByTestId("knowledge-job-error-message-42");
    expect(message.textContent).toBe("模型调用超时：网关 504");

    fireEvent.click(screen.getByTestId("knowledge-job-retry-42"));
    await waitFor(() => {
      expect(mocks.toast).toHaveBeenCalledWith(
        expect.objectContaining({ title: "已重新入队" }),
      );
    });
    const controlCall = mocks.localFetch.mock.calls.find(([path]) =>
      String(path).includes("/control"),
    );
    expect(controlCall).toBeTruthy();
    const init = controlCall![1] as RequestInit;
    expect(init.method).toBe("POST");
    expect(JSON.parse(String(init.body))).toEqual({
      action: "retry",
      expected_revision: failedJob.updated_at,
    });
  });

  it("renders nothing when there are no failed jobs", async () => {
    mocks.localFetch.mockImplementation(() =>
      Promise.resolve({ ok: true, json: () => Promise.resolve({ jobs: [] }) }),
    );
    const { container } = render(<KnowledgeJobsErrorCard />);
    await waitFor(() => {
      expect(
        container.querySelector("[data-testid='knowledge-jobs-errors']"),
      ).toBeNull();
    });
  });

  it("falls back to the error code when no message was persisted", async () => {
    mocks.localFetch.mockImplementation(() =>
      Promise.resolve({
        ok: true,
        json: () =>
          Promise.resolve({
            jobs: [{ ...failedJob, last_error_message: null }],
          }),
      }),
    );
    render(<KnowledgeJobsErrorCard />);
    const message = await screen.findByTestId("knowledge-job-error-message-42");
    expect(message.textContent).toBe("model_error");
  });
});
