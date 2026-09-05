// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import {
  ACP_ADAPTER_FLAGS,
  selectableAcpAdapters,
  type AcpAdapterInfo,
} from "@/lib/utils/preset-appearance";

/** ACP agents are a local capability — always available in the local-only build. */
export function useAcpRolloutEnabled(): boolean {
  return true;
}

/**
 * Which agents the picker may offer, resolved in one place.
 */
export function useSelectableAcpAdapters(
  currentId?: string | null,
): readonly AcpAdapterInfo[] {
  return selectableAcpAdapters(ACP_ADAPTER_FLAGS, currentId);
}

export function filterAcpPresets<T extends { provider: string }>(
  presets: T[],
  enabled: boolean,
): T[] {
  return enabled ? presets : presets.filter((preset) => preset.provider !== "acp");
}
