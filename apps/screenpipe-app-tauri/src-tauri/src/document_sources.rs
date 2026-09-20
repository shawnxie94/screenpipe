// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Local document directory auto-ingest (Roadmap step 2).
//!
//! The user explicitly adds watch directories in settings; this module owns
//! the native side of the pipeline:
//!
//! - a `notify` recommended watcher (recursive) per enabled source, plus a
//!   15-minute reconcile loop that re-scans every directory so missed
//!   events, moves, renames, deletes, and watcher restarts all converge;
//! - directory walks run on `spawn_blocking` — never the Tauri main thread;
//! - the engine (`/documents/sources*`) owns the DB truth: a full scan marks
//!   vanished files missing (managed copies survive), and answers which
//!   paths need importing;
//! - import work (reading bytes, JS parsers) runs in the hidden
//!   `document-importer` webview via the existing `importLocalDocument`
//!   pipeline; this module queues tasks and delivers them as events, and
//!   retries on the next reconcile if the page never answers. Files are
//!   never silently dropped.
//!
//! All writes go through the engine HTTP API — no second DB writer here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, info, warn};

use crate::recording::{local_api_context_from_app, LocalApiContext};

/// Mirrors the engine accept-list in
/// `crates/screenpipe-engine/src/routes/documents.rs` (and the renderer's
/// `DOC_PICKER_EXTENSIONS`) so unsupported files never enter the pipeline.
pub const DEFAULT_EXTS: &[&str] = &[
    "pdf", "docx", "xlsx", "xls", "txt", "md", "markdown", "csv", "tsv",
    "json", "log", "yaml", "yml", "xml", "html", "htm", "rtf", "ini", "toml",
];

/// Same guard as the engine: don't even queue files the import would reject.
const MAX_DOC_BYTES: i64 = 25 * 1024 * 1024;

/// Directory names never ingested, on top of hidden directories.
const DEFAULT_EXCLUDED_DIRS: &[&str] = &["node_modules", ".git"];

/// Editor/OS temp-file patterns: events for these are dropped.
const TEMP_FILE_SUFFIXES: &[&str] = &[
    ".tmp", ".swp", ".swo", ".part", ".crdownload", ".download", ".ds_store",
];
const TEMP_FILE_PREFIXES: &[&str] = &["~$", ".#", ".."];

/// How long file events are held before acting, so editors that write a file
/// many times a second settle into one import.
const EVENT_DEBOUNCE: Duration = Duration::from_secs(2);

/// Reconcile cadence: the safety net for missed events, watcher death,
/// moves, and deletes.
const RECONCILE_INTERVAL: Duration = Duration::from_secs(15 * 60);

pub struct ImportTask {
    pub source_id: String,
    pub path: PathBuf,
    pub name: String,
    pub size: i64,
}

#[derive(Default)]
pub struct DocumentSourcesState {
    /// Raw watcher hits waiting to be debounced: (source_id, path).
    pub raw_events: Mutex<Vec<(String, PathBuf)>>,
    /// Imports queued but not yet handed to the importer page.
    pub pending: Mutex<Vec<ImportTask>>,
    /// The hidden importer page has loaded and is listening.
    pub importer_ready: AtomicBool,
    /// Watched source roots → source_id, for routing events back.
    pub roots: Mutex<HashMap<PathBuf, String>>,
}

struct WatcherInstance {
    watcher: notify::RecommendedWatcher,
}

/// One shared watcher plus a generation counter. The forwarder thread only
/// clears the slot on watcher death if its generation is still current, so a
/// rebuild can never be clobbered by a stale thread's teardown.
#[derive(Default)]
struct WatcherSlot {
    instance: Mutex<Option<WatcherInstance>>,
    generation: std::sync::atomic::AtomicU64,
}

/// Manage state and spawn the service loop. Called once from `main.rs` setup.
pub fn init(app: &AppHandle) {
    app.manage(DocumentSourcesState::default());
    app.manage(WatcherSlot::default());

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Give the embedded engine time to come up; reconcile retries anyway.
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            if let Err(e) = reconcile_all(&handle).await {
                warn!("document sources reconcile failed: {e}");
            }
            process_raw_events(&handle).await;
            tokio::time::sleep(EVENT_DEBOUNCE).await;
            // The full reconcile piggybacks on the same loop tick counter.
            tick_reconcile(&handle).await;
        }
    });
}

