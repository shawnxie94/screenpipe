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
#[derive(Debug, Clone, Default)]
pub struct ConnectorObjectDraft {
    pub connector: String,
    pub namespace: String,
    pub object_kind: String,
    pub object_id: String,
    pub revision: Option<String>,
    pub title: Option<String>,
    pub body_text: String,
    pub event_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub source_url: Option<String>,
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

impl DatabaseManager {
    async fn connector_ensure_connection(
        &self,
        connector: &str,
        key: &str,
    ) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "INSERT OR IGNORE INTO connector_connections (connector, key) VALUES (?1, ?2)",
        )
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
    /// of syncing forever. Returns the number of rows reset.
    pub async fn connector_reset_stale_sync_runs(&self) -> Result<u64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
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

    /// Insert or update an imported object plus its FTS projection. Returns
    /// true when the object was created. User-erased objects (state='deleted')
    /// are never resurrected.
    pub async fn connector_upsert_object(
        &self,
        draft: &ConnectorObjectDraft,
    ) -> Result<bool, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let fetched = connector_format_ts(draft.fetched_at);
        let event = draft.event_at.map(connector_format_ts);
        let normalized = draft
            .body_text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT state FROM connector_objects WHERE connector = ?1 AND namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(&draft.connector)
        .bind(&draft.namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .fetch_optional(&mut **tx.conn())
        .await?;
        if matches!(existing.as_ref(), Some((s,)) if s == "deleted") {
            tx.commit().await?;
            return Ok(false);
        }
        let created = existing.is_none();
        sqlx::query(
            "INSERT INTO connector_objects (connector, namespace, object_kind, object_id, \
             revision, title, body_text, event_at, fetched_at, source_url, state) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'active') \
             ON CONFLICT (connector, namespace, object_kind, object_id) DO UPDATE SET \
             revision = ?5, title = ?6, body_text = ?7, event_at = ?8, fetched_at = ?9, \
             source_url = ?10, state = 'active', \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(&draft.connector)
        .bind(&draft.namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .bind(&draft.revision)
        .bind(&draft.title)
        .bind(&normalized)
        .bind(&event)
        .bind(&fetched)
        .bind(&draft.source_url)
        .execute(&mut **tx.conn())
        .await?;
        // FTS: same Chinese unigram/bigram projection as the office tables.
        let fts_body = crate::text_normalizer::chinese_project(&format!(
            "{} {}",
            draft.title.clone().unwrap_or_default(),
            normalized
        ));
        sqlx::query(
            "DELETE FROM connector_objects_fts WHERE connector = ?1 AND namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(&draft.connector)
        .bind(&draft.namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .execute(&mut **tx.conn())
        .await?;
        sqlx::query(
            "INSERT INTO connector_objects_fts (body, title, connector, namespace, \
             object_kind, object_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&fts_body)
        .bind(&draft.title)
        .bind(&draft.connector)
        .bind(&draft.namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(created)
    }

    pub async fn connector_imported_object_count(
        &self,
        connector: &str,
    ) -> Result<i64, SqlxError> {
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
    pub async fn connector_disable_all_objects(
        &self,
        connector: &str,
    ) -> Result<u64, SqlxError> {
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
        let projected = crate::text_normalizer::chinese_project(query);
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
            let rows = sqlx::query_as::<_, ConnectorObjectRow>(sqlx::AssertSqlSafe(page_sql.as_str()))
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
            let projected = crate::text_normalizer::chinese_project(trimmed);
            let page_sql = format!(
                "SELECT o.connector, o.namespace, o.object_kind, o.object_id, o.revision, \
                 o.title, o.body_text, o.event_at, o.fetched_at, o.source_url, o.state \
                 FROM connector_objects_fts f \
                 JOIN connector_objects o ON o.connector = f.connector \
                     AND o.namespace = f.namespace \
                     AND o.object_kind = f.object_kind \
                     AND o.object_id = f.object_id \
                 WHERE o.state = 'active' AND connector_objects_fts MATCH ?3 {time_pred} \
                 ORDER BY bm25(connector_objects_fts, 10.0) \
                 LIMIT {limit} OFFSET {offset}"
            );
            let count_sql = format!(
                "SELECT COUNT(*) FROM connector_objects_fts f \
                 JOIN connector_objects o ON o.connector = f.connector \
                     AND o.namespace = f.namespace \
                     AND o.object_kind = f.object_kind \
                     AND o.object_id = f.object_id \
                 WHERE o.state = 'active' AND connector_objects_fts MATCH ?3 {time_pred}"
            );
            let rows = sqlx::query_as::<_, ConnectorObjectRow>(sqlx::AssertSqlSafe(page_sql.as_str()))
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
            event_at: None,
            fetched_at: Utc::now(),
            source_url: None,
        }
    }

    #[tokio::test]
    async fn upsert_dedups_and_updates() {
        let db = db().await;
        assert!(db
            .connector_upsert_object(&draft("rss", "feed-a", "i1", "标题", "内容一"))
            .await
            .unwrap());
        // Same identity: update, not create.
        assert!(!db
            .connector_upsert_object(&draft("rss", "feed-a", "i1", "标题改", "内容一改"))
            .await
            .unwrap());
        assert_eq!(
            db.connector_imported_object_count("rss").await.unwrap(),
            1
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
        assert!(!db
            .connector_upsert_object(&draft("rss", "feed-a", "i1", "t", "b"))
            .await
            .unwrap());
        assert_eq!(
            db.connector_imported_object_count("rss").await.unwrap(),
            0
        );
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

        let hits = db.connector_search("rss", "english body", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].object_id, "i2");

        // Disabled content stops surfacing.
        db.connector_disable_all_objects("rss").await.unwrap();
        assert!(db.connector_search("rss", "english", 10).await.unwrap().is_empty());
        assert_eq!(
            db.connector_imported_object_count("rss").await.unwrap(),
            0
        );
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

        db.connector_set_cursor("rss", "", "feed:a", "item-42").await.unwrap();
        assert_eq!(
            db.connector_get_cursor("rss", "", "feed:a").await.unwrap(),
            Some("item-42".to_string())
        );
        db.connector_clear_cursors("rss").await.unwrap();
        assert_eq!(db.connector_get_cursor("rss", "", "feed:a").await.unwrap(), None);
    }
}
