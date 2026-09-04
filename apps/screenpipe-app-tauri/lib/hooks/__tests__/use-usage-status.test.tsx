// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { createElement, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import {
  formatAllowanceLabel,
  formatAllowanceWindow,
  hostedAiAllowanceForModel,
  shouldWarnLowHostedAiAllowance,
  useUsageStatus,
  useUsageStatusQuery,
} from "../use-usage-status";

let queryClient: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return createElement(QueryClientProvider, { client: queryClient }, children);
}

describe("useUsageStatus", () => {
  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
  });

  it("does not poll a hosted usage endpoint", () => {
    const { result } = renderHook(() => useUsageStatusQuery(), { wrapper });
    expect(result.current).toMatchObject({
      usage: null,
      isLoading: false,
      isRefreshing: false,
      isUnavailable: false,
    });
    const status = renderHook(() => useUsageStatus(), { wrapper });
    expect(status.result.current).toBeNull();
  });

  it("retains allowance formatting helpers for local cached data", () => {
    const allowance = {
      lane: "frontier" as const,
      used_percent: 80,
      remaining_percent: 20,
      window_seconds: 7 * 86_400,
      technique: "fixed" as const,
      resets_at: "2026-08-17T00:00:00.000Z",
    };
    expect(formatAllowanceWindow(allowance.window_seconds)).toBe("7 天");
    expect(formatAllowanceLabel(allowance)).toBe("前沿模型 · 每周");
    expect(shouldWarnLowHostedAiAllowance(allowance)).toBe(true);
    expect(hostedAiAllowanceForModel({
      tier: "anonymous",
      used_today: 0,
      limit_today: 0,
      remaining: 0,
      resets_at: "",
      cost_limit_reached: null,
      hosted_ai: {
        plan: null,
        usage_as_of: null,
        allowances: [allowance],
        frontierModels: ["model"],
        upgrade: null,
      },
    }, "model")).toEqual(allowance);
  });
});
