// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Office connector persistence: connection/scope/cursor state and imported
//! objects with their own full-text search. Self-contained — no dependency on
//! the retired knowledge domain.

use super::*;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

/// Timestamp format used across office tables (SQLite TEXT, RFC3339 micros).
pub fn office_format_ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

pub fn office_fingerprint(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            hasher.update([0x1f]);
        }
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

#[derive(Debug, sqlx::FromRow)]
pub struct OfficeConnectionRow {
    pub provider: String,
    pub account_namespace: Option<String>,
    pub cli_path: Option<String>,
    pub cli_version: Option<String>,
    pub credential_ref: Option<String>,
    pub runtime_status: String,
    pub auth_status: String,
    pub sync_status: String,
    pub connection_revision: i64,
    pub scope_revision: i64,
    pub enabled: bool,
    pub auto_sync: bool,
    pub last_sync_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct OfficeObjectRow {
    pub provider: String,
    pub account_namespace: String,
    pub object_kind: String,
    pub object_id: String,
    pub revision: Option<String>,
    pub title: Option<String>,
    pub body_text: String,
    pub completeness: String,
    pub event_at: Option<String>,
    pub fetched_at: String,
    pub source_url: Option<String>,
    pub activity_anchor: Option<String>,
    pub platform_generated: bool,
    pub state: String,
}

/// Normalized imported object before persistence; maps to an
/// `office_objects` row plus its FTS entry.
#[derive(Clone, Debug)]
pub struct OfficeObjectDraft {
    pub provider: String,
    pub account_namespace: String,
    pub object_kind: String, // message | document | meeting | transcript | summary
    pub object_id: String,
    pub revision: Option<String>,
    pub title: Option<String>,
    pub body_text: String,
    pub completeness: String, // full | partial:<reason>
    pub event_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub source_url: Option<String>,
    pub activity_anchor: Option<String>,
    pub platform_generated: bool,
}

impl DatabaseManager {
    // ------------------------------------------------------------------
    // Connections
    // ------------------------------------------------------------------

