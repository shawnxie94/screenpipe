// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Office connector persistence — now a thin mapping over the generic
//! `connector_*` store (see `db/connector.rs`), kept under the historical
//! `office_*` method names so the engine's sync orchestration is untouched.
//! Office-specific fields (CLI paths, completeness, activity anchors) travel
//! in the `metadata` JSON column; the tables themselves stay channel-generic.

use super::*;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

/// Timestamp format shared with the connector tables (RFC3339 micros).
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

/// The office channel registers one connector id per provider
/// (`office:feishu`, `office:tencent-meeting`) so rows keep their provider
/// identity in the generic `(connector, key)` addressing; the aggregate
/// adapter presents both as sub-keys of one "office" channel.
fn conn_id(provider: &str) -> String {
    format!("office:{provider}")
}


/// Connection-level fields with no generic column, packed into
/// `connector_connections.metadata`.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct OfficeConnectionMeta {
    #[serde(default)]
    account_namespace: Option<String>,
    #[serde(default)]
    cli_path: Option<String>,
    #[serde(default)]
    cli_version: Option<String>,
    #[serde(default)]
    credential_ref: Option<String>,
    #[serde(default)]
    runtime_status: Option<String>,
}

/// Object-level fields with no generic column, packed into
/// `connector_objects.metadata`.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct OfficeObjectMeta {
    #[serde(default)]
    completeness: Option<String>,
    #[serde(default)]
    activity_anchor: Option<String>,
    #[serde(default, deserialize_with = "deserialize_platform_generated")]
    platform_generated: Option<bool>,
}

// Pre-merge rows carried a strict 0/1 column; JSON carries true/false or the
// number. Accept all three so migrated and fresh rows parse identically.
fn deserialize_platform_generated<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<serde_json::Value> = serde::Deserialize::deserialize(deserializer)?;
    Ok(match v {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Bool(b)) => Some(b),
        Some(serde_json::Value::Number(n)) => Some(n.as_i64().unwrap_or(0) != 0),
        Some(other) => {
            return Err(serde::de::Error::custom(format!(
                "invalid platform_generated: {other}"
            )))
        }
    })
}

