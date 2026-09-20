// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

"use client";

/**
 * Settings card for local document directory auto-ingest: add/remove watch
 * directories, enable/disable each one, and trigger an immediate reconcile.
 * Chat attachments and manual pick/drop import are untouched by this.
 */

import React, { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { FolderOpen, Loader2, RefreshCw, Trash2 } from "lucide-react";
import { Input } from "@/components/ui/input";
import { useToast } from "@/components/ui/use-toast";

interface DocumentSource {
  id: string;
  path: string;
  enabled: boolean;
  include_exts: string;
  exclude_globs: string;
}

interface DocumentLocation {
  state: string;
}

export function DocumentSourcesSettings() {
  const { toast } = useToast();
  const [sources, setSources] = useState<DocumentSource[]>([]);
  const [loading, setLoading] = useState(true);
  const [reconciling, setReconciling] = useState(false);
  const [adding, setAdding] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const result = await invoke<{ data: DocumentSource[] }>(
        "document_sources_list",
      );
      setSources(result.data ?? []);
    } catch (err) {
      toast({
        title: "读取文档目录失败",
        description: String(err),
        variant: "destructive",
      });
    } finally {
      setLoading(false);
    }
  }, [toast]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleAdd = async () => {
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked !== "string" || adding) return;
    setAdding(true);
    try {
      await invoke("document_source_add", {
        args: { path: picked },
      });
      toast({ title: "已添加文档目录，正在后台采集" });
      await refresh();
    } catch (err) {
      toast({
        title: "添加文档目录失败",
        description: String(err),
        variant: "destructive",
      });
    } finally {
      setAdding(false);
    }
  };

  const handleToggle = async (source: DocumentSource, enabled: boolean) => {
    setSources((prev) =>
      prev.map((s) => (s.id === source.id ? { ...s, enabled } : s)),
    );
    try {
      await invoke("document_source_update", {
        args: { id: source.id, enabled },
      });
    } catch (err) {
      toast({
        title: "更新文档目录失败",
        description: String(err),
        variant: "destructive",
      });
      refresh();
    }
  };

  const handleRemove = async (source: DocumentSource) => {
    try {
      await invoke("document_source_remove", { id: source.id });
      setSources((prev) => prev.filter((s) => s.id !== source.id));
      toast({ title: "已停止采集该目录（已导入的文档保留）" });
    } catch (err) {
      toast({
        title: "移除文档目录失败",
        description: String(err),
        variant: "destructive",
      });
    }
  };

  const handleReconcile = async () => {
    setReconciling(true);
    try {
      await invoke("document_sources_reconcile_now");
      toast({ title: "对账完成" });
      await refresh();
    } catch (err) {
      toast({
        title: "对账失败",
        description: String(err),
        variant: "destructive",
      });
    } finally {
      setReconciling(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>文档目录采集</CardTitle>
        <CardDescription>
          选择本地文件夹，其中的文档（PDF、Word、Excel、文本等）会自动导入为可搜索的本地资料。已导入内容不受移除目录影响。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex items-center justify-between">
          <Button
            onClick={handleAdd}
            variant="outline"
            size="sm"
            disabled={adding}
          >
            {adding ? (
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            ) : (
              <FolderOpen className="h-4 w-4 mr-2" />
            )}
            添加目录
          </Button>
          <Button
            onClick={handleReconcile}
            variant="ghost"
            size="sm"
            disabled={reconciling}
          >
            <RefreshCw
              className={`h-3 w-3 mr-1.5 ${reconciling ? "animate-spin" : ""}`}
            />
            立即对账
          </Button>
        </div>

        {loading ? (
          <p className="text-muted-foreground text-sm">加载中…</p>
        ) : sources.length === 0 ? (
          <p className="text-muted-foreground text-sm">
            还没有添加任何目录。聊天附件与手动拖拽导入不受影响。
          </p>
        ) : (
          <ul className="space-y-2">
            {sources.map((source) => (
              <SourceRow
                key={source.id}
                source={source}
                onToggle={handleToggle}
                onRemove={handleRemove}
              />
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function SourceRow({
  source,
  onToggle,
  onRemove,
}: {
  source: DocumentSource;
  onToggle: (source: DocumentSource, enabled: boolean) => void;
  onRemove: (source: DocumentSource) => void;
}) {
  const { toast } = useToast();
  const [includeExts, setIncludeExts] = useState(source.include_exts);
  const [excludeGlobs, setExcludeGlobs] = useState(source.exclude_globs);
  const [savingFilters, setSavingFilters] = useState(false);
  const filtersChanged =
    includeExts !== source.include_exts || excludeGlobs !== source.exclude_globs;

  const saveFilters = async () => {
    setSavingFilters(true);
    try {
      await invoke("document_source_update", {
        args: {
          id: source.id,
          includeExts: includeExts.trim(),
          excludeGlobs: excludeGlobs.trim(),
        },
      });
      toast({ title: "已保存过滤规则" });
    } catch (err) {
      toast({
        title: "保存过滤规则失败",
        description: String(err),
        variant: "destructive",
      });
    } finally {
      setSavingFilters(false);
    }
  };

  const [stats, setStats] = useState<{
    imported: number;
    failed: number;
    pending: number;
    missing: number;
  } | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<{ data: DocumentLocation[] }>("document_source_locations", {
      sourceId: source.id,
    })
      .then((result) => {
        if (cancelled) return;
        const next = { imported: 0, failed: 0, pending: 0, missing: 0 };
        for (const row of result.data ?? []) {
          if (row.state in next) {
            next[row.state as keyof typeof next] += 1;
          }
        }
        setStats(next);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [source.id]);

  return (
    <li className="flex items-center gap-3 rounded-md border p-3">
      <Switch
        checked={source.enabled}
        onCheckedChange={(checked) => onToggle(source, checked)}
      />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm" title={source.path}>
          {source.path}
        </p>
        <div className="mt-2 space-y-1.5">
          <div className="flex flex-wrap items-center gap-1.5">
            <Input
              value={includeExts}
              onChange={(event) => setIncludeExts(event.target.value)}
              placeholder="扩展名白名单，如 pdf, md（留空=默认全部支持）"
              className="h-7 min-w-0 flex-1 text-xs"
            />
            <Input
              value={excludeGlobs}
              onChange={(event) => setExcludeGlobs(event.target.value)}
              placeholder="排除规则（逗号分隔的路径关键字）"
              className="h-7 min-w-0 flex-1 text-xs"
            />
          </div>
          {filtersChanged && (
            <Button
              variant="outline"
              size="sm"
              className="h-6 text-xs"
              disabled={savingFilters}
              onClick={() => void saveFilters()}
            >
              {savingFilters ? "保存中…" : "保存过滤规则"}
            </Button>
          )}
        </div>
        {stats && (
          <div className="mt-1 flex flex-wrap gap-1">
            {stats.imported > 0 && (
              <Badge variant="secondary" className="text-xs">
                已导入 {stats.imported}
              </Badge>
            )}
            {stats.pending > 0 && (
              <Badge variant="outline" className="text-xs">
                待导入 {stats.pending}
              </Badge>
            )}
            {stats.failed > 0 && (
              <Badge variant="destructive" className="text-xs">
                失败 {stats.failed}
              </Badge>
            )}
            {stats.missing > 0 && (
              <Badge variant="outline" className="text-xs">
                已移动/删除 {stats.missing}
              </Badge>
            )}
          </div>
        )}
      </div>
      <Button
        variant="ghost"
        size="icon"
        onClick={() => onRemove(source)}
        aria-label="移除目录"
      >
        <Trash2 className="h-4 w-4 text-muted-foreground" />
      </Button>
    </li>
  );
}
