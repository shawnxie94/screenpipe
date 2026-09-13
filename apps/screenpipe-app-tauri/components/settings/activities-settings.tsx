// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
"use client";

import React, { useCallback, useEffect, useState } from "react";
import { AlertCircle, Clock3, ListChecks, Loader2, RefreshCw } from "lucide-react";

import { Switch } from "@/components/ui/switch";
import { useToast } from "@/components/ui/use-toast";
import { localFetch } from "@/lib/api";
import { useSettings } from "@/lib/hooks/use-settings";
import type { Settings } from "@/lib/hooks/use-settings";
import type { SettingsField } from "./settings-search";

const DEFAULT_INTERVAL_MINUTES = 15;

export const searchIndex: SettingsField[] = [
  { label: "启用活动记录", keywords: ["activity", "history", "automatic"] },
  { label: "间隔", keywords: ["activity", "frequency", "cadence", "minutes", "schedule", "legacy"] },
  { label: "知识管线节拍", keywords: ["knowledge", "pipeline", "cadence", "distill", "summarize", "reconcile"] },
  { label: "失败的知识任务", keywords: ["knowledge", "jobs", "error", "retry", "失败", "重试"], conditional: true },
];

/** One settings-driven numeric quantity: key, unit and allowed range. The
 *  defaults and ranges MUST mirror the engine's `KnowledgeCadenceConfig`
 *  (crates/screenpipe-engine/src/knowledge/cadence.rs) — the engine clamps
 *  again on every tick, this is only the editing surface. */
export interface NumericKnowledgeField {
  key: NumericSettingKey;
  label: string;
  unit: "分钟" | "小时" | "天" | "秒" | "个" | "次";
  helper: string;
  min: number;
  max: number;
}

/** The 13 knowledge-pipeline settings keys (plan §4.2) — all numeric in
 *  `settings.extra` (serde-flattened on the Rust side). */
export type NumericSettingKey =
  | "knowledgeReconcileMinutes"
  | "knowledgeReconcileWindowHours"
  | "knowledgeSummarizeMinutes"
  | "knowledgeWorkUnitMinutes"
  | "knowledgeDistillHours"
  | "knowledgeDistillCooldownDays"
  | "activityMergeGapMinutes"
  | "activityMinDwellSeconds"
  | "knowledgeDiscoveryLookbackHours"
  | "knowledgeDiscoveryBatch"
  | "knowledgeCallTimeoutSeconds"
  | "knowledgeStepTimeoutSeconds"
  | "knowledgeMaxModelCallsPerJob";

export const KNOWLEDGE_NUMBER_FIELDS: NumericKnowledgeField[] = [
  {
    key: "knowledgeReconcileMinutes",
    label: "① 间隔重建节拍",
    unit: "分钟",
    helper: "活动间隔重建的执行频率",
    min: 1,
    max: 60,
  },
  {
    key: "knowledgeReconcileWindowHours",
    label: "① 重建回看窗口",
    unit: "小时",
    helper: "每轮重建最近多久的活动",
    min: 1,
    max: 48,
  },
  {
    key: "knowledgeSummarizeMinutes",
    label: "② 间隔摘要扫描",
    unit: "分钟",
    helper: "扫描缺失摘要间隔的频率",
    min: 5,
    max: 1440,
  },
  {
    key: "knowledgeWorkUnitMinutes",
    label: "③ WorkUnit 抽取扫描",
    unit: "分钟",
    helper: "扫描可抽取间隔的频率",
    min: 5,
    max: 1440,
  },
  {
    key: "knowledgeDistillHours",
    label: "④ 知识蒸馏周期",
    unit: "小时",
    helper: "蒸馏扫描的粗粒度定时",
    min: 1,
    max: 168,
  },
  {
    key: "knowledgeDistillCooldownDays",
    label: "④ 蒸馏冷却期",
    unit: "天",
    helper: "同一分组两次蒸馏的最小间隔",
    min: 1,
    max: 90,
  },
  {
    key: "activityMergeGapMinutes",
    label: "相邻段合并上限",
    unit: "分钟",
    helper: "同对象短间隙合并的时间上限",
    min: 1,
    max: 120,
  },
  {
    key: "activityMinDwellSeconds",
    label: "短段吸收阈值",
    unit: "秒",
    helper: "低于该时长的短段可被相邻段吸收",
    min: 5,
    max: 300,
  },
  {
    key: "knowledgeDiscoveryLookbackHours",
    label: "发现回看窗口",
    unit: "小时",
    helper: "摘要与抽取扫描回看多久的历史",
    min: 1,
    max: 168,
  },
  {
    key: "knowledgeDiscoveryBatch",
    label: "每轮投递上限",
    unit: "个",
    helper: "每轮发现最多入队的任务数",
    min: 1,
    max: 500,
  },
  {
    key: "knowledgeCallTimeoutSeconds",
    label: "单次模型调用超时",
    unit: "秒",
    helper: "超时即失败，不自动重试",
    min: 10,
    max: 600,
  },
  {
    key: "knowledgeStepTimeoutSeconds",
    label: "单任务总时限",
    unit: "秒",
    helper: "超时任务置为失败终态",
    min: 30,
    max: 1800,
  },
  {
    key: "knowledgeMaxModelCallsPerJob",
    label: "每任务模型调用上限",
    unit: "次",
    helper: "超出后任务失败，不再消耗额度",
    min: 1,
    max: 10,
  },
];