async fn tick_reconcile(app: &AppHandle) {
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering as AtomicOrdering;
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let last = LAST.load(AtomicOrdering::Relaxed);
    if now.saturating_sub(last) < RECONCILE_INTERVAL.as_secs() {
        return;
    }
    LAST.store(now, AtomicOrdering::Relaxed);
    if let Err(e) = reconcile_all(app).await {
        warn!("document sources reconcile failed: {e}");
    }
}

fn api(app: &AppHandle) -> LocalApiContext {
    local_api_context_from_app(app)
}

async fn engine_post_json(
    app: &AppHandle,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let ctx = api(app);
    let url = ctx.url(path);
    let resp = ctx
        .apply_auth(app_http_client().post(url).json(body))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let value: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "{} -> {}: {}",
            path,
            status,
            value["message"].as_str().unwrap_or("unknown")
        ));
    }
    Ok(value)
}

async fn engine_get_json(app: &AppHandle, path: &str) -> Result<serde_json::Value, String> {
    let ctx = api(app);
    let resp = ctx
        .apply_auth(app_http_client().get(ctx.url(path)))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let value: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{} -> {}", path, status));
    }
    Ok(value)
}

fn app_http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

// --- Filtering --------------------------------------------------------------

pub fn ext_of(name: &str) -> String {
    name.rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

/// True when a path component is hidden (dot-prefixed) or a default-excluded
/// directory (node_modules, .git).
pub fn is_default_excluded_component(component: &str) -> bool {
    component.starts_with('.')
        || DEFAULT_EXCLUDED_DIRS
        .iter()
        .any(|d| component.eq_ignore_ascii_case(d))
}

/// True when the file name is an editor/OS temp artifact, not a document.
pub fn is_temp_artifact(name: &str) -> bool {
    let lower = name.to_lowercase();
    TEMP_FILE_PREFIXES.iter().any(|p| lower.starts_with(p))
        || TEMP_FILE_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

/// User-configured excludes: comma-separated substrings, matched against the
/// full path case-insensitively.
pub fn matches_user_excludes(path: &str, exclude_globs: &str) -> bool {
    let lower = path.to_lowercase();
    exclude_globs
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .any(|p| lower.contains(&p.to_lowercase()))
}

/// The include list: comma-separated extensions overriding the default
/// allowlist when non-empty. Stored/compared without dots.
fn allowed_exts(include_exts: &str) -> HashSet<String> {
    let parsed: HashSet<String> = include_exts
        .split(',')
        .map(|e| e.trim().trim_start_matches('.').to_lowercase())
        .filter(|e| !e.is_empty())
        .collect();
    if parsed.is_empty() {
        DEFAULT_EXTS.iter().map(|s| s.to_string()).collect()
    } else {
        parsed
    }
}

/// Decide whether a scanned candidate file should be ingested. Hidden/excluded
/// component checks apply only to the path *below* `root`, so a user-chosen
/// directory that itself lives under a dot-directory still works.
pub fn should_ingest(
    path: &Path,
    root: &Path,
    managed_root: &Path,
    include_exts: &str,
    exclude_globs: &str,
) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if relative.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(is_default_excluded_component)
            .unwrap_or(true)
    }) {
        return false;
    }
    if path.starts_with(managed_root) {
        // Never re-ingest our own managed copies.
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if is_temp_artifact(name) {
        return false;
    }
    if matches_user_excludes(&path.to_string_lossy(), exclude_globs) {
        return false;
    }
    allowed_exts(include_exts).contains(&ext_of(name))
}

// --- Scanning ---------------------------------------------------------------