fn parse_meta<T: Default + serde::de::DeserializeOwned>(raw: Option<String>) -> T {
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[derive(Debug)]
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

#[derive(Debug, serde::Serialize)]
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

/// Normalized imported object before persistence; maps to a
/// `connector_objects` row (connector='office') plus its FTS entry.
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

/// Raw connector_connections row; converted to `OfficeConnectionRow` with
/// metadata unpacked.
#[derive(Debug, sqlx::FromRow)]
struct ConnectorConnRaw {
    key: String,
    enabled: i64,
    auto_sync: i64,
    auth_status: String,
    sync_status: String,
    connection_revision: i64,
    scope_revision: i64,
    last_sync_at: Option<String>,
    last_success_at: Option<String>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    metadata: Option<String>,
}

impl From<ConnectorConnRaw> for OfficeConnectionRow {
    fn from(r: ConnectorConnRaw) -> Self {
        let meta: OfficeConnectionMeta = parse_meta(r.metadata);
        Self {
            provider: r.key,
            account_namespace: meta.account_namespace,
            cli_path: meta.cli_path,
            cli_version: meta.cli_version,
            credential_ref: meta.credential_ref,
            runtime_status: meta
                .runtime_status
                .unwrap_or_else(|| "missing".to_string()),
            auth_status: r.auth_status,
            sync_status: r.sync_status,
            connection_revision: r.connection_revision,
            scope_revision: r.scope_revision,
            enabled: r.enabled != 0,
            auto_sync: r.auto_sync != 0,
            last_sync_at: r.last_sync_at,
            last_success_at: r.last_success_at,
            last_error_code: r.last_error_code,
            last_error_message: r.last_error_message,
        }
    }
}

impl DatabaseManager {
    async fn office_get_connection_raw(
        &self,
        provider: &str,
    ) -> Result<Option<ConnectorConnRaw>, SqlxError> {
        sqlx::query_as::<_, ConnectorConnRaw>(
            "SELECT key, enabled, auto_sync, auth_status, sync_status, \
             connection_revision, scope_revision, last_sync_at, last_success_at, \
             last_error_code, last_error_message, metadata FROM connector_connections \
             WHERE connector = ?1 AND key = ?2",
        )
        .bind(conn_id(provider))
        .bind(provider)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn office_get_connection(
        &self,
        provider: &str,
    ) -> Result<Option<OfficeConnectionRow>, SqlxError> {
        Ok(self
            .office_get_connection_raw(provider)
            .await?
            .map(OfficeConnectionRow::from))
    }

    pub async fn office_list_connections(&self) -> Result<Vec<OfficeConnectionRow>, SqlxError> {
        let rows = sqlx::query_as::<_, ConnectorConnRaw>(
            "SELECT key, enabled, auto_sync, auth_status, sync_status, \
             connection_revision, scope_revision, last_sync_at, last_success_at, \
             last_error_code, last_error_message, metadata FROM connector_connections \
             WHERE connector LIKE 'office:%' ORDER BY key",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(OfficeConnectionRow::from).collect())
    }

    pub async fn office_ensure_connection(&self, provider: &str) -> Result<(), SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        self.office_ensure_connection_tx(&mut tx, provider).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn office_ensure_connection_tx(
        &self,
        tx: &mut ImmediateTx,
        provider: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "INSERT OR IGNORE INTO connector_connections (connector, key) VALUES (?1, ?2)",
        )
        .bind(conn_id(provider))
        .bind(provider)
        .execute(&mut **tx.conn())
        .await?;
        Ok(())
    }

    pub async fn office_update_connection(
        &self,
        provider: &str,
        updates: OfficeConnectionUpdate,
    ) -> Result<i64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        self.office_ensure_connection_tx(&mut tx, provider).await?;

        // Office-specific fields live in metadata; read-merge-write inside the
        // same transaction the generic columns are updated in.
        let meta_field_updates = [
            updates.account_namespace.clone().map(|v| ("account_namespace", v)),
            updates.cli_path.clone().map(|v| ("cli_path", v)),
            updates.cli_version.clone().map(|v| ("cli_version", v)),
            updates.credential_ref.clone().map(|v| ("credential_ref", v)),
            updates.runtime_status.clone().map(|v| ("runtime_status", v)),
        ];
        let has_meta_updates = meta_field_updates.iter().any(|u| u.is_some());
        if has_meta_updates {
            let current: Option<(String,)> = sqlx::query_as(
                "SELECT metadata FROM connector_connections \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .fetch_optional(&mut **tx.conn())
            .await?;
            let mut meta: OfficeConnectionMeta =
                parse_meta(current.map(|(m,)| m));
            for update in meta_field_updates.into_iter().flatten() {
                match update {
                    ("account_namespace", v) => meta.account_namespace = Some(v),
                    ("cli_path", v) => meta.cli_path = Some(v),
                    ("cli_version", v) => meta.cli_version = Some(v),
                    ("credential_ref", v) => meta.credential_ref = Some(v),
                    ("runtime_status", v) => meta.runtime_status = Some(v),
                    _ => unreachable!("field list above"),
                }
            }
            sqlx::query(
                "UPDATE connector_connections SET metadata = ?3 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .bind(serde_json::to_string(&meta).unwrap_or_default())
            .execute(&mut **tx.conn())
            .await?;
        }

        if let Some(v) = updates.auth_status {
            sqlx::query(
                "UPDATE connector_connections SET auth_status = ?3 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .bind(v)
            .execute(&mut **tx.conn())
            .await?;
        }
        if let Some(v) = updates.sync_status {
            sqlx::query(
                "UPDATE connector_connections SET sync_status = ?3 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .bind(v)
            .execute(&mut **tx.conn())
            .await?;
        }
        if let Some(v) = updates.enabled {
            sqlx::query(
                "UPDATE connector_connections SET enabled = ?3 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .bind(v as i64)
            .execute(&mut **tx.conn())
            .await?;
        }
        if let Some(v) = updates.auto_sync {
            sqlx::query(
                "UPDATE connector_connections SET auto_sync = ?3 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .bind(v as i64)
            .execute(&mut **tx.conn())
            .await?;
        }
        if let Some(v) = updates.last_sync_at {
            sqlx::query(
                "UPDATE connector_connections SET last_sync_at = ?3 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .bind(v)
            .execute(&mut **tx.conn())
            .await?;
        }
        if let Some(v) = updates.last_success_at {
            sqlx::query(
                "UPDATE connector_connections SET last_success_at = ?3 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .bind(v)
            .execute(&mut **tx.conn())
            .await?;
        }
        if let Some(v) = updates.last_error_code {
            sqlx::query(
                "UPDATE connector_connections SET last_error_code = ?3 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .bind(v)
            .execute(&mut **tx.conn())
            .await?;
        }
        if let Some(v) = updates.last_error_message {
            sqlx::query(
                "UPDATE connector_connections SET last_error_message = ?3 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .bind(v)
            .execute(&mut **tx.conn())
            .await?;
        }
        if updates.clear_errors {
            sqlx::query(
                "UPDATE connector_connections SET last_error_code = NULL, \
                 last_error_message = NULL WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .execute(&mut **tx.conn())
            .await?;
        }
        if updates.bump_connection_revision {
            sqlx::query(
                "UPDATE connector_connections SET connection_revision = connection_revision + 1 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .execute(&mut **tx.conn())
            .await?;
        }
        if updates.bump_scope_revision {
            sqlx::query(
                "UPDATE connector_connections SET scope_revision = scope_revision + 1 \
                 WHERE connector = ?1 AND key = ?2",
            )
            .bind(conn_id(provider))
            .bind(provider)
            .execute(&mut **tx.conn())
            .await?;
        }
        sqlx::query(
            "UPDATE connector_connections SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
             WHERE connector = ?1 AND key = ?2",
        )
        .bind(conn_id(provider))
        .bind(provider)
        .execute(&mut **tx.conn())
        .await?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT scope_revision FROM connector_connections \
             WHERE connector = ?1 AND key = ?2",
        )
        .bind(conn_id(provider))
        .bind(provider)
        .fetch_one(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(revision)
    }

    // ------------------------------------------------------------------
    // Scopes
    // ------------------------------------------------------------------

    pub async fn office_get_scope(
        &self,
        provider: &str,
    ) -> Result<Option<(String, i64)>, SqlxError> {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT scope, revision FROM connector_scopes WHERE connector = ?1 AND key = ?2",
        )
        .bind(conn_id(provider))
        .bind(provider)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn office_save_scope(&self, provider: &str, scope_json: &str) -> Result<i64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        self.office_ensure_connection_tx(&mut tx, provider).await?;
        sqlx::query(
            "INSERT INTO connector_scopes (connector, key, scope, revision) \
             VALUES (?1, ?2, ?3, 1) \
             ON CONFLICT (connector, key) DO UPDATE SET scope = ?3, \
             revision = revision + 1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(conn_id(provider))
        .bind(provider)
        .bind(scope_json)
        .execute(&mut **tx.conn())
        .await?;
        // The connection row's scope_revision is what start_sync's OCC guard
        // compares against — bump it in step with the scope revision and
        // return it (pre-merge semantics).
        sqlx::query(
            "UPDATE connector_connections SET scope_revision = scope_revision + 1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
             WHERE connector = ?1 AND key = ?2",
        )
        .bind(conn_id(provider))
        .bind(provider)
        .execute(&mut **tx.conn())
        .await?;
        let (revision,): (i64,) = sqlx::query_as(
            "SELECT scope_revision FROM connector_connections \
             WHERE connector = ?1 AND key = ?2",
        )
        .bind(conn_id(provider))
        .bind(provider)
        .fetch_one(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(revision)
    }

    // ------------------------------------------------------------------
    // Imported objects
    // ------------------------------------------------------------------

    /// Upsert one imported object (idempotent per provider/account/kind/id).
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
        let meta = serde_json::to_string(&OfficeObjectMeta {
            completeness: Some(draft.completeness.clone()),
            activity_anchor: draft.activity_anchor.clone(),
            platform_generated: Some(draft.platform_generated),
        })
        .unwrap_or_default();
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT state FROM connector_objects WHERE connector = ?1 AND namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(conn_id(&draft.provider))
        .bind(&draft.account_namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .fetch_optional(&mut **tx.conn())
        .await?;
        // User-erased objects (state='deleted') must never be resurrected by
        // a later import over the same identity.
        if matches!(existing.as_ref(), Some((s,)) if s == "deleted") {
            tx.commit().await?; // read-only run; release the reservation
            return Ok(false);
        }
        let created = existing.is_none();
        sqlx::query(
            "INSERT INTO connector_objects (connector, namespace, object_kind, object_id, \
             revision, title, body_text, event_at, fetched_at, source_url, metadata, state) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'active') \
             ON CONFLICT (connector, namespace, object_kind, object_id) DO UPDATE SET \
             revision = ?5, title = ?6, body_text = ?7, event_at = ?8, fetched_at = ?9, \
             source_url = ?10, metadata = ?11, state = 'active', \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(conn_id(&draft.provider))
        .bind(&draft.account_namespace)
        .bind(&draft.object_kind)
        .bind(&draft.object_id)
        .bind(&draft.revision)
        .bind(&draft.title)
        .bind(&normalized)
        .bind(&event)
        .bind(&fetched)
        .bind(&draft.source_url)
        .bind(&meta)
        .execute(&mut **tx.conn())
        .await?;
        // FTS: replace the row's projection (identity columns enable a direct
        // join with connector_objects on query). Chinese text is projected
        // into unigram/bigram tokens so unicode61 can match it.
        let fts_body = crate::text_normalizer::chinese_project(&format!(
            "{} {}",
            draft.title.clone().unwrap_or_default(),
            normalized
        ));
        sqlx::query(
            "DELETE FROM connector_objects_fts WHERE connector = ?1 AND namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(conn_id(&draft.provider))
        .bind(&draft.account_namespace)
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
        .bind(conn_id(&draft.provider))
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
            "SELECT COUNT(*) FROM connector_objects WHERE connector = ?1 AND namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(conn_id(provider))
        .bind(account_namespace)
        .bind(object_kind)
        .bind(object_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    async fn office_row_from_parts(
        &self,
        provider: &str,
        namespace: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<OfficeObjectRow>, SqlxError> {
        let raw: Option<(Option<String>, Option<String>, String, Option<String>, String, Option<String>, Option<String>, String)> = sqlx::query_as(
            "SELECT revision, title, body_text, event_at, fetched_at, source_url, metadata, state \
             FROM connector_objects WHERE connector = ?1 AND namespace = ?2 \
             AND object_kind = ?3 AND object_id = ?4",
        )
        .bind(conn_id(provider))
        .bind(namespace)
        .bind(kind)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let Some((revision, title, body_text, event_at, fetched_at, source_url, metadata, state)) =
            raw
        else {
            return Ok(None);
        };
        let meta: OfficeObjectMeta = parse_meta(metadata);
        Ok(Some(OfficeObjectRow {
            provider: provider.to_string(),
            account_namespace: namespace.to_string(),
            object_kind: kind.to_string(),
            object_id: id.to_string(),
            revision,
            title,
            completeness: meta.completeness.unwrap_or_else(|| "full".to_string()),
            body_text,
            event_at,
            fetched_at,
            source_url,
            activity_anchor: meta.activity_anchor,
            platform_generated: meta.platform_generated.unwrap_or(false),
            state,
        }))
    }

    pub async fn office_get_object(
        &self,
        provider: &str,
        account_namespace: &str,
        object_kind: &str,
        object_id: &str,
    ) -> Result<Option<OfficeObjectRow>, SqlxError> {
        self.office_row_from_parts(provider, account_namespace, object_kind, object_id)
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
            "SELECT object_kind, object_id FROM connector_objects \
             WHERE connector = ?1 AND namespace = ?2 AND state = 'active'",
        )
        .bind(conn_id(provider))
        .bind(account_namespace)
        .fetch_all(&mut **tx.conn())
        .await?;
        let mut disabled = Vec::new();
        for (kind, id) in rows {
            let keep = keep_object_ids.iter().any(|(k, i)| *k == kind && *i == id);
            if !keep {
                sqlx::query(
                    "UPDATE connector_objects SET state = 'disabled', \
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
                     WHERE connector = ?1 AND namespace = ?2 AND object_kind = ?3 AND object_id = ?4",
                )
                .bind(conn_id(provider))
                .bind(account_namespace)
                .bind(&kind)
                .bind(&id)
                .execute(&mut **tx.conn())
                .await?;
                // Drop FTS so disabled content stops surfacing.
                sqlx::query(
                    "DELETE FROM connector_objects_fts WHERE connector = ?1 AND namespace = ?2 \
                     AND object_kind = ?3 AND object_id = ?4",
                )
                .bind(conn_id(provider))
                .bind(account_namespace)
                .bind(&kind)
                .bind(&id)
                .execute(&mut **tx.conn())
                .await?;
                disabled.push((kind, id));
            }
        }
        tx.commit().await?;
        let _ = provider;
        Ok(disabled)
    }

    pub async fn office_imported_object_count(&self, provider: &str) -> Result<i64, SqlxError> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM connector_objects WHERE connector = ?1 AND state = 'active'",
        )
        .bind(conn_id(provider))
        .fetch_one(&self.pool)
        .await
    }

    /// Erase every imported object of a provider, dropping FTS rows and
    /// leaving the object identity in place (state='deleted') so re-imports
    /// are suppressed.
    pub async fn office_erase(&self, provider: &str) -> Result<u64, SqlxError> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT namespace, object_kind, object_id FROM connector_objects \
             WHERE connector = ?1",
        )
        .bind(conn_id(provider))
        .fetch_all(&mut **tx.conn())
        .await?;
        let mut erased = 0u64;
        for (ns, kind, id) in &rows {
            sqlx::query(
                "DELETE FROM connector_objects_fts WHERE connector = ?1 AND namespace = ?2 \
                 AND object_kind = ?3 AND object_id = ?4",
            )
            .bind(conn_id(provider))
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
            .bind(conn_id(provider))
            .bind(ns)
            .bind(kind)
            .bind(id)
            .execute(&mut **tx.conn())
            .await?;
            erased += 1;
        }
        tx.commit().await?;
        let _ = provider;
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
            "SELECT f.connector, f.namespace, f.object_kind, f.object_id, \
             bm25(connector_objects_fts, 10.0) AS rank \
             FROM connector_objects_fts f \
             JOIN connector_objects o ON o.connector = f.connector \
                 AND o.namespace = f.namespace \
                 AND o.object_kind = f.object_kind \
                 AND o.object_id = f.object_id \
             WHERE o.state = 'active' AND o.connector LIKE 'office:%' \
                 AND (?1 IS NULL OR o.connector = ?1) AND connector_objects_fts MATCH ?2 \
             ORDER BY rank LIMIT ?3",
        )
        .bind(provider.map(conn_id))
        .bind(&projected)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for (conn, ns, kind, id, rank) in rows {
            // connector id is `office:<provider>`; recover the provider so the
            // DTO keeps the shape office has always returned.
            let provider = conn
                .strip_prefix("office:")
                .unwrap_or(conn.as_str())
                .to_string();
            if let Some(row) = self
                .office_row_from_parts(&provider, &ns, &kind, &id)
                .await?
            {
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
            "SELECT cursor_value FROM connector_cursors \
             WHERE connector = ?1 AND key = ?2 AND cursor_key = ?3",
        )
        .bind(conn_id(provider))
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
            "INSERT INTO connector_cursors (connector, key, cursor_key, cursor_value) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (connector, key, cursor_key) DO UPDATE SET cursor_value = ?4, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(conn_id(provider))
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
        sqlx::query("DELETE FROM connector_cursors WHERE connector = ?1")
            .bind(conn_id(provider))
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
