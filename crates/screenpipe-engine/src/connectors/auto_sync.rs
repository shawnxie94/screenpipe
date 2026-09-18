// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Periodic auto-sync for every connector channel. Reads each connection's
//! enabled flag and kicks the same `start_sync` the manual button uses, so
//! the OCC guard, running guard, and status finalization all apply unchanged.
//! A tick that finds nothing to do (not authorized, toggle off, sync already
//! in flight, revision raced) is quiet by design.

use std::sync::Arc;
use std::time::Duration;

use screenpipe_connect::office::types::{
    OfficeAuthStatus, OfficeErrorCode, OfficeProvider, OfficeScope, OfficeSyncStatus,
};
use screenpipe_db::DatabaseManager;
use tracing::{debug, info};

use super::rss::RssService;
use crate::office::OfficeService;

/// Poll interval. `SCREENPIPE_AUTO_SYNC_SECS` overrides (0 disables);
/// default 15 minutes — matching the toggle copy in the connection cards.
pub(crate) fn auto_sync_interval() -> Duration {
    let secs = std::env::var("SCREENPIPE_AUTO_SYNC_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(900);
    Duration::from_secs(secs)
}

/// Guards evaluated before kicking a sync. Shared by both channel flavors so
/// the semantics stay identical: authorized, toggled on, nothing in flight.
pub(crate) fn should_auto_sync(
    auth_status: &str,
    auto_sync: bool,
    sync_status: &str,
) -> bool {
    auth_status == OfficeAuthStatus::Authorized.as_str()
        && auto_sync
        && sync_status != OfficeSyncStatus::Running.as_str()
        && sync_status != OfficeSyncStatus::Queued.as_str()
}

/// Spawn the loop. One tick sweeps every channel; per-channel failures are
/// logged and never cancel the sweep.
pub fn spawn_auto_sync(db: Arc<DatabaseManager>, managed_dir: std::path::PathBuf) {
    let interval = auto_sync_interval();
    if interval.is_zero() {
        info!("auto sync: disabled (SCREENPIPE_AUTO_SYNC_SECS=0)");
        return;
    }
    info!(
        "auto sync: enabled, interval {}s",
        interval.as_secs()
    );
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            let office = OfficeService::new(db.clone(), managed_dir.clone());
            for provider in [
                OfficeProvider::Feishu.as_str(),
                OfficeProvider::TencentMeeting.as_str(),
            ] {
                if let Err(e) = office_auto_sync_tick(&office, provider).await {
                    debug!(
                        "auto sync: {provider} skipped: {} {}",
                        e.code.as_str(),
                        e.message
                    );
                }
            }
            if let Err(e) = rss_auto_sync_tick(&db).await {
                debug!("auto sync: rss skipped: {e}");
            }
        }
    });
}

async fn office_auto_sync_tick(
    service: &OfficeService,
    provider: &str,
) -> Result<(), crate::office::OfficeServiceError> {
    let row = service
        .db
        .office_get_connection(provider)
        .await?
        .ok_or_else(|| {
            crate::office::OfficeServiceError::new(
                OfficeErrorCode::ScopeInvalid,
                "未连接",
                0,
            )
        })?;
    let auto_sync = service
        .db
        .office_get_scope(provider)
        .await?
        .and_then(|(json, _)| serde_json::from_str::<OfficeScope>(&json).ok())
        .map(|s| s.auto_sync)
        .unwrap_or(false);
    if !should_auto_sync(&row.auth_status, auto_sync, &row.sync_status) {
        return Ok(());
    }
    match service.start_sync(provider, row.scope_revision).await {
        Ok(run_id) => {
            info!("auto sync: {provider} kicked (run {run_id})");
            Ok(())
        }
        // 429 = a manual sync is in flight, 409 = scope revision moved since
        // the row was read; both are ordinary races, wait for the next tick.
        Err(e) if e.http == 429 || e.http == 409 => Ok(()),
        Err(e) => Err(e),
    }
}

async fn rss_auto_sync_tick(db: &Arc<DatabaseManager>) -> Result<(), String> {
    let service = RssService::new(db.clone());
    let row = service
        .status()
        .await
        .map_err(|e| e.message.clone())?;
    // The status payload carries the persisted scope (feed list + toggle);
    // auth is derived from a non-empty feed list.
    let auto_sync = row
        .get("scope")
        .and_then(|s| s.get("auto_sync"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let auth = row
        .get("auth_status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("disconnected");
    let sync_status = row
        .get("sync_status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("idle");
    if !should_auto_sync(auth, auto_sync, sync_status) {
        return Ok(());
    }
    let revision = row
        .get("scope_revision")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    match service.start_sync(revision).await {
        Ok(run_id) => {
            info!("auto sync: rss kicked (run {run_id})");
            Ok(())
        }
        Err(e) if e.http == 429 || e.http == 409 => Ok(()),
        Err(e) => Err(e.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorized_and_toggled_on_with_idle_sync_kicks() {
        assert!(should_auto_sync("authorized", true, "idle"));
        assert!(should_auto_sync("authorized", true, "partial"));
        assert!(should_auto_sync("authorized", true, "failed"));
    }

    #[test]
    fn not_authorized_or_toggled_off_never_kicks() {
        assert!(!should_auto_sync("pending", true, "idle"));
        assert!(!should_auto_sync("disconnected", true, "idle"));
        assert!(!should_auto_sync("authorized", false, "idle"));
    }

    #[test]
    fn sync_in_flight_never_kicks() {
        assert!(!should_auto_sync("authorized", true, "running"));
        assert!(!should_auto_sync("authorized", true, "queued"));
    }

    #[test]
    fn interval_defaults_to_ten_minutes_and_env_overrides() {
        // Env is process-global; assert the default parse path only when the
        // var is unset, otherwise just that it parses to something legal.
        match std::env::var("SCREENPIPE_AUTO_SYNC_SECS") {
            Err(_) => assert_eq!(auto_sync_interval(), Duration::from_secs(900)),
            Ok(v) => {
                let secs = v.parse::<u64>().unwrap_or(600);
                assert_eq!(auto_sync_interval(), Duration::from_secs(secs));
            }
        }
    }
}
