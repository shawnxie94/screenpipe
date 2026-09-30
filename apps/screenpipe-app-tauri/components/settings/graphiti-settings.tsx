// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

"use client";

import { useCallback, useEffect, useState } from "react";
import { Network, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { appServerFetch } from "@/lib/notifications/app-server";
import type { SettingsField } from "@/components/settings/settings-search";

export interface GraphitiSettingsValue {
  adapter_url: string | null;
  allow_http_localhost: boolean;
  auto_sync_enabled: boolean;
  search_enabled: boolean;
  sync_interval_seconds: number;
}

interface GraphitiSyncStatusValue {
  version: number;
  last_attempt_at: string | null;
  status: "success" | "partial" | "failed" | null;
  delivered_count: number;
}

const EMPTY_SYNC_STATUS: GraphitiSyncStatusValue = {
  version: 1,
  last_attempt_at: null,
  status: null,
  delivered_count: 0,
};

const DEFAULT_SETTINGS: GraphitiSettingsValue = {
  adapter_url: null,
  allow_http_localhost: false,
  auto_sync_enabled: false,
  search_enabled: false,
  sync_interval_seconds: 30,
};

export const searchIndex: SettingsField[] = [
  { label: "Graphiti 适配器地址", keywords: ["图谱", "知识图谱", "adapter", "url", "endpoint"] },
  { label: "自动同步", keywords: ["activity", "活动", "sync", "同步"] },
  { label: "Graphiti 搜索", keywords: ["search", "检索", "retrieval"] },
  { label: "同步间隔", keywords: ["frequency", "频率", "seconds", "秒"] },
];

function validateAdapterUrl(raw: string, allowHttpLocalhost: boolean): string | null {
  const value = raw.trim();
  if (!value) return null;
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return "请输入有效的适配器 URL。";
  }
  if (url.username || url.password || url.search || url.hash || !["", "/"].includes(url.pathname)) {
    return "URL 不能包含账号、密码、路径、查询参数或片段。";
  }
  const host = url.hostname.toLowerCase();
  const tailnet = url.protocol === "https:" && host.length > ".ts.net".length && host.endsWith(".ts.net");
  const loopback =
    allowHttpLocalhost &&
    url.protocol === "http:" &&
    ["localhost", "127.0.0.1", "[::1]", "::1"].includes(host);
  if (!tailnet && !loopback) {
    return "仅支持 HTTPS *.ts.net；HTTP 仅在启用本机地址选项后支持。";
  }
  return null;
}

function validSyncStatus(value: unknown): value is GraphitiSyncStatusValue {
  if (!value || typeof value !== "object") return false;
  const status = value as Partial<GraphitiSyncStatusValue>;
  return (
    status.version === 1 &&
    (status.last_attempt_at === null || typeof status.last_attempt_at === "string") &&
    (status.status === null || status.status === "success" || status.status === "partial" || status.status === "failed") &&
    typeof status.delivered_count === "number" &&
    Number.isInteger(status.delivered_count) &&
    status.delivered_count >= 0
  );
}

async function readSyncStatus(): Promise<GraphitiSyncStatusValue> {
  const response = await appServerFetch("/graphiti/sync-status");
  if (!response.ok) throw new Error(`读取最近同步状态失败（${response.status}）`);
  const value: unknown = await response.json();
  if (!validSyncStatus(value)) throw new Error("最近同步状态响应格式无效。");
  return value;
}

function formatSyncTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return "时间无效";
  return new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(date);
}

function validResponse(value: unknown): value is GraphitiSettingsValue {
  if (!value || typeof value !== "object") return false;
  const settings = value as Partial<GraphitiSettingsValue>;
  return (
    (settings.adapter_url === null || typeof settings.adapter_url === "string") &&
    typeof settings.allow_http_localhost === "boolean" &&
    typeof settings.auto_sync_enabled === "boolean" &&
    typeof settings.search_enabled === "boolean" &&
    typeof settings.sync_interval_seconds === "number"
  );
}

async function responseMessage(response: Response, fallback: string): Promise<string> {
  const body = (await response.json().catch(() => ({}))) as { error?: string };
  return body.error === "invalid_graphiti_settings"
    ? "设置无效，请检查地址和同步间隔。"
    : `${fallback}（${response.status}）`;
}

