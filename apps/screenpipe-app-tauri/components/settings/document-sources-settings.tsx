// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

"use client";

/**
 * Settings card for local document ingest: directory auto-collection
 * (add/remove watch directories, enable/disable each one, immediate
 * reconcile) and manual file import — the single home for both entry
 * points. Chat attachments and search-window drag-drop import are
 * untouched by this.
 */

import React, { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
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
import {
  ChevronDown,
  FileUp,
  FolderOpen,
  Loader2,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { useToast } from "@/components/ui/use-toast";
import {
  importAudioDocument,
  importLocalDocument,
  importVideoDocument,
  summarizeImportResults,
} from "@/lib/utils/document-import";
import {
  AUDIO_EXTS,
  DOC_PICKER_EXTENSIONS,
  VIDEO_EXTS,
  extFromName,
  isSupportedAudioExt,
  isSupportedVideoExt,
} from "@/lib/pi/extract-document";
import { localFetch } from "@/lib/api";
import { cn } from "@/lib/utils";

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

/** One row of GET /documents/list — the full import ledger, failures
 *  included (unlike the per-directory stats, which only count). */
interface DocumentRecord {
  sha256: string;
  file_name: string;
  ext: string;
  size_bytes: number;
  original_path: string | null;
  state: string;
  chunk_count: number;
  error_message: string | null;
  imported_at: string;
}

const RECORD_STATES: Record<
  string,
  { label: string; variant: "secondary" | "outline" | "destructive" }
> = {
  imported: { label: "已导入", variant: "secondary" },
  pending: { label: "待导入", variant: "outline" },
  failed: { label: "失败", variant: "destructive" },
  missing: { label: "已移动/删除", variant: "outline" },
};

function formatRelativeTime(isoString: string): string {
  const date = new Date(isoString);
  const diffMins = Math.floor((Date.now() - date.getTime()) / 60000);
  if (diffMins < 1) return "刚刚";
  if (diffMins < 60) return `${diffMins} 分钟前`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours} 小时前`;
  return `${Math.floor(diffHours / 24)} 天前`;
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function DocumentSourcesSettings() {
  const { toast } = useToast();
  const [sources, setSources] = useState<DocumentSource[]>([]);
  const [loading, setLoading] = useState(true);
  const [reconciling, setReconciling] = useState(false);
  const [adding, setAdding] = useState(false);
  const [importing, setImporting] = useState(false);
  const [recordsOpen, setRecordsOpen] = useState(false);
  const [records, setRecords] = useState<DocumentRecord[] | null>(null);
  const [recordsLoading, setRecordsLoading] = useState(false);

  const loadRecords = useCallback(async () => {
    setRecordsLoading(true);
    try {
      const resp = await localFetch("/documents/list?limit=200", {
        signal: AbortSignal.timeout(8000),
      });
      if (resp.ok) {
        const data = await resp.json();
        setRecords((data?.data ?? []) as DocumentRecord[]);
      }
    } catch {
      // Transient failures keep the previous list visible.
    } finally {
      setRecordsLoading(false);
    }
  }, []);

  const removeRecord = useCallback(
    async (sha256: string) => {
      try {
        await localFetch("/documents/remove", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ sha256 }),
        });
        setRecords((prev) => (prev ? prev.filter((r) => r.sha256 !== sha256) : prev));
        toast({ title: "已移除该文档及其可搜索内容" });
      } catch {
        toast({ title: "移除失败", variant: "destructive" });
      }
    },
    [toast],
  );

  const toggleRecords = useCallback(() => {
    setRecordsOpen((open) => {
      if (!open && records === null) void loadRecords();
      return !open;
    });
  }, [records, loadRecords]);

  // Manual file import — the same pipeline the search window's drag-drop
  // uses (pick paths → read bytes → /documents/import), relocated here from
  // the search modal so ingest entry points live in settings, not search.
  const handleImportFiles = async () => {
    if (importing) return;
    let picked: string[] | string | null = null;
    try {
      picked = await openDialog({
        multiple: true,
        filters: [
          { name: "Documents", extensions: [...DOC_PICKER_EXTENSIONS] },
          { name: "Audio", extensions: [...AUDIO_EXTS] },
          { name: "Video", extensions: [...VIDEO_EXTS] },
        ],
      });
    } catch (err) {
      console.error("document file picker error:", err);
      return;
    }
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    setImporting(true);
    try {
      const results = [];
      for (const path of paths) {
        const name = path.split(/[\\/]/).pop() || path;
        const ext = extFromName(name);
        const importFn = isSupportedAudioExt(ext)
          ? importAudioDocument
          : isSupportedVideoExt(ext)
            ? importVideoDocument
            : importLocalDocument;
        results.push(
          await importFn({
            name,
            originalPath: path,
            loadBytes: () => readFile(path),
          }),
        );
      }
      const { failed, succeeded } = summarizeImportResults(results);
      if (failed.length > 0) {
        toast({
          title: `导入失败 ${failed.length} 个文件`,
          description: failed.map((f) => `${f.name}：${f.reason}`).join("\n"),
          variant: "destructive",
        });
      }
      if (succeeded > 0) {
        const hasAudio = results.some((r) => r.status === "imported" && isSupportedAudioExt(extFromName(r.name)));
        const hasVideo = results.some((r) => r.status === "imported" && isSupportedVideoExt(extFromName(r.name)));
        toast({
          title: hasAudio || hasVideo
            ? `已导入 ${succeeded} 个文件，媒体转写完成，可在搜索中全文检索`
            : `已导入 ${succeeded} 个文档，可在搜索中全文检索`,
        });
        if (recordsOpen) void loadRecords();
      }
    } finally {
      setImporting(false);
    }
  };

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
        <CardTitle>文档采集</CardTitle>
        <CardDescription>
          添加本地文件夹持续采集其中的文档（PDF、Word、Excel、文本等），或点击「导入文档」手动导入文件；导入内容可全文搜索并回到原始文件。已导入内容不受移除目录影响。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
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
              onClick={handleImportFiles}
              variant="outline"
              size="sm"
              disabled={importing}
            >
              {importing ? (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <FileUp className="h-4 w-4 mr-2" />
              )}
              导入文档
            </Button>
          </div>
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
            还没有添加任何目录。也可点击上方「导入文档」手动导入文件；搜索窗口拖入文件同样可导入。
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

        <div className="rounded-md border">
          <button
            type="button"
            aria-expanded={recordsOpen}
            onClick={toggleRecords}
            className="flex w-full items-center gap-2 px-3 py-2 text-sm transition-colors hover:bg-muted/50"
          >
            <ChevronDown
              className={cn(
                "h-4 w-4 text-muted-foreground transition-transform",
                recordsOpen && "rotate-180",
              )}
            />
            导入记录
            {records !== null && (
              <span className="text-xs text-muted-foreground">
                共 {records.length} 条
              </span>
            )}
            {recordsOpen && (
              <span
                role="button"
                tabIndex={0}
                aria-label="刷新导入记录"
                className="ml-auto inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
                onClick={(event) => {
                  event.stopPropagation();
                  void loadRecords();
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.stopPropagation();
                    void loadRecords();
                  }
                }}
              >
                <RefreshCw
                  className={cn(
                    "h-3 w-3",
                    recordsLoading && "animate-spin",
                  )}
                />
                刷新
              </span>
            )}
          </button>
          {recordsOpen && (
            <div className="border-t">
              {recordsLoading && records === null ? (
                <p className="flex items-center gap-2 px-3 py-3 text-sm text-muted-foreground">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  加载中…
                </p>
              ) : !records || records.length === 0 ? (
                <p className="px-3 py-3 text-sm text-muted-foreground">
                  还没有导入记录。点击「导入文档」选择文件，或在搜索窗口拖入文件。
                </p>
              ) : (
                <ul className="max-h-72 divide-y overflow-y-auto">
                  {records.map((record) => {
                    const stateMeta =
                      RECORD_STATES[record.state] ?? {
                        label: record.state,
                        variant: "outline" as const,
                      };
                    return (
                      <li
                        key={record.sha256}
                        className="flex items-start gap-2.5 px-3 py-2"
                      >
                        <Badge
                          variant={stateMeta.variant}
                          className="mt-0.5 shrink-0 text-[10px]"
                        >
                          {stateMeta.label}
                        </Badge>
                        <div className="min-w-0 flex-1">
                          <p className="flex items-center gap-1.5 text-sm">
                            <span className="truncate">{record.file_name}</span>
                            {record.ext && (
                              <span className="shrink-0 rounded border border-border px-1 py-px text-[10px] text-muted-foreground">
                                {record.ext}
                              </span>
                            )}
                            {record.chunk_count > 0 && (
                              <span className="shrink-0 text-[10px] text-muted-foreground">
                                {record.chunk_count} 段
                              </span>
                            )}
                          </p>
                          {record.original_path && (
                            <p
                              className="mt-0.5 truncate text-[10px] text-muted-foreground/70"
                              title={record.original_path}
                            >
                              {record.original_path}
                            </p>
                          )}
                          {record.state === "failed" && record.error_message && (
                            <p className="mt-0.5 truncate text-xs text-destructive">
                              {record.error_message}
                            </p>
                          )}
                        </div>
                        <div className="shrink-0 text-right text-[11px] text-muted-foreground">
                          <p>{formatRelativeTime(record.imported_at)}</p>
                          <p>{formatBytes(record.size_bytes)}</p>
                        </div>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6 shrink-0"
                          aria-label="移除该文档"
                          title="移除该文档（同时删除可搜索内容与托管副本）"
                          onClick={() => {
                            if (window.confirm(`移除 ${record.file_name}？其可搜索内容与托管副本将一并删除。`)) {
                              void removeRecord(record.sha256);
                            }
                          }}
                        >
                          <X className="h-3.5 w-3.5 text-muted-foreground" />
                        </Button>
                      </li>
                    );
                  })}
                </ul>
              )}
              {records && records.length >= 200 && (
                <p className="border-t px-3 py-1.5 text-[10px] text-muted-foreground">
                  仅显示最近 200 条，更早的请在搜索（⌘K → 文档）中检索
                </p>
              )}
            </div>
          )}
        </div>
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
