// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Local Knowledge service layer. Owns the source registry, the deletion
//! journal/barrier, the serial KnowledgeWorker and its model executor hook, the
//! office connectors, history migration, extraction/compilation, the
//! knowledge review state machine, retrieval and grounded answers.
//!
//! The engine never opens its own SQLite writer and never resolves model
//! credentials: the desktop shell injects a [`KnowledgeModelExecutor`], and all
//! persistence flows through the shared `DatabaseManager`.

pub mod types;

use std::path::PathBuf;
use std::sync::Arc;

use screenpipe_db::DatabaseManager;

pub mod deletion;
pub mod answer;
pub mod cadence;
pub mod search;
pub mod compile;
pub mod distill;
pub mod executor;
pub mod extract;
pub mod prompts;
pub mod registry;
pub mod routes;
pub mod items;
pub mod migration;
pub mod office;
pub mod office_routes;
pub mod skill_revisions;
pub mod sources;
pub mod summarize;
pub mod trace;
pub mod worker;

pub use deletion::DeletionBarrier;
pub use deletion::DeletionService;

/// Process-wide Knowledge state shared by REST handlers and the worker. The
/// desktop shell and the engine server construct one per process.
pub struct KnowledgeShared {
    pub db: Arc<DatabaseManager>,
    /// Dataset-scoped managed state directory (`<screenpipe_dir>/knowledge`).
    pub knowledge_dir: PathBuf,
    pub barrier: Arc<DeletionBarrier>,
    /// Serializes journal sequence allocation with the following DB record.
    pub journal_lock: Arc<tokio::sync::Mutex<()>>,
    pub wave_lock: Arc<tokio::sync::Mutex<()>>,
    recovery: tokio::sync::OnceCell<Result<(), String>>,
    /// Running worker handle, injected by the desktop shell.
    pub worker: tokio::sync::RwLock<Option<Arc<worker::WorkerHandle>>>,
    /// Injected model executor (same instance the worker uses), so the
    /// interactive answer lane can run completions without credentials.
    pub executor: tokio::sync::RwLock<Option<Arc<dyn executor::KnowledgeModelExecutor>>>,
}

impl KnowledgeShared {
    pub async fn set_worker(&self, handle: Arc<worker::WorkerHandle>) {
        *self.worker.write().await = Some(handle);
    }

    pub async fn worker_handle(&self) -> Option<Arc<worker::WorkerHandle>> {
        self.worker.read().await.clone()
    }

    pub async fn set_executor(&self, executor: Arc<dyn executor::KnowledgeModelExecutor>) {
        *self.executor.write().await = Some(executor);
    }

    pub async fn executor_handle(&self) -> Option<Arc<dyn executor::KnowledgeModelExecutor>> {
        self.executor.read().await.clone()
    }
}

static SHARED: std::sync::OnceLock<Arc<KnowledgeShared>> = std::sync::OnceLock::new();

/// Process-wide instance. Retention and other non-handler call sites use
/// [`shared`] so every path agrees on one deletion barrier.
pub fn set_shared(shared: Arc<KnowledgeShared>) {
    let _ = SHARED.set(shared);
}

pub fn shared() -> Option<&'static Arc<KnowledgeShared>> {
    SHARED.get()
}

impl KnowledgeShared {
    pub fn new(db: Arc<DatabaseManager>, screenpipe_dir: PathBuf) -> Arc<Self> {
        let knowledge_dir = screenpipe_dir.join("knowledge");
        let _ = std::fs::create_dir_all(&knowledge_dir);
        Arc::new(Self {
            db,
            knowledge_dir,
            barrier: Arc::new(DeletionBarrier::new()),
            journal_lock: Arc::new(tokio::sync::Mutex::new(())),
            wave_lock: Arc::new(tokio::sync::Mutex::new(())),
            recovery: tokio::sync::OnceCell::const_new(),
            worker: tokio::sync::RwLock::new(None),
            executor: tokio::sync::RwLock::new(None),
        })
    }

    pub fn deletion(&self) -> DeletionService {
        DeletionService {
            db: self.db.clone(),
            knowledge_dir: self.knowledge_dir.clone(),
            barrier: self.barrier.clone(),
            journal_lock: self.journal_lock.clone(),
            wave_lock: self.wave_lock.clone(),
        }
    }

    /// Refresh the in-memory barrier epoch from the database (startup and
    /// after external changes such as restored backups).
    pub async fn refresh_barrier(&self) {
        let epoch = self.db.knowledge_current_deletion_epoch().await.unwrap_or(0);
        self.barrier.set_epoch(epoch.max(0) as u64);
    }

    /// Complete journal replay before any worker or HTTP route is exposed.
    /// Errors are retained so later callers fail closed instead of serving a
    /// database whose durable deletion journal was not replayed.
    pub async fn ensure_recovered(&self) -> Result<(), String> {
        let deletion = self.deletion();
        self.recovery
            .get_or_init(|| async move {
                deletion
                    .recover_incomplete()
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())?;
                self.refresh_barrier().await;
                Ok(())
            })
            .await
            .clone()
    }
}
