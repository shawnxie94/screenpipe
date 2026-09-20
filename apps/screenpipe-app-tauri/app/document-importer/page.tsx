// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

"use client";

/**
 * Hidden document-importer page.
 *
 * Lives in its own always-running (invisible) webview window so the existing
 * JS parsers (PDF/DOCX/XLSX via `importLocalDocument`) keep working when no
 * app window is open. The native watcher/reconciler queues one
 * `document-source-file` event per file that needs importing; this page
 * processes them sequentially and reports each outcome back through the
 * `document_source_import_result` command. Failed work is retried by the
 * native 15-minute reconcile — nothing is silently dropped.
 */

import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";
import { importLocalDocument } from "@/lib/utils/document-import";

interface SourceFileEvent {
  sourceId: string;
  path: string;
  name: string;
  size: number;
}

export default function DocumentImporterPage() {
  // One import at a time: parsers are heavy and the queue is already async.
  const busy = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    const runTask = (event: SourceFileEvent) => {
      busy.current = busy.current.then(async () => {
        try {
          const bytes = await readFile(event.path);
          const result = await importLocalDocument({
            name: event.name,
            loadBytes: async () => bytes,
            originalPath: event.path,
          });
          await invoke("document_source_import_result", {
            args: {
              sourceId: event.sourceId,
              path: event.path,
              sha256: result.sha256,
              status: result.status === "failed" ? "failed" : "imported",
              error: result.reason,
            },
          });
        } catch (err) {
          // Even read errors must land as a visible failed location.
          await invoke("document_source_import_result", {
            args: {
              sourceId: event.sourceId,
              path: event.path,
              status: "failed",
              error: err instanceof Error ? err.message : String(err),
            },
          }).catch(() => {
            // The native reconcile will retry this file.
          });
        }
      });
    };

    listen<SourceFileEvent>("document-source-file", (event) => {
      runTask(event.payload);
    }).then((un) => {
      if (cancelled) {
        un();
        return;
      }
      unlisten = un;
      // Only announce readiness once we can actually receive events, so no
      // queued import is lost between ready-flag and listener registration.
      invoke("document_importer_ready").catch(() => {});
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return null;
}
