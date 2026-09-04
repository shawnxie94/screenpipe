// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { usePiModels } from "../use-pi-models";

let settingsState: any;
let localProviders: any[] = [];
let localProvidersError: Error | null = null;

vi.mock("@/lib/hooks/use-settings", () => ({
  useSettings: () => settingsState,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (command: string) => {
    if (command !== "pi_list_local_providers") {
      throw new Error(`unexpected invoke: ${command}`);
    }
    if (localProvidersError) throw localProvidersError;
    return localProviders;
  },
}));

describe("usePiModels", () => {
  beforeEach(() => {
    settingsState = { settings: { user: null }, isSettingsLoaded: false };
    localProviders = [];
    localProvidersError = null;
  });

  it("does not query providers before settings hydration", () => {
    const { result } = renderHook(() => usePiModels());
    expect(result.current).toMatchObject({
      piModels: [],
      localProviders: [],
      isLoading: false,
      upgradeEligible: null,
    });
  });

  it("lists providers configured in the local Pi install", async () => {
    settingsState.isSettingsLoaded = true;
    localProviders = [{
      name: "ollama",
      title: "Ollama",
      models: [{ id: "qwen3:8b", name: "qwen3:8b" }],
    }];

    const { result } = renderHook(() => usePiModels());
    await waitFor(() => expect(result.current.localProviders).toEqual(localProviders));
    expect(result.current.piModels).toEqual([]);
    expect(result.current.upgradeEligible).toBeNull();
  });

  it("keeps the picker usable when local provider discovery fails", async () => {
    settingsState.isSettingsLoaded = true;
    localProvidersError = new Error("command unavailable");

    const { result } = renderHook(() => usePiModels());
    await waitFor(() => expect(result.current.localProviders).toEqual([]));
    expect(result.current.piModels).toEqual([]);
  });
});