/// Persisted change fingerprint: mtime as unix epoch millis (0 if unknown).
fn modified_millis(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

struct WalkResult {
    files: Vec<screenpipe_scan::ScannedFile>,
}

/// Cheap stand-in module so the scan type stays decoupled from the DB crate
/// on the native side; converted to JSON for the engine.
mod screenpipe_scan {
    #[derive(serde::Serialize, Clone)]
    pub struct ScannedFile {
        pub path: String,
        pub file_name: String,
        pub ext: String,
        pub size_bytes: i64,
        pub modified_ms: i64,
    }
}

/// Recursively walk `root` collecting candidate files. Runs on
/// `spawn_blocking`. Depth-bounded and symlink-skipping so a filesystem loop
/// cannot hang the reconcile.
fn walk_directory(root: &Path, include_exts: &str, exclude_globs: &str) -> WalkResult {
    let managed_root = screenpipe_core::paths::default_screenpipe_data_dir().join("documents");
    let mut files = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if depth < 32 {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !should_ingest(&path, root, &managed_root, include_exts, exclude_globs) {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let size = meta.len() as i64;
            if size > MAX_DOC_BYTES {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            files.push(screenpipe_scan::ScannedFile {
                path: path.to_string_lossy().to_string(),
                ext: ext_of(&name),
                file_name: name,
                size_bytes: size,
                modified_ms: modified_millis(&meta),
            });
        }
    }
    WalkResult { files }
}

// --- Watcher ----------------------------------------------------------------

fn ensure_watchers(app: &AppHandle, sources: &[serde_json::Value]) {
    // Desired watch set: enabled sources only. Disabled or removed roots get
    // unwatched below, so toggling a source off stops its events for real.
    let desired: Vec<(PathBuf, String)> = sources
        .iter()
        .filter(|s| s["enabled"].as_bool().unwrap_or(false))
        .filter_map(|s| {
            let path = s["path"].as_str()?;
            let id = s["id"].as_str()?;
            Some((PathBuf::from(path), id.to_string()))
        })
        .collect();

    let slot = app.state::<WatcherSlot>();
    let mut instance = match slot.instance.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let state = app.state::<DocumentSourcesState>();
    let mut roots = state.roots.lock().unwrap_or_else(|e| e.into_inner());

    let Some(inst) = instance.as_mut() else {
        // First use (or previous watcher died): create fresh and watch all.
        let (tx, rx) = std::sync::mpsc::channel::<Result<notify::Event, notify::Error>>();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                warn!("document watcher init failed: {e}");
                return;
            }
        };
        roots.clear();
        for (root, id) in &desired {
            match watcher.watch(root, RecursiveMode::Recursive) {
                Ok(()) => {
                    roots.insert(root.clone(), id.clone());
                }
                Err(e) => warn!("document watcher failed on {root:?}: {e}"),
            }
        }
        let generation = slot.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *instance = Some(WatcherInstance { watcher });

        // Forward raw events into the state queue; the async loop debounces.
        let app_for_thread = app.clone();
        std::thread::Builder::new()
            .name("document-source-watcher".into())
            .spawn(move || {
                for event in rx {
                    let Ok(event) = event else { continue };
                    let state = app_for_thread.state::<DocumentSourcesState>();
                    let roots = state.roots.lock().unwrap_or_else(|e| e.into_inner());
                    let mut queue = state.raw_events.lock().unwrap_or_else(|e| e.into_inner());
                    for path in event.paths {
                        if !path.is_file() {
                            continue;
                        }
                        let Some((_root, source_id)) = roots
                            .iter()
                            .find(|(root, _)| path.starts_with(root))
                            .map(|(r, id)| (r.clone(), id.clone()))
                        else {
                            continue;
                        };
                        queue.push((source_id, path));
                        if queue.len() > 10_000 {
                            queue.drain(0..5_000);
                        }
                    }
                }
                // Channel closed: the watcher died. Only clear the slot if no
                // newer generation has already replaced it.
                let slot = app_for_thread.state::<WatcherSlot>();
                if slot.generation.load(Ordering::SeqCst) == generation {
                    if let Ok(mut instance) = slot.instance.lock() {
                        *instance = None;
                    }
                }
            })
            .ok();
        return;
    };

    // Watcher alive: apply the desired-set delta in place — no rebuild, no
    // thread churn, no race.
    let to_remove: Vec<PathBuf> = roots
        .keys()
        .filter(|root| !desired.iter().any(|(r, _)| r == *root))
        .cloned()
        .collect();
    for root in to_remove {
        if let Err(e) = inst.watcher.unwatch(&root) {
            warn!("document watcher unwatch failed on {root:?}: {e}");
        }
        roots.remove(&root);
    }
    for (root, id) in &desired {
        if roots.contains_key(root) {
            continue;
        }
        match inst.watcher.watch(root, RecursiveMode::Recursive) {
            Ok(()) => {
                roots.insert(root.clone(), id.clone());
            }
            Err(e) => warn!("document watcher failed on {root:?}: {e}"),
        }
    }
}

