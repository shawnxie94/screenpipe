// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Desktop office runtime: owns the opt-in 15-minute auto-sync loop and the
//! connection card service. The CLI runner stays inside `screenpipe-connect`;
//! sync runs as an independent in-process task on `OfficeService` — no
//! knowledge worker/job dependency.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use screenpipe_connect::office::types::{OfficeAuthStatus, OfficeProvider, OfficeScope};
use screenpipe_db::DatabaseManager;
use screenpipe_engine::office::OfficeService;
use tauri::AppHandle;

/// Build the OfficeService the same way the REST routes do (shared managed
/// directory), so states agree no matter which side enqueues.
pub fn office_service(app: &AppHandle) -> Option<OfficeService> {
    let data_dir = screenpipe_core::paths::default_screenpipe_data_dir();
    let _ = app;
    let db = app.state::<Arc<DatabaseManager>>();
    Some(OfficeService::new(
        (*db).clone(),
        managed_dir(&data_dir),
    ))
}

pub fn managed_dir(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("office-cli")
}

/// Spawn the opt-in auto-sync loop: every 15 minutes, for each provider with
/// `auto_sync` enabled and a saved scope, start an independent sync task.
/// Failures are visible through connection status, never silent.
pub fn start_auto_sync(app: &AppHandle) {
    let app = app.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(15 * 60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let Some(service) = office_service(&app) else {
                continue;
            };
            for provider in [OfficeProvider::Feishu, OfficeProvider::TencentMeeting] {
                let Ok(status) = service.status(provider.as_str()).await else {
                    continue;
                };
                if !auto_sync_is_eligible(&status.scope, &status.auth_status) {
                    continue;
                }
                let _ = service
                    .start_sync(provider.as_str(), status.scope_revision)
                    .await;
            }
        }
    });
}

/// Keep the scope shape referenced so future desktop-side validation shares
/// one definition with the connector layer.
pub fn scope_is_empty(scope: &OfficeScope) -> bool {
    scope.is_empty()
}

/// Auto-sync is deliberately opt-in and must never run for an unauthorised
/// connection, even if a stale scope row still has `auto_sync=true`.
pub fn auto_sync_is_eligible(scope: &OfficeScope, auth_status: &str) -> bool {
    scope.auto_sync && auth_status == OfficeAuthStatus::Authorized.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_directory_is_isolated_from_general_app_data() {
        assert_eq!(
            managed_dir(&PathBuf::from("/tmp/screenpipe")),
            PathBuf::from("/tmp/screenpipe/office-cli")
        );
    }

    #[test]
    fn scope_empty_matches_import_policy() {
        let mut scope = OfficeScope::default();
        assert!(scope_is_empty(&scope));

        scope
            .document_ids
            .push("https://example.feishu.cn/docx/abc".into());
        assert!(!scope_is_empty(&scope));

        scope.document_ids.clear();
        scope.all_accessible_meetings = true;
        assert!(!scope_is_empty(&scope));
    }

    #[test]
    fn auto_sync_requires_opt_in_and_authorization() {
        let mut scope = OfficeScope::default();
        assert!(!auto_sync_is_eligible(
            &scope,
            OfficeAuthStatus::Authorized.as_str()
        ));

        scope.auto_sync = true;
        assert!(!auto_sync_is_eligible(
            &scope,
            OfficeAuthStatus::Disconnected.as_str()
        ));
        assert!(auto_sync_is_eligible(
            &scope,
            OfficeAuthStatus::Authorized.as_str()
        ));
    }
}