// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

"use client";

import { useCallback, useEffect, useState } from "react";
import { BookOpen, Loader2, RefreshCw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  configureWeReadKey,
  getWeReadStatus,
  removeWeReadKey,
  saveWeReadScope,
  startWeReadSync,
  type WeReadStatus,
} from "@/lib/connections/weread";

export function WeReadConnectionPanel({ onChanged }: { onChanged?: () => void }) {
  const [status, setStatus] = useState<WeReadStatus | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadStatus = useCallback(async (): Promise<WeReadStatus | null> => {
    try {
      const next = await getWeReadStatus();
      setStatus(next);
      return next;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "加载连接状态失败");
      return null;
    }
  }, []);

  useEffect(() => {
    void loadStatus();
  }, [loadStatus]);

  const handleConnect = async () => {
    const candidate = apiKey.trim();
    if (!candidate) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await configureWeReadKey(candidate);
      setApiKey("");
      setNotice("连接成功。API Key 已保存在本机凭证存储中。");
      await loadStatus();
      onChanged?.();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "连接微信读书失败");
    } finally {
      setBusy(false);
    }
  };

  const handleRemove = async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await removeWeReadKey();
      setApiKey("");
      setNotice("已移除 API Key；已导入的本地数据会保留。");
      await loadStatus();
      onChanged?.();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "移除连接失败");
    } finally {
      setBusy(false);
    }
  };

  const handleSync = async () => {
    if (!status) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await startWeReadSync(status.scope_revision);
      setNotice("同步已开始，可在此查看进度。");
      for (let attempt = 0; attempt < 20; attempt += 1) {
        await new Promise((resolve) => setTimeout(resolve, 1000));
        const latest = await loadStatus();
        if (latest?.sync_status !== "running") break;
      }
      onChanged?.();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "启动同步失败");
    } finally {
      setBusy(false);
    }
  };

  const handleAutoSync = async (enabled: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await saveWeReadScope(enabled);
      await loadStatus();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "保存同步设置失败");
    } finally {
      setBusy(false);
    }
  };

  const connected = status?.credential_configured === true;

  return (
    <section className="space-y-3" aria-label="微信读书连接设置">
      <div className="flex items-start gap-3">
        <BookOpen className="mt-0.5 h-5 w-5 shrink-0 text-muted-foreground" aria-hidden="true" />
        <div className="min-w-0 flex-1 space-y-3">
          <div>
            <h3 className="text-sm font-semibold">微信读书</h3>
            <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
              使用微信读书 App 中「我 → 设置 → 微信读书 Skill」生成的 API Key。只读导入书架、个人划线和想法；不会下载书籍正文。
            </p>
          </div>

          {!connected ? (
            <div className="space-y-2">
              <label className="block text-xs" htmlFor="weread-api-key">API Key</label>
              <Input
                id="weread-api-key"
                type="password"
                autoComplete="off"
                spellCheck={false}
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                onKeyDown={(event) => { if (event.key === "Enter") void handleConnect(); }}
                placeholder="wrk-…"
                disabled={busy}
                aria-label="微信读书 API Key"
              />
              <Button size="sm" onClick={() => void handleConnect()} disabled={!apiKey.trim() || busy}>
                {busy && <Loader2 className="mr-1 h-3 w-3 animate-spin" />}
                验证并连接
              </Button>
              <p className="text-[11px] leading-relaxed text-muted-foreground">
                Key 仅发送给微信读书官方接口进行验证，并保存在本机凭证存储中；不会写入 screenpipe 设置或日志。接口基于 Skill 文档，稳定性以微信读书服务为准。
              </p>
            </div>
          ) : (
            <div className="flex flex-wrap items-center gap-2">
              <span className="rounded-full bg-foreground px-2 py-0.5 text-xs text-background">已连接</span>
              <Button variant="outline" size="sm" onClick={() => void handleSync()} disabled={busy || status?.sync_status === "running"}>
                <RefreshCw className={`mr-1 h-3 w-3 ${busy ? "animate-spin" : ""}`} />
                立即同步
              </Button>
              <Button variant="ghost" size="sm" onClick={() => void handleRemove()} disabled={busy}>
                <Trash2 className="mr-1 h-3 w-3" />移除连接
              </Button>
            </div>
          )}

          {status && connected && (
            <div className="space-y-2 border-t pt-3 text-xs text-muted-foreground">
              <div className="flex items-center justify-between gap-3">
                <span>同步状态：{status.sync_status}</span>
                <span>已导入 {status.imported_objects} 条</span>
              </div>
              {status.last_sync_at && <p>上次同步：{new Date(status.last_sync_at).toLocaleString()}</p>}
              {status.last_error_message && <p className="text-destructive">{status.last_error_message}</p>}
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={status.scope?.auto_sync ?? false}
                  onChange={(event) => void handleAutoSync(event.target.checked)}
                  disabled={busy}
                />
                自动同步（每 15 分钟）
              </label>
            </div>
          )}
          {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
          {notice && <p role="status" className="text-xs text-muted-foreground">{notice}</p>}
        </div>
      </div>
    </section>
  );
}