    pub async fn office_get_connection(
        &self,
        provider: &str,
    ) -> Result<Option<OfficeConnectionRow>, SqlxError> {
        sqlx::query_as::<_, OfficeConnectionRow>(
            "SELECT provider, account_namespace, cli_path, cli_version, credential_ref, \
             runtime_status, auth_status, sync_status, connection_revision, scope_revision, \
             enabled, auto_sync, last_sync_at, last_success_at, last_error_code, \
             last_error_message FROM office_connections WHERE provider = ?1",
        )
        .bind(provider)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn office_list_connections(&self) -> Result<Vec<OfficeConnectionRow>, SqlxError> {
        sqlx::query_as::<_, OfficeConnectionRow>(
            "SELECT provider, account_namespace, cli_path, cli_version, credential_ref, \
             runtime_status, auth_status, sync_status, connection_revision, scope_revision, \
             enabled, auto_sync, last_sync_at, last_success_at, last_error_code, \
             last_error_message FROM office_connections ORDER BY provider",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn office_ensure_connection(&self, provider: &str) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query("INSERT OR IGNORE INTO office_connections (provider) VALUES (?1)")
            .bind(provider)
            .execute(&mut **tx.conn())
            .await?;
        tx.commit().await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn office_update_connection(
        &self,
        provider: &str,
        updates: OfficeConnectionUpdate,
    ) -> Result<i64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        self.office_ensure_connection_tx(&mut tx, provider).await?;
        if let Some(v) = updates.account_namespace {
            sqlx::query("UPDATE office_connections SET account_namespace = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.cli_path {
            sqlx::query("UPDATE office_connections SET cli_path = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.cli_version {
            sqlx::query("UPDATE office_connections SET cli_version = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.credential_ref {
            sqlx::query("UPDATE office_connections SET credential_ref = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.runtime_status {
            sqlx::query("UPDATE office_connections SET runtime_status = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.auth_status {
            sqlx::query("UPDATE office_connections SET auth_status = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.sync_status {
            sqlx::query("UPDATE office_connections SET sync_status = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.enabled {
            sqlx::query("UPDATE office_connections SET enabled = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.auto_sync {
            sqlx::query("UPDATE office_connections SET auto_sync = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.last_sync_at {
            sqlx::query("UPDATE office_connections SET last_sync_at = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.last_success_at {
            sqlx::query("UPDATE office_connections SET last_success_at = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.last_error_code {
            sqlx::query("UPDATE office_connections SET last_error_code = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if let Some(v) = updates.last_error_message {
            sqlx::query("UPDATE office_connections SET last_error_message = ?1 WHERE provider = ?2")
                .bind(v)
                .bind(provider)
                .execute(&mut **tx.conn())
                .await?;
        }
        if updates.clear_errors {
            sqlx::query(
                "UPDATE office_connections SET last_error_code = NULL, \
                 last_error_message = NULL WHERE provider = ?1",
            )
            .bind(provider)
            .execute(&mut **tx.conn())
            .await?;
        }
        if updates.bump_connection_revision {
            sqlx::query(
                "UPDATE office_connections SET connection_revision = connection_revision + 1 \
                 WHERE provider = ?1",
            )
            .bind(provider)
            .execute(&mut **tx.conn())
            .await?;
        }
        if updates.bump_scope_revision {
            sqlx::query(
                "UPDATE office_connections SET scope_revision = scope_revision + 1 \
                 WHERE provider = ?1",
            )
            .bind(provider)
            .execute(&mut **tx.conn())
            .await?;
        }
        sqlx::query(
            "UPDATE office_connections SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
             WHERE provider = ?1",
        )
        .bind(provider)
        .execute(&mut **tx.conn())
        .await?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT scope_revision FROM office_connections WHERE provider = ?1",
        )
        .bind(provider)
        .fetch_one(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(revision)
    }

    async fn office_ensure_connection_tx(
        &self,
        tx: &mut ImmediateTx,
        provider: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query("INSERT OR IGNORE INTO office_connections (provider) VALUES (?1)")
            .bind(provider)
            .execute(&mut **tx.conn())
            .await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Scopes
    // ------------------------------------------------------------------

    pub async fn office_get_scope(
        &self,
        provider: &str,
    ) -> Result<Option<(String, i64)>, SqlxError> {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT scope, revision FROM office_scopes WHERE provider = ?1",
        )
        .bind(provider)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn office_save_scope(&self, provider: &str, scope_json: &str) -> Result<i64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        self.office_ensure_connection_tx(&mut tx, provider).await?;
        sqlx::query(
            "INSERT INTO office_scopes (provider, scope, revision) VALUES (?1, ?2, 1) \
             ON CONFLICT (provider) DO UPDATE SET scope = ?2, revision = revision + 1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(provider)
        .bind(scope_json)
        .execute(&mut **tx.conn())
        .await?;
        sqlx::query(
            "UPDATE office_connections SET scope_revision = scope_revision + 1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE provider = ?1",
        )
        .bind(provider)
        .execute(&mut **tx.conn())
        .await?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT scope_revision FROM office_connections WHERE provider = ?1",
        )
        .bind(provider)
        .fetch_one(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(revision)
    }

    // ------------------------------------------------------------------
    // Objects
    // ------------------------------------------------------------------

    /// Upsert one imported object (idempotent per provider/account/kind/id).
    /// `doc_key` derives from the stable identity so FTS rows follow upserts.
    /// Returns true when the object was new (first import this run).
    pub async fn office_upsert_object(&self, draft: &OfficeObjectDraft) -> Result<bool, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let fetched = office_format_ts(draft.fetched_at);
        let event = draft.event_at.map(office_format_ts);
        let normalized = draft
            .body_text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT revision, state FROM office_objects WHERE provider = ?1 \
             AND account_namespace = ?2 AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(&draft.provider)
        .bind(&draft.account_namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .fetch_optional(&mut **tx.conn())
        .await?;
        // User-erased objects (state='deleted') must never be resurrected by
        // a later import over the same identity.
        if matches!(existing.as_ref(), Some((_, s)) if s == "deleted") {
            tx.commit().await?; // read-only run; release the reservation
            return Ok(false);
        }
        let created = existing.is_none();
        sqlx::query(
            "INSERT INTO office_objects (provider, account_namespace, object_kind, object_id, \
             revision, title, body_text, completeness, event_at, fetched_at, source_url, \
             activity_anchor, platform_generated, state) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'active') \
             ON CONFLICT (provider, account_namespace, object_kind, object_id) DO UPDATE SET \
             revision = ?5, title = ?6, body_text = ?7, completeness = ?8, event_at = ?9, \
             fetched_at = ?10, source_url = ?11, activity_anchor = ?12, \
             platform_generated = ?13, state = 'active', \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(&draft.provider)
        .bind(&draft.account_namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .bind(&draft.revision)
        .bind(&draft.title)
        .bind(&normalized)
        .bind(&draft.completeness)
        .bind(&event)
        .bind(&fetched)
        .bind(&draft.source_url)
        .bind(&draft.activity_anchor)
        .bind(draft.platform_generated)
        .execute(&mut **tx.conn())
        .await?;
        // FTS: replace the row's projection (provider/ns/kind/id enable a
        // direct join with office_objects on query). Chinese text is
        // projected into unigram/bigram tokens so unicode61 can match it.
        let fts_body = crate::text_normalizer::chinese_project(&format!(
            "{} {}",
            draft.title.clone().unwrap_or_default(),
            normalized
        ));
        sqlx::query(
            "DELETE FROM office_objects_fts WHERE provider = ?1 AND account_namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(&draft.provider)
        .bind(&draft.account_namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .execute(&mut **tx.conn())
        .await?;
        sqlx::query(
            "INSERT INTO office_objects_fts (body, title, provider, account_namespace, \
             object_kind, object_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&fts_body)
        .bind(&draft.title)
        .bind(&draft.provider)
        .bind(&draft.account_namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(created)
    }

    /// Returns true when the object exists (any state) — used to suppress
    /// re-import of user-erased content.
    pub async fn office_object_exists(
        &self,
        provider: &str,
        account_namespace: &str,
        object_kind: &str,
        object_id: &str,
    ) -> Result<bool, SqlxError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM office_objects WHERE provider = ?1 AND account_namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(provider)
        .bind(account_namespace)
        .bind(object_kind)
        .bind(object_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn office_get_object(
        &self,
        provider: &str,
        account_namespace: &str,
        object_kind: &str,
        object_id: &str,
    ) -> Result<Option<OfficeObjectRow>, SqlxError> {
        sqlx::query_as::<_, OfficeObjectRow>(
            "SELECT provider, account_namespace, object_kind, object_id, revision, title, \
             body_text, completeness, event_at, fetched_at, source_url, activity_anchor, \
             platform_generated, state \
             FROM office_objects WHERE provider = ?1 AND account_namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(provider)
        .bind(account_namespace)
        .bind(object_kind)
        .bind(object_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Disable everything outside the saved scope (scope shrink / account
    /// switch / disconnect). Returns affected object identities.
    pub async fn office_disable_out_of_scope(
        &self,
        provider: &str,
        account_namespace: &str,
        keep_object_ids: &[(String, String)], // (object_kind, object_id)
    ) -> Result<Vec<(String, String)>, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT object_kind, object_id FROM office_objects \
             WHERE provider = ?1 AND account_namespace = ?2 AND state = 'active'",
        )
        .bind(provider)
        .bind(account_namespace)
        .fetch_all(&mut **tx.conn())
        .await?;
        let mut disabled = Vec::new();
        for (kind, id) in rows {
            let keep = keep_object_ids.iter().any(|(k, i)| *k == kind && *i == id);
            if !keep {
                sqlx::query(
                    "UPDATE office_objects SET state = 'disabled', \
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
                     WHERE provider = ?1 AND account_namespace = ?2 AND object_kind = ?3 AND object_id = ?4",
                )
                .bind(provider)
                .bind(account_namespace)
                .bind(&kind)
                .bind(&id)
                .execute(&mut **tx.conn())
                .await?;
                // Drop FTS so disabled content stops surfacing.
                sqlx::query(
                    "DELETE FROM office_objects_fts WHERE provider = ?1 AND account_namespace = ?2 \
                     AND object_kind = ?3 AND object_id = ?4",
                )
                .bind(provider)
                .bind(account_namespace)
                .bind(&kind)
                .bind(&id)
                .execute(&mut **tx.conn())
                .await?;
                disabled.push((kind, id));
            }
        }
        tx.commit().await?;
        Ok(disabled)
    }

    pub async fn office_imported_object_count(&self, provider: &str) -> Result<i64, SqlxError> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM office_objects WHERE provider = ?1 AND state = 'active'",
        )
        .bind(provider)
        .fetch_one(&self.pool)
        .await
    }

    /// Erase every imported object of a provider, dropping FTS rows and
    /// leaving the object identity in place (state='deleted') so re-imports
    /// are suppressed.
    pub async fn office_erase(&self, provider: &str) -> Result<u64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT provider, account_namespace, object_kind, object_id \
             FROM office_objects WHERE provider = ?1",
        )
        .bind(provider)
        .fetch_all(&mut **tx.conn())
        .await?;
        let mut erased = 0u64;
        for (prov, ns, kind, id) in &rows {
            sqlx::query(
                "DELETE FROM office_objects_fts WHERE provider = ?1 AND account_namespace = ?2 \
                 AND object_kind = ?3 AND object_id = ?4",
            )
            .bind(prov)
            .bind(ns)
            .bind(kind)
            .bind(id)
            .execute(&mut **tx.conn())
            .await?;
            sqlx::query(
                "UPDATE office_objects SET state = 'deleted', \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
                 WHERE provider = ?1 AND account_namespace = ?2 AND object_kind = ?3 AND object_id = ?4",
            )
            .bind(prov)
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

    /// Full-text search over imported office objects. Returns matching
    /// active objects, newest first.
    pub async fn office_search(
        &self,
        provider: Option<&str>,
        query: &str,
        limit: u32,
    ) -> Result<Vec<(OfficeObjectRow, f64)>, SqlxError> {
        let projected = crate::text_normalizer::chinese_project(query);
        let limit = limit.max(1) as i64;
        let rows: Vec<(String, String, String, String, f64)> = sqlx::query_as(
            "SELECT f.provider, f.account_namespace, f.object_kind, f.object_id, \
             bm25(office_objects_fts, 10.0) AS rank \
             FROM office_objects_fts f \
             JOIN office_objects o ON o.provider = f.provider \
                 AND o.account_namespace = f.account_namespace \
                 AND o.object_kind = f.object_kind \
                 AND o.object_id = f.object_id \
             WHERE o.state = 'active' AND office_objects_fts MATCH ?1 \
                 AND (?2 IS NULL OR o.provider = ?2) \
             ORDER BY rank LIMIT ?3",
        )
        .bind(&projected)
        .bind(provider)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for (prov, ns, kind, id, rank) in rows {
            if let Some(row) = self.office_get_object(&prov, &ns, &kind, &id).await? {
                out.push((row, rank));
            }
        }
        Ok(out)
    }

    // ------------------------------------------------------------------
    // Cursors (per provider/key high-water; never shared with backfill)
    // ------------------------------------------------------------------

    pub async fn office_get_cursor(
        &self,
        provider: &str,
        cursor_key: &str,
    ) -> Result<Option<String>, SqlxError> {
        sqlx::query_scalar(
            "SELECT cursor_value FROM office_cursors WHERE provider = ?1 AND cursor_key = ?2",
        )
        .bind(provider)
        .bind(cursor_key)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn office_set_cursor(
        &self,
        provider: &str,
        cursor_key: &str,
        cursor_value: &str,
    ) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "INSERT INTO office_cursors (provider, cursor_key, cursor_value) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT (provider, cursor_key) DO UPDATE SET cursor_value = ?3, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(provider)
        .bind(cursor_key)
        .bind(cursor_value)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn office_clear_cursors(&self, provider: &str) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query("DELETE FROM office_cursors WHERE provider = ?1")
            .bind(provider)
            .execute(&mut **tx.conn())
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

impl OfficeObjectRow {
    /// DSL helper: FTS doc_key for a projection query (identity of the
    /// indexed row; matches `office_upsert_object`'s doc_key computation).
    pub fn doc_key_for(provider: &str, ns: &str, kind: &str, id: &str) -> String {
        office_fingerprint(&[provider, ns, kind, id])
    }
}

/// Explicit update payload for `office_update_connection`.
#[derive(Clone, Debug, Default)]
pub struct OfficeConnectionUpdate {
    pub account_namespace: Option<String>,
    pub cli_path: Option<String>,
    pub cli_version: Option<String>,
    pub credential_ref: Option<String>,
    pub runtime_status: Option<String>,
    pub auth_status: Option<String>,
    pub sync_status: Option<String>,
    pub enabled: Option<bool>,
    pub auto_sync: Option<bool>,
    pub last_sync_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub bump_connection_revision: bool,
    pub bump_scope_revision: bool,
    /// `last_error_*` fields use `None` = leave unchanged (field-by-field
    /// update); set this flag to actually clear a previously recorded error.
    pub clear_errors: bool,
}