export function GraphitiSettings() {
  const [settings, setSettings] = useState<GraphitiSettingsValue>(DEFAULT_SETTINGS);
  const [adapterUrl, setAdapterUrl] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [syncStatus, setSyncStatus] = useState(EMPTY_SYNC_STATUS);
  const [syncStatusLoading, setSyncStatusLoading] = useState(true);
  const [syncStatusError, setSyncStatusError] = useState<string | null>(null);

  const refreshSyncStatus = useCallback(async () => {
    setSyncStatusLoading(true);
    setSyncStatusError(null);
    try {
      setSyncStatus(await readSyncStatus());
    } catch (cause) {
      setSyncStatusError(cause instanceof Error ? cause.message : "读取最近同步状态失败。");
    } finally {
      setSyncStatusLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshSyncStatus();
  }, [refreshSyncStatus]);

  useEffect(() => {
    let active = true;
    void appServerFetch("/graphiti/settings")
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "加载 Graphiti 设置失败"));
        const value: unknown = await response.json();
        if (!validResponse(value)) throw new Error("Graphiti 设置响应格式无效。");
        if (active) {
          setSettings(value);
          setAdapterUrl(value.adapter_url ?? "");
        }
      })
      .catch((cause: unknown) => {
        if (active) setError(cause instanceof Error ? cause.message : "加载 Graphiti 设置失败。");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => { active = false; };
  }, []);

  const save = async () => {
    setError(null);
    setNotice(null);
    const urlError = validateAdapterUrl(adapterUrl, settings.allow_http_localhost);
    if (urlError) {
      setError(urlError);
      return;
    }
    if (
      (settings.auto_sync_enabled || settings.search_enabled) &&
      !adapterUrl.trim()
    ) {
      setError("启用同步或搜索前，请填写适配器地址。");
      return;
    }
    const interval = settings.sync_interval_seconds;
    if (!Number.isInteger(interval) || interval < 30 || interval > 86400) {
      setError("同步间隔必须在 30 秒至 24 小时之间。");
      return;
    }
    setBusy(true);
    try {
      const response = await appServerFetch("/graphiti/settings", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...settings, adapter_url: adapterUrl.trim() || null }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "保存 Graphiti 设置失败"));
      const value: unknown = await response.json();
      if (!validResponse(value)) throw new Error("Graphiti 设置响应格式无效。");
      setSettings(value);
      setAdapterUrl(value.adapter_url ?? "");
      setNotice("Graphiti 设置已保存并立即生效。");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "保存 Graphiti 设置失败。");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="max-w-2xl space-y-6" aria-label="Graphiti 设置">
      <header className="flex items-start gap-3">
        <Network className="mt-0.5 h-5 w-5 shrink-0 text-muted-foreground" aria-hidden="true" />
        <div>
          <h3 className="text-sm font-semibold">Graphiti 图谱记忆</h3>
          <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
            连接你配置的 Graphiti 适配器。默认关闭；启用搜索后，查询文本会发送到适配器。自动同步只发送活动总结与脱敏后的引用元数据，不发送原始屏幕或音频内容。
          </p>
        </div>
      </header>

      {loading ? (
        <p className="text-xs text-muted-foreground" role="status">正在读取本机设置…</p>
      ) : (
        <div className="space-y-5">
          <div className="space-y-2">
            <label className="block text-xs font-medium" htmlFor="graphiti-adapter-url">Graphiti 适配器地址</label>
            <Input
              id="graphiti-adapter-url"
              value={adapterUrl}
              onChange={(event) => setAdapterUrl(event.target.value)}
              placeholder="https://graphiti.example.ts.net"
              autoComplete="url"
              spellCheck={false}
              disabled={busy}
              aria-describedby="graphiti-url-policy"
            />
            <p id="graphiti-url-policy" className="text-[11px] leading-relaxed text-muted-foreground">
              仅接受 HTTPS *.ts.net 地址；不允许凭据、路径、查询参数或片段。
            </p>
          </div>

          <label className="flex items-start gap-2 text-xs">
            <input
              type="checkbox"
              checked={settings.allow_http_localhost}
              onChange={(event) => setSettings({ ...settings, allow_http_localhost: event.target.checked })}
              disabled={busy}
              aria-label="允许 HTTP 本机适配器"
            />
            <span>允许 HTTP 本机适配器（localhost、127.0.0.1 或 ::1）</span>
          </label>

          <div className="space-y-3 border-y border-border py-4">
            <label className="flex items-start gap-2 text-xs">
              <input
                type="checkbox"
                checked={settings.auto_sync_enabled}
                onChange={(event) => setSettings({ ...settings, auto_sync_enabled: event.target.checked })}
                disabled={busy}
                aria-label="启用 Graphiti 自动同步"
              />
              <span>
                <span className="block font-medium">自动同步活动总结</span>
                <span className="mt-1 block text-muted-foreground">只导出已完成且有引用的活动总结；采集与本地数据库写入不依赖网络。</span>
              </span>
            </label>
            <label className="flex items-start gap-2 text-xs">
              <input
                type="checkbox"
                checked={settings.search_enabled}
                onChange={(event) => setSettings({ ...settings, search_enabled: event.target.checked })}
                disabled={busy}
                aria-label="启用 Graphiti 搜索"
              />
              <span>
                <span className="block font-medium">允许 Graphiti 搜索</span>
                <span className="mt-1 block text-muted-foreground">通过搜索 API 的 mode=graphiti 明确选择；不可用时返回错误，不混用本地搜索结果。</span>
              </span>
            </label>
          </div>

          <div className="max-w-xs space-y-2">
            <label className="block text-xs font-medium" htmlFor="graphiti-sync-interval">同步间隔（秒）</label>
            <Input
              id="graphiti-sync-interval"
              type="number"
              min={30}
              max={86400}
              step={30}
              value={settings.sync_interval_seconds}
              onChange={(event) => setSettings({ ...settings, sync_interval_seconds: Number(event.target.value) })}
              disabled={busy}
            />
            <p className="text-[11px] text-muted-foreground">范围：30 秒至 86400 秒（24 小时）。</p>
          </div>

          <div className="space-y-3 rounded-md border border-border p-3" aria-label="最近一次自动同步">
            <div className="flex items-center justify-between gap-3">
              <h4 className="text-xs font-medium">最近一次自动同步</h4>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => void refreshSyncStatus()}
                disabled={syncStatusLoading}
                aria-label="刷新同步状态"
              >
                <RefreshCw className="mr-1 h-3 w-3" aria-hidden="true" />
                {syncStatusLoading ? "读取中…" : "刷新"}
              </Button>
            </div>
            {syncStatusLoading ? (
              <p className="text-xs text-muted-foreground">正在读取同步状态…</p>
            ) : syncStatusError ? (
              <p className="text-xs text-destructive" role="alert">{syncStatusError}</p>
            ) : syncStatus.status && syncStatus.last_attempt_at ? (
              <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
                <dt className="text-muted-foreground">时间</dt>
                <dd>{formatSyncTime(syncStatus.last_attempt_at)}</dd>
                <dt className="text-muted-foreground">结果</dt>
                <dd>{syncStatus.status === "success" ? "成功" : syncStatus.status === "partial" ? "部分成功" : "失败"}</dd>
                <dt className="text-muted-foreground">成功送达</dt>
                <dd>{syncStatus.delivered_count} 条</dd>
              </dl>
            ) : (
              <p className="text-xs text-muted-foreground">尚无自动同步记录</p>
            )}
            <p className="text-[11px] text-muted-foreground">仅显示最近一轮自动同步；成功 0 条表示本轮没有摘要送达适配器。</p>
          </div>

          <div className="flex items-center gap-3">
            <Button size="sm" onClick={() => void save()} disabled={busy}>
              {busy ? "正在保存…" : "保存设置"}
            </Button>
            {notice && <p className="text-xs text-muted-foreground" role="status">{notice}</p>}
          </div>
          {error && <p className="text-xs text-destructive" role="alert">{error}</p>}
        </div>
      )}
    </section>
  );
}