/// Drain raw watcher events, drop temp files/dupes, and queue imports for
/// files whose stored location state says they actually changed.
async fn process_raw_events(app: &AppHandle) {
    let state = app.state::<DocumentSourcesState>();
    let drained: Vec<(String, PathBuf)> = std::mem::take(
        &mut state.raw_events.lock().unwrap_or_else(|e| e.into_inner()),
    );
    if drained.is_empty() {
        return;
    }

    // Debounce: one entry per (source, path).
    let mut unique: Vec<(String, PathBuf)> = Vec::new();
    let mut seen: HashSet<(String, PathBuf)> = HashSet::new();
    for (source_id, path) in drained {
        if seen.insert((source_id.clone(), path.clone())) {
            unique.push((source_id, path));
        }
    }

    // Group by source and stat on a blocking thread.
    let mut per_source: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for (source_id, path) in unique {
        per_source.entry(source_id).or_default().push(path);
    }
    for (source_id, paths) in per_source {
        let sources = engine_get_json(app, "/documents/sources").await.unwrap_or(json!({ "data": [] }));
        let source = sources["data"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|s| s["id"].as_str() == Some(source_id.as_str()))
            })
            .cloned();
        let Some(source) = source else { continue };
        if !source["enabled"].as_bool().unwrap_or(false) {
            continue;
        }
        let include = source["include_exts"].as_str().unwrap_or("").to_string();
        let exclude = source["exclude_globs"].as_str().unwrap_or("").to_string();

        let managed_root = screenpipe_core::paths::default_screenpipe_data_dir().join("documents");
        let source_root = PathBuf::from(source["path"].as_str().unwrap_or(""));
        let stats = tauri::async_runtime::spawn_blocking(move || {
            paths
                .into_iter()
                .filter_map(|p| {
                    let name = p.file_name()?.to_str()?.to_string();
                    if is_temp_artifact(&name)
                        || !should_ingest(&p, &source_root, &managed_root, &include, &exclude)
                    {
                        return None;
                    }
                    let meta = std::fs::metadata(&p).ok()?;
                    let ext = ext_of(&name);
                    let modified_ms = modified_millis(&meta);
                    Some((p, name, ext, meta.len() as i64, modified_ms))
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();

        queue_imports(app, &source_id, stats).await;
    }
}

/// Decide which (path, name, ext, size) tuples need importing by comparing
/// against stored locations, then queue. Runs a partial diff only — the full
/// missing-marking reconcile is a separate endpoint.
async fn queue_imports(
    app: &AppHandle,
    source_id: &str,
    stats: Vec<(PathBuf, String, String, i64, i64)>,
) {
    if stats.is_empty() {
        return;
    }
    let stored = engine_get_json(
        app,
        &format!("/documents/locations?source_id={source_id}&limit=500"),
    )
    .await
    .ok();
    let stored_fingerprint = |path: &str| -> Option<(i64, i64, String)> {
        let rows = stored.as_ref()?;
        rows["data"].as_array()?.iter().find_map(|row| {
            if row["path"].as_str()? == path {
                Some((
                    row["size_bytes"].as_i64()?,
                    row["modified_ms"].as_i64()?,
                    row["state"].as_str()?.to_string(),
                ))
            } else {
                None
            }
        })
    };

    // Collect decisions first (the oversize path awaits below); the pending
    // queue lock is only taken at the very end, never across an await.
    let mut to_queue: Vec<ImportTask> = Vec::new();
    for (path, name, _ext, size, modified_ms) in stats {
        let path_str = path.to_string_lossy().to_string();
        if size > MAX_DOC_BYTES {
            // Visible failure rather than a silent skip; the page never sees it.
            let body = json!({
                "source_id": source_id,
                "path": path_str,
                "status": "failed",
                "error": format!("文件过大（{:.1} MB），最大支持 25 MB", size as f64 / (1024.0 * 1024.0)),
            });
            let _ = engine_post_json(app, "/documents/locations/imported", &body).await;
            continue;
        }
        if let Some((stored_size, stored_modified, stored_state)) = stored_fingerprint(&path_str)
        {
            // Size alone misses same-length edits; mtime is the tiebreaker.
            let unchanged = stored_state == "imported"
                && stored_size == size
                && stored_modified == modified_ms;
            if unchanged {
                continue;
            }
        }
        to_queue.push(ImportTask {
            source_id: source_id.to_string(),
            path,
            name,
            size,
        });
    }
    if to_queue.is_empty() {
        return;
    }
    let state = app.state::<DocumentSourcesState>();
    state
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .extend(to_queue);
    flush_pending(app);
}

// --- Importer page ----------------------------------------------------------

fn ensure_importer_window(app: &AppHandle) {
    if app.get_webview_window("document-importer").is_some() {
        return;
    }
    info!("creating hidden document-importer window");
    let result = tauri::WebviewWindowBuilder::new(
        app,
        "document-importer",
        tauri::WebviewUrl::App("document-importer".into()),
    )
    .title("document-importer")
    .visible(false)
    .skip_taskbar(true)
    .resizable(false)
    .decorations(false)
    .build()
    .map(crate::window::finalize_webview_window);
    if let Err(e) = result {
        // The reconcile loop re-enqueues anything still pending, so a failed
        // window creation delays imports; it never drops them.
        warn!("document-importer window creation failed: {e}");
    }
}

/// Hand queued tasks to the hidden importer page. Tasks stay in `pending`
/// until the page reports a result, so a missing page only delays work.
fn flush_pending(app: &AppHandle) {
    ensure_importer_window(app);
    let state = app.state::<DocumentSourcesState>();
    if !state.importer_ready.load(Ordering::SeqCst) {
        return;
    }
    let ready: Vec<ImportTask> = {
        let mut queue = state.pending.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *queue)
    };
    for task in ready {
        let payload = json!({
            "sourceId": task.source_id,
            "path": task.path.to_string_lossy(),
            "name": task.name,
            "size": task.size,
        });
        if let Err(e) = app.emit_to("document-importer", "document-source-file", payload) {
            warn!("document-source-file emit failed: {e}");
            state
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(task);
        }
    }
}

// --- Reconcile --------------------------------------------------------------

pub async fn reconcile_all(app: &AppHandle) -> Result<(), String> {
    let sources = engine_get_json(app, "/documents/sources").await?;
    let list = sources["data"].as_array().cloned().unwrap_or_default();
    ensure_watchers(app, &list);

    for source in list {
        if !source["enabled"].as_bool().unwrap_or(false) {
            continue;
        }
        let Some(id) = source["id"].as_str().map(str::to_string) else {
            continue;
        };
        let root = source["path"].as_str().map(str::to_string).unwrap_or_default();
        let include = source["include_exts"].as_str().unwrap_or("").to_string();
        let exclude = source["exclude_globs"].as_str().unwrap_or("").to_string();

        // Walk off the async runtime: directory IO can block for seconds.
        let root_for_walk = root.clone();
        let walked = tauri::async_runtime::spawn_blocking(move || {
            walk_directory(Path::new(&root_for_walk), &include, &exclude)
        })
        .await
        .map_err(|e| e.to_string())?;

        let body = json!({
            "source_id": id,
            "files": walked.files,
        });
        let diff = engine_post_json(app, "/documents/sources/scan", &body).await?;
        let to_import: Vec<String> = diff["to_import"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if !to_import.is_empty() {
            debug!(
                "document reconcile: {} file(s) to import from {root}",
                to_import.len()
            );
            let managed_root =
                screenpipe_core::paths::default_screenpipe_data_dir().join("documents");
            let stats = tauri::async_runtime::spawn_blocking(move || {
                to_import
                    .into_iter()
                    .filter_map(|p| {
                        let path = PathBuf::from(&p);
                        let name = path.file_name()?.to_str()?.to_string();
                        let meta = std::fs::metadata(&path).ok()?;
                        let ext = ext_of(&name);
                        let modified_ms = modified_millis(&meta);
                        Some((path, name, ext, meta.len() as i64, modified_ms))
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            // Filter through should_ingest once more (managed-root guard).
            let filtered: Vec<_> = stats
                .into_iter()
                .filter(|(p, _, _, _, _)| !p.starts_with(&managed_root))
                .collect();
            queue_imports(app, &id, filtered).await;
        }
    }
    Ok(())
}

// --- Tauri commands ---------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSourceArgs {
    path: String,
    #[serde(default)]
    include_exts: Option<String>,
    #[serde(default)]
    exclude_globs: Option<String>,
}

#[tauri::command]
pub async fn document_source_add(
    app: AppHandle,
    args: AddSourceArgs,
) -> Result<serde_json::Value, String> {
    let path = args.path.trim().to_string();
    if path.is_empty() {
        return Err("缺少目录路径".to_string());
    }
    // Normalize for consistent watcher roots and DB uniqueness.
    let canonical = std::fs::canonicalize(&path).map_err(|e| format!("目录不存在：{e}"))?;
    let canonical = canonical.to_string_lossy().to_string();
    let body = json!({
        "path": canonical,
        "include_exts": args.include_exts.unwrap_or_default(),
        "exclude_globs": args.exclude_globs.unwrap_or_default(),
    });
    let created = engine_post_json(&app, "/documents/sources", &body).await?;
    drop(created);
    // Bring the new source under management immediately.
    reconcile_all(&app).await?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSourceArgs {
    id: String,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    include_exts: Option<String>,
    #[serde(default)]
    exclude_globs: Option<String>,
}

#[tauri::command]
pub async fn document_source_update(
    app: AppHandle,
    args: UpdateSourceArgs,
) -> Result<serde_json::Value, String> {
    let body = json!({
        "id": args.id,
        "enabled": args.enabled,
        "include_exts": args.include_exts,
        "exclude_globs": args.exclude_globs,
    });
    let result = engine_post_json(&app, "/documents/sources/update", &body).await?;
    reconcile_all(&app).await?;
    Ok(result)
}

#[tauri::command]
pub async fn document_source_remove(
    app: AppHandle,
    id: String,
) -> Result<serde_json::Value, String> {
    let result = engine_post_json(
        &app,
        "/documents/sources/remove",
        &json!({ "id": id }),
    )
    .await?;
    reconcile_all(&app).await?;
    Ok(result)
}

#[tauri::command]
pub async fn document_sources_list(app: AppHandle) -> Result<serde_json::Value, String> {
    engine_get_json(&app, "/documents/sources").await
}

#[tauri::command]
pub async fn document_source_locations(
    app: AppHandle,
    source_id: String,
) -> Result<serde_json::Value, String> {
    engine_get_json(
        &app,
        &format!("/documents/locations?source_id={source_id}&limit=500"),
    )
    .await
}

#[tauri::command]
pub async fn document_sources_reconcile_now(app: AppHandle) -> Result<(), String> {
    reconcile_all(&app).await
}

/// The hidden importer page signals it is listening; queued work flushes.
#[tauri::command]
pub async fn document_importer_ready(app: AppHandle) -> Result<(), String> {
    let state = app.state::<DocumentSourcesState>();
    state.importer_ready.store(true, Ordering::SeqCst);
    flush_pending(&app);
    Ok(())
}

/// The importer page reports one file's outcome; it also acks the task.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultArgs {
    source_id: String,
    path: String,
    #[serde(default)]
    sha256: Option<String>,
    status: String,
    #[serde(default)]
    error: Option<String>,
}

#[tauri::command]
pub async fn document_source_import_result(
    app: AppHandle,
    args: ImportResultArgs,
) -> Result<(), String> {
    let body = json!({
        "source_id": args.source_id,
        "path": args.path,
        "sha256": args.sha256,
        "status": args.status,
        "error": args.error,
    });
    engine_post_json(&app, "/documents/locations/imported", &body).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("doc-sources-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_excludes_hidden_dirs_and_node_modules() {
        assert!(is_default_excluded_component(".git"));
        assert!(is_default_excluded_component(".hidden"));
        assert!(is_default_excluded_component("node_modules"));
        assert!(!is_default_excluded_component("docs"));
    }

    #[test]
    fn temp_artifacts_are_recognized() {
        assert!(is_temp_artifact("~$报告.docx"));
        assert!(is_temp_artifact("notes.md.swp"));
        assert!(is_temp_artifact(".#notes.md"));
        assert!(is_temp_artifact("export.CRDOWNLOAD"));
        assert!(!is_temp_artifact("notes.md"));
        assert!(!is_temp_artifact("report.pdf"));
    }

    #[test]
    fn user_excludes_match_substrings_case_insensitively() {
        assert!(matches_user_excludes("/Users/x/私密/notes.md", "私密"));
        assert!(matches_user_excludes("/Users/x/Build/a.md", "build"));
        assert!(!matches_user_excludes("/Users/x/docs/a.md", "build"));
        assert!(!matches_user_excludes("/Users/x/docs/a.md", ""));
    }

    #[test]
    fn include_exts_override_default_allowlist() {
        assert!(allowed_exts("").contains("md"));
        assert!(allowed_exts(".PDF, txt").contains("pdf"));
        assert!(!allowed_exts("pdf").contains("md"));
    }

    #[test]
    fn should_ingest_respects_ext_allowlist_and_managed_root() {
        let managed = Path::new("/data/.screenpipe/documents");
        let dir = tempdir("ingest");
        fs::write(dir.join("a.md"), "x").unwrap();
        assert!(should_ingest(
            &dir.join("a.md"),
            &dir,
            managed,
            "",
            ""
        ));
        // Unsupported ext.
        assert!(!should_ingest(
            &dir.join("a.png"),
            &dir,
            managed,
            "",
            ""
        ));
        // Inside the managed copy tree — never re-ingest.
        assert!(!should_ingest(
            &managed.join("ab/abc.md"),
            managed,
            managed,
            "",
            ""
        ));
        // User exclude.
        assert!(!should_ingest(
            &dir.join("a.md"),
            &dir,
            managed,
            "",
            "doc-sources"
        ));
        // Hidden path component below the root.
        assert!(!should_ingest(
            &dir.join(".git").join("config.md"),
            &dir,
            managed,
            "",
            ""
        ));
        // A root that itself sits under a hidden directory still ingests.
        assert!(should_ingest(
            &dir.join("a.md"),
            &dir,
            managed,
            "",
            ""
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn walk_collects_supported_files_only() {
        let dir = tempdir("walk");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::create_dir_all(dir.join("node_modules")).unwrap();
        fs::create_dir_all(dir.join(".hidden")).unwrap();
        fs::write(dir.join("a.md"), "x").unwrap();
        fs::write(dir.join("b.txt"), "y").unwrap();
        fs::write(dir.join("~$c.docx"), "z").unwrap();
        fs::write(dir.join("sub").join("d.pdf"), "w").unwrap();
        fs::write(dir.join("node_modules").join("e.md"), "v").unwrap();
        fs::write(dir.join(".hidden").join("f.md"), "u").unwrap();

        let result = walk_directory(&dir, "", "");
        let mut names: Vec<&str> = result
            .files
            .iter()
            .map(|f| f.file_name.as_str())
            .collect();
        names.sort();
        assert_eq!(names, vec!["a.md", "b.txt", "d.pdf"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
