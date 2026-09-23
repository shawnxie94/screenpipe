// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Generic connector channel persistence (`connector_*` tables): the same
//! connection/scope/cursor/object shape as the office connector, keyed by
//! `connector` + `key` so new channels register without new migrations.

use super::*;
use chrono::{DateTime, Utc};

/// Timestamp format shared with the office tables (SQLite TEXT, RFC3339 micros).
pub fn connector_format_ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

#[derive(Debug, sqlx::FromRow)]
pub struct ConnectorConnectionRow {
    pub connector: String,
    pub key: String,
    pub enabled: i64,
    pub auto_sync: i64,
    pub auth_status: String,
    pub sync_status: String,
    pub connection_revision: i64,
    pub scope_revision: i64,
    pub last_sync_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct ConnectorObjectRow {
    pub connector: String,
    pub namespace: String,
    pub object_kind: String,
    pub object_id: String,
    pub revision: Option<String>,
    pub title: Option<String>,
    pub body_text: String,
    pub event_at: Option<String>,
    pub fetched_at: String,
    pub source_url: Option<String>,
    pub state: String,
}

/// A fetched object to persist. `namespace` scopes identity within the
/// channel (e.g. the feed URL for RSS), mirroring office's account_namespace.
/// `content_hash` (SHA-256 of the normalized body) enables cross-identity
/// dedup and no-op upserts; `None` opts out (empty bodies).
#[derive(Debug, Clone, Default)]
pub struct ConnectorObjectDraft {
    pub connector: String,
    pub namespace: String,
    pub object_kind: String,
    pub object_id: String,
    pub revision: Option<String>,
    pub title: Option<String>,
    pub body_text: String,
    pub content_hash: Option<String>,
    pub event_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub source_url: Option<String>,
}

/// Outcome of one upsert, for sync-run accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorUpsertOutcome {
    /// New row created.
    Created,
    /// Same identity with unchanged content (or user-erased suppression) —
    /// nothing written.
    Unchanged,
    /// Same content already stored under a different identity — skipped.
    Duplicate,
}

/// Partial connection update — `None` fields keep their column values.
#[derive(Debug, Clone, Default)]
pub struct ConnectorConnectionUpdate {
    pub enabled: Option<bool>,
    pub auto_sync: Option<bool>,
    pub auth_status: Option<String>,
    pub sync_status: Option<String>,
    pub bump_scope_revision: bool,
    pub bump_connection_revision: bool,
    pub last_sync_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub clear_errors: bool,
}

/// One finalized sync attempt, as surfaced in the channel status payload.
#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct ConnectorSyncRunRow {
    pub id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub processed: i64,
    pub skipped: i64,
    pub failed: i64,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