/** Clamp to the field's allowed range; non-finite input falls back to the min
 *  so a cleared input can never persist NaN into settings. */
export function clampKnowledgeFieldValue(field: NumericKnowledgeField, raw: number): number {
  if (!Number.isFinite(raw)) return field.min;
  return Math.min(field.max, Math.max(field.min, Math.round(raw)));
}

/** A failed knowledge job as returned by GET /knowledge/jobs. */
export interface KnowledgeJob {
  id: number;
  kind: string;
  state: string;
  attempts: number;
  last_error_code: string | null;
  last_error_message: string | null;
  updated_at: string;
}

const KIND_LABELS: Record<string, string> = {
  extract: "WorkUnit 抽取",
  summarize: "间隔摘要",
  compile: "知识蒸馏",
  backfill_extract: "回填抽取",
};

function kindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind;
}

/** Failed knowledge jobs with their persisted error message and a manual
 *  retry entry (plan §4.2.5). Failure is terminal — this is the ONLY way a
 *  failed job runs again; there is no automatic retry. */
export function KnowledgeJobsErrorCard() {
  const { toast } = useToast();
  const [jobs, setJobs] = useState<KnowledgeJob[]>([]);
  const [loading, setLoading] = useState(true);
  const [retryingId, setRetryingId] = useState<number | null>(null);

  const load = useCallback(async () => {
    try {
      const response = await localFetch("/knowledge/jobs?state=failed&limit=20");
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const body = (await response.json()) as { jobs?: KnowledgeJob[] };
      setJobs(body.jobs ?? []);
    } catch {
      // A failed poll keeps the previous list; the next cycle retries.
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
    const interval = window.setInterval(() => void load(), 30_000);
    return () => window.clearInterval(interval);
  }, [load]);

  const retry = useCallback(
    async (job: KnowledgeJob) => {
      setRetryingId(job.id);
      try {
        const response = await localFetch(`/knowledge/jobs/${job.id}/control`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ action: "retry", expected_revision: job.updated_at }),
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        toast({ title: "已重新入队", description: `任务 #${job.id} 稍后由知识管线执行。` });
        await load();
      } catch (error) {
        toast({
          title: "重试失败",
          description: error instanceof Error ? error.message : String(error),
          variant: "destructive",
        });
      } finally {
        setRetryingId(null);
      }
    },
    [load, toast],
  );

  if (loading && jobs.length === 0) {
    return (
      <div className="border border-border px-4 py-3 text-xs text-muted-foreground" data-testid="knowledge-jobs-loading">
        正在加载知识任务…
      </div>
    );
  }

  if (jobs.length === 0) return null;

  return (
    <div className="border border-border" data-testid="knowledge-jobs-errors">
      <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
        <div className="flex items-start gap-3">
          <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-destructive" />
          <div>
            <h3 className="text-sm font-medium text-foreground">失败的知识任务</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              失败即终态，不会自动重试；确认问题后可手动重试。
            </p>
          </div>
        </div>
        <button
          type="button"
          data-testid="knowledge-jobs-refresh"
          aria-label="刷新失败任务"
          onClick={() => void load()}
          className="inline-flex h-8 w-8 items-center justify-center border border-border text-muted-foreground transition-colors hover:bg-muted"
        >
          <RefreshCw className="h-3.5 w-3.5" />
        </button>
      </div>
      <ul>
        {jobs.map((job) => (
          <li
            key={job.id}
            data-testid={`knowledge-job-error-${job.id}`}
            className="flex items-start justify-between gap-4 border-b border-border px-4 py-3 last:border-b-0"
          >
            <div className="min-w-0">
              <p className="text-xs font-medium text-foreground">
                {kindLabel(job.kind)} · 任务 #{job.id}
              </p>
              <p className="mt-1 break-words text-xs text-destructive" data-testid={`knowledge-job-error-message-${job.id}`}>
                {job.last_error_message || job.last_error_code || "未知错误"}
              </p>
            </div>
            <button
              type="button"
              data-testid={`knowledge-job-retry-${job.id}`}
              disabled={retryingId === job.id}
              onClick={() => void retry(job)}
              className="inline-flex h-7 shrink-0 items-center gap-1 border border-border px-2 text-xs text-foreground transition-colors hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
            >
              {retryingId === job.id ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : null}
              手动重试
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function ActivitiesSettings() {
  const { settings, updateSettings } = useSettings();
  const enabled = settings.activitiesEnabled ?? false;
  const intervalMinutes = settings.activitiesIntervalMinutes ?? DEFAULT_INTERVAL_MINUTES;

  return (
    <div className="space-y-5" data-testid="section-settings-activities">
      <p className="text-sm text-muted-foreground">
        控制活动记录与知识管线的节拍、回看窗口与模型预算。
      </p>

      <KnowledgeJobsErrorCard />

      <div className="border border-border bg-card">
        <div className="flex items-center justify-between gap-6 px-4 py-3">
          <div className="flex items-start gap-3">
            <ListChecks className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
            <div>
              <h3 className="text-sm font-medium text-foreground">启用活动记录</h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                从你的屏幕历史自动创建活动摘要。
              </p>
            </div>
          </div>
          <Switch
            data-testid="activities-enabled-toggle"
            checked={enabled}
            onCheckedChange={(checked) => updateSettings({ activitiesEnabled: checked })}
            aria-label="启用活动记录"
          />
        </div>

        <div className="border-t border-border px-4 py-3">
          <div className="flex items-center justify-between gap-6">
            <div className="flex items-start gap-3">
              <Clock3 className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
              <div>
                <h3 className="text-sm font-medium text-foreground">
                  间隔{" "}
                  <span
                    className="ml-1 align-middle text-[10px] uppercase tracking-wide text-muted-foreground"
                    data-testid="activities-interval-legacy-badge"
                  >
                    legacy
                  </span>
                </h3>
                <p className="mt-0.5 text-xs text-muted-foreground">
                  仅影响旧版叙事生成（已停用自动生成）；知识管线节拍见下方设置。
                </p>
              </div>
            </div>
            <select
              aria-label="活动间隔"
              value={intervalMinutes}
              disabled={!enabled}
              onChange={(event) =>
                updateSettings({ activitiesIntervalMinutes: Number(event.target.value) })
              }
              className="h-9 min-w-40 border border-border bg-background px-3 font-mono text-xs text-foreground outline-none transition-colors focus:border-foreground disabled:cursor-not-allowed disabled:opacity-50"
            >
              <option value={5}>每 5 分钟</option>
              <option value={15}>每 15 分钟</option>
              <option value={30}>每 30 分钟</option>
              <option value={60}>每小时</option>
            </select>
          </div>
        </div>
      </div>

      <div className="border border-border bg-card" data-testid="knowledge-cadence-settings">
        <div className="border-b border-border px-4 py-3">
          <h3 className="text-sm font-medium text-foreground">知识管线节拍</h3>
          <p className="mt-0.5 text-xs text-muted-foreground">
            四层各自独立调度：① 活动间隔重建、② 间隔摘要、③ WorkUnit 抽取、④ 知识蒸馏。
            改动在下一轮生效，无需重启。
          </p>
        </div>
        <ul>
          {KNOWLEDGE_NUMBER_FIELDS.map((field) => (
            <KnowledgeNumberFieldRow key={field.key} field={field} />
          ))}
        </ul>
      </div>
    </div>
  );
}

function KnowledgeNumberFieldRow({ field }: { field: NumericKnowledgeField }) {
  const { settings, updateSettings } = useSettings();
  const value = (settings[field.key] as number | undefined) ?? clampKnowledgeFieldValue(field, 0);
  const [draft, setDraft] = useState(String(value));

  // Keep the input in sync when the value changes from elsewhere (defaults
  // load async, another surface writes the same key).
  useEffect(() => {
    setDraft(String(value));
  }, [value]);

  const commit = () => {
    const parsed = Number(draft);
    const clamped = clampKnowledgeFieldValue(field, parsed);
    if (parsed !== clamped || !Number.isFinite(parsed)) {
      setDraft(String(clamped));
    }
    if (clamped !== value) {
      updateSettings({ [field.key]: clamped } as unknown as Partial<Settings>);
    }
  };

  return (
    <li className="flex items-center justify-between gap-6 border-b border-border px-4 py-3 last:border-b-0">
      <div className="min-w-0">
        <h4 className="text-sm font-medium text-foreground">{field.label}</h4>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {field.helper}（{field.min}–{field.max}
          {field.unit === "个" ? "" : ` ${field.unit}`}）
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <input
          type="number"
          data-testid={`knowledge-setting-${field.key}`}
          aria-label={`${field.label}（${field.unit}）`}
          value={draft}
          min={field.min}
          max={field.max}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commit();
            }
          }}
          className="h-9 w-24 border border-border bg-background px-3 text-right font-mono text-xs text-foreground outline-none transition-colors focus:border-foreground"
        />
        <span className="w-8 text-xs text-muted-foreground">{field.unit}</span>
      </div>
    </li>
  );
}
