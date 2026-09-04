// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

"use client";

import { useCallback } from "react";
import type { ManagedTeamSkill } from "@/lib/utils/tauri";
import {
  DEFAULT_ENTERPRISE_AI_PRESET_POLICY,
  type EnterpriseAiPresetPolicy,
  type EnterpriseManagedAiPreset,
} from "@/lib/enterprise-ai-preset-policy";
import {
  DEFAULT_ENTERPRISE_APP_UPDATE_POLICY,
  type EnterpriseAppUpdatePolicy,
} from "@/lib/enterprise/app-update-policy";
import type { ManagedPipe } from "./use-enterprise-pipes";

export type EnterpriseAuthenticationMethod = "account" | "license_key";
export type EnterpriseAuthenticationState =
  | "checking"
  | "choice"
  | "account"
  | "license_key"
  | "authenticated";

export type EnterprisePolicy = {
  hiddenSections: string[];
  lockedSettings: Record<string, unknown>;
  managedAiPreset: EnterpriseManagedAiPreset | null;
  aiPresetPolicy: EnterpriseAiPresetPolicy;
  appUpdatePolicy: EnterpriseAppUpdatePolicy;
  managedPipes: ManagedPipe[];
  managedSkills: ManagedTeamSkill[];
  orgName: string;
  requireAccountLogin: boolean;
  recordingAllowed: boolean;
};

const LOCAL_POLICY: EnterprisePolicy = {
  hiddenSections: [],
  lockedSettings: {},
  managedAiPreset: null,
  aiPresetPolicy: DEFAULT_ENTERPRISE_AI_PRESET_POLICY,
  appUpdatePolicy: DEFAULT_ENTERPRISE_APP_UPDATE_POLICY,
  managedPipes: [],
  managedSkills: [],
  orgName: "",
  requireAccountLogin: false,
  recordingAllowed: true,
};

/**
 * Personal local builds do not enroll devices, fetch organization policy, or
 * enforce remotely managed settings. Keep this compatibility-shaped hook while
 * callers are simplified incrementally, without retaining any network or
 * account side effects.
 */
export function useEnterprisePolicyRuntime() {
  const never = useCallback((_key?: string) => false, []);
  const noValue = useCallback((_key?: string): undefined => undefined, []);
  const selectAuthenticationMethod = useCallback(
    (_method: EnterpriseAuthenticationMethod) => undefined,
    [],
  );
  const submitLicenseKey = useCallback(async (_key: string) => {
    return { ok: false, error: "Enterprise licensing is not available in this local-only build." };
  }, []);

  return {
    policy: LOCAL_POLICY,
    isEnterprise: false,
    isEnterpriseBuildResolved: true,
    isEnterpriseBuildResolutionError: false,
    authenticationState: "authenticated" as EnterpriseAuthenticationState,
    authenticationError: null,
    isEnterpriseAuthenticated: true,
    isSectionHidden: never,
    isSettingLocked: never,
    getManagedValue: noValue,
    selectAuthenticationMethod,
    submitLicenseKey,
  };
}