impl DatabaseManager {
    async fn connector_ensure_connection(
        &self,
        connector: &str,
        key: &str,
    ) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query("INSERT OR IGNORE INTO connector_connections (connector, key) VALUES (?1, ?2)")
            .bind(connector)
            .bind(key)
            .execute(&mut **tx.conn())
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn connector_get_connection(
        &self,
        connector: &str,
        key: &str,
    ) -> Result<Option<ConnectorConnectionRow>, SqlxError> {
        sqlx::query_as(
            "SELECT connector, key, enabled, auto_sync, auth_status, sync_status, \
             connection_revision, scope_revision, last_sync_at, last_success_at, \
             last_error_code, last_error_message \
             FROM connector_connections WHERE connector = ?1 AND key = ?2",
        )
        .bind(connector)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn connector_list_connections(
        &self,
        connector: &str,
    ) -> Result<Vec<ConnectorConnectionRow>, SqlxError> {
        sqlx::query_as(
            "SELECT connector, key, enabled, auto_sync, auth_status, sync_status, \
             connection_revision, scope_revision, last_sync_at, last_success_at, \
             last_error_code, last_error_message \
             FROM connector_connections WHERE connector = ?1 ORDER BY key",
        )
        .bind(connector)
        .fetch_all(&self.pool)
        .await
    }

    /// Connector syncs are in-process tasks — nothing can legitimately still
    /// be running after a restart, so a row marked running/queued at startup is
    /// a crashed run. Left alone, the running guard would lock the channel out
    /// of syncing forever. Returns the number of rows reset. History rows left
    /// open by the same crash are closed as failed.
    pub async fn connector_reset_stale_sync_runs(&self) -> Result<u64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "UPDATE connector_sync_runs SET status = 'failed', \
             finished_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'), \
             error_code = 'interrupted', \
             error_message = '同步进程重启，上一次运行被中断' \
             WHERE status = 'running'",
        )
        .execute(&mut **tx.conn())
        .await?;
        let result = sqlx::query(
            "UPDATE connector_connections SET sync_status = 'failed', \
             last_error_code = 'interrupted', \
             last_error_message = '同步进程重启，上一次运行被中断' \
             WHERE sync_status IN ('running', 'queued')",
        )
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }

    pub async fn connector_update_connection(
        &self,
        connector: &str,
        key: &str,
        update: ConnectorConnectionUpdate,
    ) -> Result<(), SqlxError> {
        self.connector_ensure_connection(connector, key).await?;
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "UPDATE connector_connections SET \
             enabled = COALESCE(?3, enabled), \
             auto_sync = COALESCE(?4, auto_sync), \
             auth_status = COALESCE(?5, auth_status), \
             sync_status = COALESCE(?6, sync_status), \
             connection_revision = connection_revision + ?7, \
             scope_revision = scope_revision + ?8, \
             last_sync_at = COALESCE(?9, last_sync_at), \
             last_success_at = COALESCE(?10, last_success_at), \
             last_error_code = CASE WHEN ?13 THEN NULL ELSE COALESCE(?11, last_error_code) END, \
             last_error_message = CASE WHEN ?13 THEN NULL ELSE COALESCE(?12, last_error_message) END, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
             WHERE connector = ?1 AND key = ?2",
        )
        .bind(connector)
        .bind(key)
        .bind(update.enabled.map(|v| v as i64))
        .bind(update.auto_sync.map(|v| v as i64))
        .bind(update.auth_status)
        .bind(update.sync_status)
        .bind(if update.bump_connection_revision { 1 } else { 0 })
        .bind(if update.bump_scope_revision { 1 } else { 0 })
        .bind(update.last_sync_at)
        .bind(update.last_success_at)
        .bind(update.last_error_code)
        .bind(update.last_error_message)
        .bind(update.clear_errors)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Returns (scope_json, revision).
    pub async fn connector_get_scope(
        &self,
        connector: &str,
        key: &str,
    ) -> Result<Option<(String, i64)>, SqlxError> {
        sqlx::query_as(
            "SELECT scope, revision FROM connector_scopes WHERE connector = ?1 AND key = ?2",
        )
        .bind(connector)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
    }

    /// Save scope JSON and bump the connection's scope_revision.
    pub async fn connector_save_scope(
        &self,
        connector: &str,
        key: &str,
        scope_json: &str,
    ) -> Result<i64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "INSERT INTO connector_scopes (connector, key, scope, revision) \
             VALUES (?1, ?2, ?3, 1) \
             ON CONFLICT (connector, key) DO UPDATE SET scope = ?3, \
             revision = revision + 1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(connector)
        .bind(key)
        .bind(scope_json)
        .execute(&mut **tx.conn())
        .await?;
        sqlx::query(
            "UPDATE connector_connections SET scope_revision = scope_revision + 1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
             WHERE connector = ?1 AND key = ?2",
        )
        .bind(connector)
        .bind(key)
        .execute(&mut **tx.conn())
        .await?;
        let (revision,): (i64,) = sqlx::query_as(
            "SELECT revision FROM connector_scopes WHERE connector = ?1 AND key = ?2",
        )
        .bind(connector)
        .bind(key)
        .fetch_one(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(revision)
    }

    /// Insert or update an imported object plus its FTS projection. Content
    /// unchanged since the stored hash skips the row update and FTS rewrite
    /// entirely (the common case when re-syncing a feed); new identities
    /// whose content hash already exists under a different identity are
    /// skipped as duplicates. User-erased objects (state='deleted') are never
    /// resurrected.
    pub async fn connector_upsert_object(
        &self,
        draft: &ConnectorObjectDraft,
    ) -> Result<ConnectorUpsertOutcome, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let fetched = connector_format_ts(draft.fetched_at);
        let event = draft.event_at.map(connector_format_ts);
        let normalized = draft
            .body_text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let existing: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT state, content_hash FROM connector_objects WHERE connector = ?1 \
             AND namespace = ?2 AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(&draft.connector)
        .bind(&draft.namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .fetch_optional(&mut **tx.conn())
        .await?;
        if let Some((state, stored_hash)) = existing {
            if state == "deleted" {
                tx.commit().await?;
                return Ok(ConnectorUpsertOutcome::Unchanged);
            }
            // Content unchanged: keep the original fetched_at — the row is a
            // confirmation the feed still carries it, not a new observation.
            if stored_hash.is_some() && stored_hash == draft.content_hash {
                tx.commit().await?;
                return Ok(ConnectorUpsertOutcome::Unchanged);
            }
            sqlx::query(
                "UPDATE connector_objects SET revision = ?5, title = ?6, body_text = ?7, \
                 content_hash = ?8, event_at = ?9, fetched_at = ?10, source_url = ?11, \
                 state = 'active', \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
                 WHERE connector = ?1 AND namespace = ?2 AND object_kind = ?3 AND object_id = ?4",
            )
            .bind(&draft.connector)
            .bind(&draft.namespace)
            .bind(&draft.object_kind)
            .bind(&draft.object_id)
            .bind(&draft.revision)
            .bind(&draft.title)
            .bind(&normalized)
            .bind(&draft.content_hash)
            .bind(&event)
            .bind(&fetched)
            .bind(&draft.source_url)
            .execute(&mut **tx.conn())
            .await?;
            Self::connector_replace_fts(
                &mut tx,
                &draft.connector,
                &draft.namespace,
                &draft.object_kind,
                &draft.object_id,
                &normalized,
                draft.title.as_deref(),
            )
            .await?;
            tx.commit().await?;
            return Ok(ConnectorUpsertOutcome::Unchanged);
        }
        // New identity: same content must not land twice under a different
        // guid or feed URL (syndicated posts).
        if let Some(hash) = &draft.content_hash {
            let duplicate: Option<(String,)> = sqlx::query_as(
                "SELECT object_id FROM connector_objects \
                 WHERE connector = ?1 AND content_hash = ?2 AND state != 'deleted' LIMIT 1",
            )
            .bind(&draft.connector)
            .bind(hash)
            .fetch_optional(&mut **tx.conn())
            .await?;
            if duplicate.is_some() {
                tx.commit().await?;
                return Ok(ConnectorUpsertOutcome::Duplicate);
            }
        }
        sqlx::query(
            "INSERT INTO connector_objects (connector, namespace, object_kind, object_id, \
             revision, title, body_text, content_hash, event_at, fetched_at, source_url, state) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'active')",
        )
        .bind(&draft.connector)
        .bind(&draft.namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .bind(&draft.revision)
        .bind(&draft.title)
        .bind(&normalized)
        .bind(&draft.content_hash)
        .bind(&event)
        .bind(&fetched)
        .bind(&draft.source_url)
        .execute(&mut **tx.conn())
        .await?;
        Self::connector_replace_fts(
            &mut tx,
            &draft.connector,
            &draft.namespace,
            &draft.object_kind,
            &draft.object_id,
            &normalized,
            draft.title.as_deref(),
        )
        .await?;
        tx.commit().await?;
        Ok(ConnectorUpsertOutcome::Created)
    }

    /// Rewrite the FTS projection for one object (delete + insert inside the
    /// caller's transaction).
    async fn connector_replace_fts(
        tx: &mut super::ImmediateTx,
        connector: &str,
        namespace: &str,
        object_kind: &str,
        object_id: &str,
        normalized: &str,
        title: Option<&str>,
    ) -> Result<(), SqlxError> {
        let fts_body = crate::text_normalizer::chinese_project(&format!(
            "{} {}",
            title.unwrap_or(""),
            normalized
        ));
        sqlx::query(
            "DELETE FROM connector_objects_fts WHERE connector = ?1 AND namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(connector)
        .bind(namespace)
        .bind(object_kind)
        .bind(object_id)
        .execute(&mut **tx.conn())
        .await?;
        sqlx::query(
            "INSERT INTO connector_objects_fts (body, title, connector, namespace, \
             object_kind, object_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&fts_body)
        .bind(title)
        .bind(connector)
        .bind(namespace)
        .bind(object_kind)
        .bind(object_id)
        .execute(&mut **tx.conn())
        .await?;
        Ok(())
    }

    pub async fn connector_imported_object_count(&self, connector: &str) -> Result<i64, SqlxError> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM connector_objects WHERE connector = ?1 AND state = 'active'",
        )
        .bind(connector)
        .fetch_one(&self.pool)
        .await
    }

    /// Disable every active object of the channel (retain-in-data mode on
    /// disconnect). Returns the disabled (kind, id) pairs; FTS rows are
    /// dropped so disabled content stops surfacing.
    pub async fn connector_disable_all_objects(&self, connector: &str) -> Result<u64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT namespace, object_kind, object_id FROM connector_objects \
             WHERE connector = ?1 AND state = 'active'",
        )
        .bind(connector)
        .fetch_all(&mut **tx.conn())
        .await?;
        let mut disabled = 0u64;
        for (ns, kind, id) in &rows {
            sqlx::query(
                "UPDATE connector_objects SET state = 'disabled', \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
                 WHERE connector = ?1 AND namespace = ?2 AND object_kind = ?3 AND object_id = ?4",
            )
            .bind(connector)
            .bind(ns)
            .bind(kind)
            .bind(id)
            .execute(&mut **tx.conn())
            .await?;
            sqlx::query(
                "DELETE FROM connector_objects_fts WHERE connector = ?1 AND namespace = ?2 \
                 AND object_kind = ?3 AND object_id = ?4",
            )
            .bind(connector)
            .bind(ns)
            .bind(kind)
            .bind(id)
            .execute(&mut **tx.conn())
            .await?;
            disabled += 1;
        }
        tx.commit().await?;
        Ok(disabled)
    }

    /// Erase every imported object of the channel, leaving the identity in
    /// place with state='deleted' so later re-imports are suppressed.
    pub async fn connector_erase(&self, connector: &str) -> Result<u64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT namespace, object_kind, object_id FROM connector_objects \
             WHERE connector = ?1",
        )
        .bind(connector)
        .fetch_all(&mut **tx.conn())
        .await?;
        let mut erased = 0u64;
        for (ns, kind, id) in &rows {
            sqlx::query(
                "DELETE FROM connector_objects_fts WHERE connector = ?1 AND namespace = ?2 \
                 AND object_kind = ?3 AND object_id = ?4",
            )
            .bind(connector)
            .bind(ns)
            .bind(kind)
            .bind(id)
            .execute(&mut **tx.conn())
            .await?;
            sqlx::query(
                "UPDATE connector_objects SET state = 'deleted', \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
                 WHERE connector = ?1 AND namespace = ?2 AND object_kind = ?3 AND object_id = ?4",
            )
            .bind(connector)
            .bind(ns)
            .bind(kind)
            .bind(id)
            .execute(&mut **tx.conn())
            .await?;
            erased += 1;
        }
        tx.commit().await?;
        Ok(erased)
    }

    /// Full-text search over the channel's imported objects (active only).
    pub async fn connector_search(
        &self,
        connector: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<ConnectorObjectRow>, SqlxError> {
        let projected =
            crate::text_normalizer::sanitize_fts5_query(&crate::text_normalizer::chinese_project(
                query,
            ));
        let limit = limit.max(1) as i64;
        let rows: Vec<(String, String, String, String, f64)> = sqlx::query_as(
            "SELECT f.connector, f.namespace, f.object_kind, f.object_id, \
             bm25(connector_objects_fts, 10.0) AS rank \
             FROM connector_objects_fts f \
             JOIN connector_objects o ON o.connector = f.connector \
                 AND o.namespace = f.namespace \
                 AND o.object_kind = f.object_kind \
                 AND o.object_id = f.object_id \
             WHERE o.state = 'active' AND o.connector = ?1 \
                 AND connector_objects_fts MATCH ?2 \
             ORDER BY rank LIMIT ?3",
        )
        .bind(connector)
        .bind(&projected)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for (conn, ns, kind, id, _rank) in rows {
            if let Some(row) = sqlx::query_as::<_, ConnectorObjectRow>(
                "SELECT connector, namespace, object_kind, object_id, revision, title, \
                 body_text, event_at, fetched_at, source_url, state \
                 FROM connector_objects WHERE connector = ?1 AND namespace = ?2 \
                 AND object_kind = ?3 AND object_id = ?4",
            )
            .bind(&conn)
            .bind(&ns)
            .bind(&kind)
            .bind(&id)
            .fetch_optional(&self.pool)
            .await?
            {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// Cross-channel paged search for the main `/search` surface
    /// (`content_type=connection`). Empty query = browse: most recently
    /// relevant items first. Time filters compare epoch seconds so RFC3339
    /// values written with different offsets (`Z` vs `+00:00`) order correctly.
    /// Cross-channel paged search for the main `/search` surface
    /// (`content_type=connection`). Empty query = browse: most recently
    /// relevant items first. Time filters compare epoch seconds so RFC3339
    /// values written with different offsets (`Z` vs `+00:00`) order correctly.
    /// Fixed parameter shape (`?N IS NULL OR ...`) keeps one SQL string per
    /// branch regardless of which filters are set.
    pub async fn connector_search_page(
        &self,
        query: &str,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<ConnectorObjectRow>, i64), SqlxError> {
        let limit = limit.clamp(1, 200) as i64;
        let offset = offset.max(0) as i64;
        let trimmed = query.trim();

        let time_pred =
            "AND (?1 IS NULL OR strftime('%s', COALESCE(o.event_at, o.fetched_at)) >= strftime('%s', ?1)) \
             AND (?2 IS NULL OR strftime('%s', COALESCE(o.event_at, o.fetched_at)) <= strftime('%s', ?2)) ";

        let (rows, total): (Vec<ConnectorObjectRow>, i64) = if trimmed.is_empty() {
            let page_sql = format!(
                "SELECT o.connector, o.namespace, o.object_kind, o.object_id, o.revision, \
                 o.title, o.body_text, o.event_at, o.fetched_at, o.source_url, o.state \
                 FROM connector_objects o WHERE o.state = 'active' {time_pred} \
                 ORDER BY strftime('%s', COALESCE(o.event_at, o.fetched_at)) DESC \
                 LIMIT {limit} OFFSET {offset}"
            );
            let count_sql = format!(
                "SELECT COUNT(*) FROM connector_objects o WHERE o.state = 'active' {time_pred}"
            );
            let rows =
                sqlx::query_as::<_, ConnectorObjectRow>(sqlx::AssertSqlSafe(page_sql.as_str()))
                    .bind(start_time)
                    .bind(end_time)
                    .fetch_all(&self.pool)
                    .await?;
            let total = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql.as_str()))
                .bind(start_time)
                .bind(end_time)
                .fetch_one(&self.pool)
                .await?;
            (rows, total)
        } else {
            let projected = crate::text_normalizer::sanitize_fts5_query(
                &crate::text_normalizer::chinese_project(trimmed),
            );
            let page_sql = format!(
                "SELECT o.connector, o.namespace, o.object_kind, o.object_id, o.revision, \
                 o.title, o.body_text, o.event_at, o.fetched_at, o.source_url, o.state \
                 FROM connector_objects_fts f \
                 JOIN connector_objects o ON o.connector = f.connector \
                     AND o.namespace = f.namespace \
                     AND o.object_kind = f.object_kind \
                     AND o.object_id = f.object_id \
                 WHERE o.state = 'active' AND connector_objects_fts MATCH ?3 {time_pred} \
                 GROUP BY o.connector, o.namespace, o.object_kind, o.object_id \
                 ORDER BY strftime('%s', COALESCE(o.event_at, o.fetched_at)) DESC \
                 LIMIT {limit} OFFSET {offset}"
            );
            let count_sql = format!(
                "SELECT COUNT(*) FROM (SELECT o.connector, o.namespace, o.object_kind, o.object_id \
                 FROM connector_objects_fts f \
                 JOIN connector_objects o ON o.connector = f.connector \
                     AND o.namespace = f.namespace \
                     AND o.object_kind = f.object_kind \
                     AND o.object_id = f.object_id \
                 WHERE o.state = 'active' AND connector_objects_fts MATCH ?3 {time_pred} \
                 GROUP BY o.connector, o.namespace, o.object_kind, o.object_id)"
            );
            let rows =
                sqlx::query_as::<_, ConnectorObjectRow>(sqlx::AssertSqlSafe(page_sql.as_str()))
                    .bind(start_time)
                    .bind(end_time)
                    .bind(&projected)
                    .fetch_all(&self.pool)
                    .await?;
            let total = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql.as_str()))
                .bind(start_time)
                .bind(end_time)
                .bind(&projected)
                .fetch_one(&self.pool)
                .await?;
            (rows, total)
        };

        Ok((rows, total))
    }

    /// Cross-channel full-text search ordered by event/fetch time for the
    /// unified `content_type=all&mode=time` surface.
    pub async fn connector_search_time_page(
        &self,
        query: &str,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<ConnectorObjectRow>, SqlxError> {
        let limit = limit.clamp(1, 200) as i64;
        let time_pred = "AND (?1 IS NULL OR strftime('%s', COALESCE(o.event_at, o.fetched_at)) >= strftime('%s', ?1)) AND (?2 IS NULL OR strftime('%s', COALESCE(o.event_at, o.fetched_at)) <= strftime('%s', ?2))";
        let trimmed = query.trim();
        if trimmed.is_empty() {
            let sql = format!(
                "SELECT o.connector, o.namespace, o.object_kind, o.object_id, o.revision, o.title, o.body_text, o.event_at, o.fetched_at, o.source_url, o.state FROM connector_objects o WHERE o.state = 'active' {time_pred} ORDER BY strftime('%s', COALESCE(o.event_at, o.fetched_at)) DESC LIMIT ?3",
            );
            return sqlx::query_as::<_, ConnectorObjectRow>(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(start_time)
                .bind(end_time)
                .bind(limit)
                .fetch_all(&self.pool)
                .await;
        }
        let projected = crate::text_normalizer::sanitize_fts5_query(
            &crate::text_normalizer::chinese_project(trimmed),
        );
        let sql = format!(
            "SELECT o.connector, o.namespace, o.object_kind, o.object_id, o.revision, o.title, o.body_text, o.event_at, o.fetched_at, o.source_url, o.state FROM connector_objects_fts f JOIN connector_objects o ON o.connector = f.connector AND o.namespace = f.namespace AND o.object_kind = f.object_kind AND o.object_id = f.object_id WHERE o.state = 'active' AND connector_objects_fts MATCH ?3 {time_pred} GROUP BY o.connector, o.namespace, o.object_kind, o.object_id ORDER BY strftime('%s', COALESCE(o.event_at, o.fetched_at)) DESC LIMIT ?4",
        );
        sqlx::query_as::<_, ConnectorObjectRow>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(start_time)
            .bind(end_time)
            .bind(projected)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
    }

    pub async fn connector_search_time_count(
        &self,
        query: &str,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<i64, SqlxError> {
        let time_pred = "AND (?1 IS NULL OR strftime('%s', COALESCE(o.event_at, o.fetched_at)) >= strftime('%s', ?1)) AND (?2 IS NULL OR strftime('%s', COALESCE(o.event_at, o.fetched_at)) <= strftime('%s', ?2))";
        let trimmed = query.trim();
        if trimmed.is_empty() {
            let sql = format!(
                "SELECT COUNT(*) FROM connector_objects o WHERE o.state = 'active' {time_pred}",
            );
            return sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(start_time)
                .bind(end_time)
                .fetch_one(&self.pool)
                .await;
        }
        let projected = crate::text_normalizer::sanitize_fts5_query(
            &crate::text_normalizer::chinese_project(trimmed),
        );
        let sql = format!(
            "SELECT COUNT(*) FROM (SELECT o.connector, o.namespace, o.object_kind, o.object_id FROM connector_objects_fts f JOIN connector_objects o ON o.connector = f.connector AND o.namespace = f.namespace AND o.object_kind = f.object_kind AND o.object_id = f.object_id WHERE o.state = 'active' AND connector_objects_fts MATCH ?3 {time_pred} GROUP BY o.connector, o.namespace, o.object_kind, o.object_id)",
        );
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(start_time)
            .bind(end_time)
            .bind(projected)
            .fetch_one(&self.pool)
            .await
    }

    pub async fn connector_get_cursor(
        &self,
        connector: &str,
        key: &str,
        cursor_key: &str,
    ) -> Result<Option<String>, SqlxError> {
        sqlx::query_scalar(
            "SELECT cursor_value FROM connector_cursors \
             WHERE connector = ?1 AND key = ?2 AND cursor_key = ?3",
        )
        .bind(connector)
        .bind(key)
        .bind(cursor_key)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn connector_set_cursor(
        &self,
        connector: &str,
        key: &str,
        cursor_key: &str,
        cursor_value: &str,
    ) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "INSERT INTO connector_cursors (connector, key, cursor_key, cursor_value) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (connector, key, cursor_key) DO UPDATE SET cursor_value = ?4, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(connector)
        .bind(key)
        .bind(cursor_key)
        .bind(cursor_value)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn connector_clear_cursors(&self, connector: &str) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query("DELETE FROM connector_cursors WHERE connector = ?1")
            .bind(connector)
            .execute(&mut **tx.conn())
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Open a sync-run row and return its id. One row per sync attempt —
    /// answers "what did the last sync actually do" from history.
    pub async fn connector_sync_run_start(
        &self,
        connector: &str,
        key: &str,
    ) -> Result<i64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO connector_sync_runs (connector, key, started_at) \
             VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ','now')) RETURNING id",
        )
        .bind(connector)
        .bind(key)
        .fetch_one(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Finalize a sync-run row with its outcome counts.
    pub async fn connector_sync_run_finish(
        &self,
        id: i64,
        status: &str,
        processed: i64,
        skipped: i64,
        failed: i64,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "UPDATE connector_sync_runs SET finished_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'), \
             status = ?2, processed = ?3, skipped = ?4, failed = ?5, \
             error_code = ?6, error_message = ?7 WHERE id = ?1",
        )
        .bind(id)
        .bind(status)
        .bind(processed)
        .bind(skipped)
        .bind(failed)
        .bind(error_code)
        .bind(error_message)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Most recent sync runs for one channel, newest first.
    pub async fn connector_sync_runs_recent(
        &self,
        connector: &str,
        key: &str,
        limit: u32,
    ) -> Result<Vec<ConnectorSyncRunRow>, SqlxError> {
        sqlx::query_as(
            "SELECT id, started_at, finished_at, status, processed, skipped, failed, \
             error_code, error_message FROM connector_sync_runs \
             WHERE connector = ?1 AND key = ?2 ORDER BY id DESC LIMIT ?3",
        )
        .bind(connector)
        .bind(key)
        .bind(limit.max(1) as i64)
        .fetch_all(&self.pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db() -> DatabaseManager {
        DatabaseManager::new("sqlite::memory:", Default::default())
            .await
            .unwrap()
    }

    fn draft(connector: &str, ns: &str, id: &str, title: &str, body: &str) -> ConnectorObjectDraft {
        ConnectorObjectDraft {
            connector: connector.to_string(),
            namespace: ns.to_string(),
            object_kind: "item".to_string(),
            object_id: id.to_string(),
            revision: None,
            title: Some(title.to_string()),
            body_text: body.to_string(),
            content_hash: None,
            event_at: None,
            fetched_at: Utc::now(),
            source_url: None,
        }
    }

    fn hashed_draft(
        connector: &str,
        ns: &str,
        id: &str,
        title: &str,
        body: &str,
        hash: &str,
    ) -> ConnectorObjectDraft {
        ConnectorObjectDraft {
            content_hash: Some(hash.to_string()),
            ..draft(connector, ns, id, title, body)
        }
    }

    #[tokio::test]
    async fn upsert_dedups_and_updates() {
        let db = db().await;
        assert_eq!(
            db.connector_upsert_object(&draft("rss", "feed-a", "i1", "标题", "内容一"))
                .await
                .unwrap(),
            ConnectorUpsertOutcome::Created
        );
        // Same identity: update, not create.
        assert_eq!(
            db.connector_upsert_object(&draft("rss", "feed-a", "i1", "标题改", "内容一改"))
                .await
                .unwrap(),
            ConnectorUpsertOutcome::Unchanged
        );
        assert_eq!(db.connector_imported_object_count("rss").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn unchanged_content_skips_rewrite() {
        let db = db().await;
        let first_at = Utc::now();
        let mut first = hashed_draft("rss", "feed-a", "i1", "标题", "  内容  一  ", "h1");
        first.fetched_at = first_at;
        assert_eq!(
            db.connector_upsert_object(&first).await.unwrap(),
            ConnectorUpsertOutcome::Created
        );
        // Same identity, same content hash: nothing written — the original
        // fetched_at survives instead of being bumped every sync.
        let mut again = hashed_draft("rss", "feed-a", "i1", "标题", "内容 一", "h1");
        again.fetched_at = first_at + chrono::Duration::hours(1);
        assert_eq!(
            db.connector_upsert_object(&again).await.unwrap(),
            ConnectorUpsertOutcome::Unchanged
        );
        let row: (String, Option<String>) = sqlx::query_as(
            "SELECT fetched_at, content_hash FROM connector_objects \
             WHERE connector = 'rss' AND object_id = 'i1'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(row.0, crate::db::connector::connector_format_ts(first_at));
        assert_eq!(row.1.as_deref(), Some("h1"));
    }

    #[tokio::test]
    async fn duplicate_content_under_new_identity_skips() {
        let db = db().await;
        assert_eq!(
            db.connector_upsert_object(&hashed_draft("rss", "feed-a", "i1", "标题", "正文", "h1"))
                .await
                .unwrap(),
            ConnectorUpsertOutcome::Created
        );
        // Same content under a different guid/namespace: duplicate, not a
        // second row.
        assert_eq!(
            db.connector_upsert_object(&hashed_draft("rss", "feed-b", "i2", "标题", "正文", "h1"))
                .await
                .unwrap(),
            ConnectorUpsertOutcome::Duplicate
        );
        assert_eq!(db.connector_imported_object_count("rss").await.unwrap(), 1);
        // A different hash under the new identity still lands.
        assert_eq!(
            db.connector_upsert_object(&hashed_draft(
                "rss",
                "feed-b",
                "i2",
                "另一篇",
                "另一正文",
                "h2"
            ))
            .await
            .unwrap(),
            ConnectorUpsertOutcome::Created
        );
    }

    #[tokio::test]
    async fn erased_objects_are_never_resurrected() {
        let db = db().await;
        db.connector_upsert_object(&draft("rss", "feed-a", "i1", "t", "b"))
            .await
            .unwrap();
        assert_eq!(db.connector_erase("rss").await.unwrap(), 1);
        // Re-import after erase: suppressed.
        assert_eq!(
            db.connector_upsert_object(&draft("rss", "feed-a", "i1", "t", "b"))
                .await
                .unwrap(),
            ConnectorUpsertOutcome::Unchanged
        );
        assert_eq!(db.connector_imported_object_count("rss").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn fts_search_matches_chinese_and_filters_by_state() {
        let db = db().await;
        db.connector_upsert_object(&draft(
            "rss",
            "feed-a",
            "i1",
            "中文标题",
            "这是一段中文正文，用于验证全文检索。",
        ))
        .await
        .unwrap();
        db.connector_upsert_object(&draft(
            "rss",
            "feed-a",
            "i2",
            "english title",
            "plain english body",
        ))
        .await
        .unwrap();

        let hits = db.connector_search("rss", "中文正文", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].object_id, "i1");

        let hits = db
            .connector_search("rss", "english body", 10)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].object_id, "i2");

        // Disabled content stops surfacing.
        db.connector_disable_all_objects("rss").await.unwrap();
        assert!(db
            .connector_search("rss", "english", 10)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(db.connector_imported_object_count("rss").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn scope_revision_advances_and_cursor_round_trips() {
        let db = db().await;
        let r1 = db
            .connector_save_scope("rss", "", r#"{"feed_urls":["https://a.example"]}"#)
            .await
            .unwrap();
        let r2 = db
            .connector_save_scope("rss", "", r#"{"feed_urls":["https://a.example"]}"#)
            .await
            .unwrap();
        assert_eq!(r2, r1 + 1);
        let (scope, revision) = db.connector_get_scope("rss", "").await.unwrap().unwrap();
        assert_eq!(revision, r2);
        assert!(scope.contains("a.example"));

        db.connector_set_cursor("rss", "", "feed:a", "item-42")
            .await
            .unwrap();
        assert_eq!(
            db.connector_get_cursor("rss", "", "feed:a").await.unwrap(),
            Some("item-42".to_string())
        );
        db.connector_clear_cursors("rss").await.unwrap();
        assert_eq!(
            db.connector_get_cursor("rss", "", "feed:a").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn sync_run_lifecycle_round_trips() {
        let db = db().await;
        let id = db.connector_sync_run_start("rss", "").await.unwrap();
        // Newest-first listing shows the open run.
        let open = db.connector_sync_runs_recent("rss", "", 5).await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].id, id);
        assert_eq!(open[0].status, "running");
        assert_eq!(open[0].processed, 0);

        db.connector_sync_run_finish(id, "partial", 3, 10, 1, None, None)
            .await
            .unwrap();
        let runs = db.connector_sync_runs_recent("rss", "", 5).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "partial");
        assert_eq!(runs[0].processed, 3);
        assert_eq!(runs[0].skipped, 10);
        assert_eq!(runs[0].failed, 1);
        assert!(runs[0].finished_at.is_some());

        // Second run lists above the first.
        let id2 = db.connector_sync_run_start("rss", "").await.unwrap();
        assert_ne!(id2, id);
        let runs = db.connector_sync_runs_recent("rss", "", 1).await.unwrap();
        assert_eq!(runs[0].id, id2);
    }

    #[tokio::test]
    async fn startup_reset_closes_run_history_left_open_by_a_crash() {
        let db = db().await;
        let id = db.connector_sync_run_start("rss", "").await.unwrap();
        db.connector_reset_stale_sync_runs().await.unwrap();
        let runs = db.connector_sync_runs_recent("rss", "", 5).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, id);
        assert_eq!(runs[0].status, "failed");
        assert_eq!(runs[0].error_code.as_deref(), Some("interrupted"));
        assert!(runs[0].finished_at.is_some());
    }
}
