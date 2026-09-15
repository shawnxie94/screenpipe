// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { describe, expect, it } from "vitest";

import { createDefaultSettingsObject } from "../use-settings";

describe("default settings: activities", () => {
  it("keeps automatic activities on with a 15 minute cadence (pre-Local-Brain behavior)", () => {
    const settings = createDefaultSettingsObject();

    expect(settings.activitiesEnabled).toBe(true);
    expect(settings.activitiesIntervalMinutes).toBe(15);
  });
});

describe("default settings: knowledge pipeline cadence (plan §4.2)", () => {
  it("defaults all 13 cadence keys to the plan values", () => {
    const settings = createDefaultSettingsObject();

    expect(settings.knowledgeReconcileMinutes).toBe(5);
    expect(settings.knowledgeReconcileWindowHours).toBe(2);
    expect(settings.knowledgeSummarizeMinutes).toBe(15);
    expect(settings.knowledgeWorkUnitMinutes).toBe(60);
    expect(settings.knowledgeDistillHours).toBe(24);
    expect(settings.knowledgeDistillCooldownDays).toBe(7);
    expect(settings.activityMergeGapMinutes).toBe(10);
    expect(settings.activityMinDwellSeconds).toBe(30);
    expect(settings.knowledgeDiscoveryLookbackHours).toBe(26);
    expect(settings.knowledgeDiscoveryBatch).toBe(20);
    expect(settings.knowledgeCallTimeoutSeconds).toBe(45);
    expect(settings.knowledgeStepTimeoutSeconds).toBe(120);
    expect(settings.knowledgeMaxModelCallsPerJob).toBe(3);
  });
});
