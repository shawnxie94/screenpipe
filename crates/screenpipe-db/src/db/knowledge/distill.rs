// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

use super::*;

/// Distillation delivery state (plan §4.2, ④ 三机制): model nominations
/// recorded at extraction time, plus per-scope last-distillation identity.
/// The three-mechanism decision itself lives in
/// `screenpipe_engine::knowledge::distill`; these are only its durable rows.
impl DatabaseManager {
    /// Record an extraction-time nomination: the extract model said this
    /// scope's work shows a reusable pattern (`flow` = suggested knowledge
    /// home, optional). Re-extraction of the same unit refreshes the row.
    pub async fn knowledge_record_distill_nomination(
        &self,
        scope_key: &str,
        work_unit_id: &str,
        flow: Option<&str>,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "INSERT INTO knowledge_distill_nominations (scope_key, work_unit_id, flow, nominated_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (scope_key, work_unit_id) DO UPDATE SET \
             flow = excluded.flow, nominated_at = excluded.nominated_at",
        )
        .bind(scope_key)
        .bind(work_unit_id)
        .bind(flow)
        .bind(super::types::format_ts(super::types::now_utc()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Drop a scope's nominations after a completed distillation, so the next
    /// delivery needs a fresh nomination (机制一：未被提名的 scope 不投).
    pub async fn knowledge_clear_distill_nominations(
        &self,
        scope_key: &str,
    ) -> Result<u64, SqlxError> {
        let result = sqlx::query("DELETE FROM knowledge_distill_nominations WHERE scope_key = ?1")
            .bind(scope_key)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Scopes with outstanding nominations, oldest nomination first — the ④
    /// tick's candidate list. Nominations are the only source: a scope the
    /// model never nominated never becomes a candidate. The list is complete:
    /// the ④ timer only decides when to look, never how many (plan §4.2 —
    /// delivery has no quantity threshold).
    pub async fn knowledge_list_distill_candidate_scopes(
        &self,
    ) -> Result<Vec<String>, SqlxError> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT scope_key FROM knowledge_distill_nominations \
             GROUP BY scope_key ORDER BY MIN(nominated_at)",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// `(last_distilled_input_hash, last_distilled_at)` for a scope; `None`
    /// when the scope was never distilled.
    pub async fn knowledge_get_distill_state(
        &self,
        scope_key: &str,
    ) -> Result<Option<(Option<String>, Option<String>)>, SqlxError> {
        let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT last_distilled_input_hash, last_distilled_at FROM knowledge_distill_state \
             WHERE scope_key = ?1",
        )
        .bind(scope_key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Persist a completed distillation: the scope's work-unit set hash at
    /// distillation time and the completion moment. One row per scope.
    pub async fn knowledge_record_distill_success(
        &self,
        scope_key: &str,
        input_hash: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "INSERT INTO knowledge_distill_state (scope_key, last_distilled_input_hash, last_distilled_at) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT (scope_key) DO UPDATE SET \
             last_distilled_input_hash = excluded.last_distilled_input_hash, \
             last_distilled_at = excluded.last_distilled_at",
        )
        .bind(scope_key)
        .bind(input_hash)
        .bind(super::types::format_ts(super::types::now_utc()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The scope's active work units with their latest valid revision hash —
    /// the members of the change-driven set hash. Capped generously; compile
    /// itself reads at most 20, so 500 bounds the hash without unbounded scans.
    pub async fn knowledge_scope_distill_members(
        &self,
        scope_key: &str,
    ) -> Result<Vec<(String, String)>, SqlxError> {
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT wu.id, \
             (SELECT r.input_hash FROM knowledge_work_unit_revisions r \
              WHERE r.work_unit_id = wu.id AND r.state = 'valid' \
              ORDER BY r.created_at DESC LIMIT 1) AS input_hash \
             FROM knowledge_work_units wu \
             WHERE wu.scope_key = ?1 AND wu.state = 'active' \
             ORDER BY wu.id LIMIT 500",
        )
        .bind(scope_key)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, hash)| hash.map(|h| (id, h)))
            .collect())
    }
}
