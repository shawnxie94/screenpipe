// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

use super::{
    durable_json, storage_error, sync_directory, HybridStorage, PrivacyPolicy, Projection,
    StorageBudget, StorageDescriptor, StorageMode,
};
use crate::DatabaseManager;
use screenpipe_config::DbConfig;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool, TypeInfo, ValueRef};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests {
    use super::*;

    // Frozen pre-optimization receipt reader/codec: an independent oracle for
    // old journals and the paired benchmark, never used by production.
    async fn legacy_receipt(db: &DatabaseManager, table: &str) -> TableParity {
        let mut columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
                .bind(table)
                .fetch_all(&db.pool)
                .await
                .unwrap();
        columns.retain(|column| !column.starts_with("_archive_"));
        let mut hash = Sha256::new();
        let mut count = 0;
        let mut last = i64::MIN;
        let mut first = true;
        loop {
            let sql = format!(
                "SELECT rowid,{} FROM {} WHERE rowid{}? AND rowid<=? ORDER BY rowid LIMIT 128",
                columns
                    .iter()
                    .map(|c| quote(c))
                    .collect::<Vec<_>>()
                    .join(","),
                quote(table),
                if first { ">=" } else { ">" }
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(last)
                .bind(i64::MAX)
                .fetch_all(&db.pool)
                .await
                .unwrap();
            if rows.is_empty() {
                break;
            }
            first = false;
            for row in rows {
                last = row.get(0);
                for index in 0..columns.len() {
                    legacy_hash_value(&mut hash, &row, index + 1, None).unwrap();
                }
                hash.update(b"E");
                count += 1;
            }
        }
        TableParity {
            table: table.into(),
            rows: count,
            sha256: format!("{:x}", hash.finalize()),
        }
    }

    fn legacy_hash_value(
        hash: &mut Sha256,
        row: &sqlx::sqlite::SqliteRow,
        index: usize,
        replacement: Option<&Option<String>>,
    ) -> Result<(), sqlx::Error> {
        if let Some(value) = replacement {
            if let Some(value) = value {
                hash.update(b"T");
                hash.update((value.len() as u64).to_le_bytes());
                hash.update(value.as_bytes());
            } else {
                hash.update(b"N");
            }
            return Ok(());
        }
        let raw = row.try_get_raw(index)?;
        if raw.is_null() {
            hash.update(b"N");
            return Ok(());
        }
        match raw.type_info().name() {
            "INTEGER" => {
                hash.update(b"I");
                hash.update(row.try_get::<i64, _>(index)?.to_le_bytes());
            }
            "REAL" => {
                hash.update(b"R");
                hash.update(row.try_get::<f64, _>(index)?.to_bits().to_le_bytes());
            }
            "TEXT" => {
                let bytes: Vec<u8> = row.try_get(index)?;
                hash.update(b"T");
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
            }
            _ => {
                let bytes: Vec<u8> = row.try_get(index)?;
                hash.update(b"B");
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
            }
        }
        Ok(())
    }

    #[tokio::test]
    #[ignore = "paired migration verification throughput benchmark on an isolated million-row history"]
    async fn parity_throughput() {
        let root = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(
            root.path().join("db.sqlite").to_str().unwrap(),
            Default::default(),
        )
        .await
        .unwrap();
        // Use the actual legacy element column layout, without capture/FTS
        // triggers in fixture setup. Both implementations hash the same cells.
        db.execute_raw_sql_write("CREATE TABLE parity_benchmark AS SELECT * FROM elements WHERE 0; CREATE UNIQUE INDEX parity_benchmark_id ON parity_benchmark(id);").await.unwrap();
        let rows = 1_000_000_i64;
        let mut tx = db.begin_immediate_with_retry().await.unwrap();
        sqlx::query("WITH RECURSIVE n(id) AS (VALUES(1) UNION ALL SELECT id+1 FROM n WHERE id<?) INSERT INTO parity_benchmark(id,frame_id,source,role,text,properties) SELECT id,id/100,'accessibility','AXText','searchable historical element '||id,'{\"label\":\"東京 history\",\"enabled\":true}' FROM n")
            .bind(rows).execute(&mut **tx.conn()).await.unwrap();
        tx.commit().await.unwrap();
        let tables = [TableParity {
            table: "parity_benchmark".into(),
            rows: 0,
            sha256: String::new(),
        }];
        for run in 1..=3 {
            let mut receipts = Vec::new();
            for legacy in if run % 2 == 1 {
                [true, false]
            } else {
                [false, true]
            } {
                let started = std::time::Instant::now();
                let receipt = if legacy {
                    legacy_receipt(&db, "parity_benchmark").await
                } else {
                    table_receipts(&db, Some(&tables)).await.unwrap().remove(0)
                };
                assert_eq!(receipt.rows, rows as u64);
                eprintln!("parity benchmark: run={run} legacy={legacy} rows={rows} elapsed_ms={:.3} sha256={}", started.elapsed().as_secs_f64()*1000.0, receipt.sha256);
                receipts.push(receipt);
            }
            assert_eq!(receipts[0], receipts[1]);
        }
        db.close().await;
    }

    #[tokio::test]
    async fn parity_includes_domain_payload_columns_and_minimum_rowids() {
        let root = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(
            root.path().join("standalone.sqlite").to_str().unwrap(),
            Default::default(),
        )
        .await
        .unwrap();
        db.execute_raw_sql_write("CREATE TABLE receipt_probe(id INTEGER PRIMARY KEY,payload_json TEXT); INSERT INTO receipt_probe VALUES(-9223372036854775808,'before')").await.unwrap();
        let before = table_receipts(&db, None).await.unwrap();
        let before = before.iter().find(|r| r.table == "receipt_probe").unwrap();
        assert_eq!(before.rows, 1);
        db.execute_raw_sql_write("UPDATE receipt_probe SET payload_json='after'")
            .await
            .unwrap();
        let after = table_receipts(&db, None).await.unwrap();
        assert_ne!(
            before.sha256,
            after
                .iter()
                .find(|r| r.table == "receipt_probe")
                .unwrap()
                .sha256
        );
        db.close().await;
    }

    #[tokio::test]
    async fn streaming_parity_matches_existing_receipts_for_every_storage_class() {
        for encoding in ["UTF-8", "UTF-16le"] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("standalone.sqlite");
            {
                let db = rusqlite::Connection::open(&path).unwrap();
                db.execute_batch(&format!(
                    "PRAGMA encoding='{encoding}'; CREATE TABLE encoding_probe(n INTEGER);"
                ))
                .unwrap();
            }
            let db = DatabaseManager::new(path.to_str().unwrap(), Default::default())
                .await
                .unwrap();
            let table = "receipt \"probe";
            db.execute_raw_sql_write(&format!("CREATE TABLE {}(id INTEGER PRIMARY KEY,untyped,\"text value\" TEXT,bytes BLOB,number REAL,flag BOOLEAN,_archive_ignored TEXT); CREATE TABLE empty_receipt(id INTEGER PRIMARY KEY,payload TEXT);",quote(table))).await.unwrap();
            db.execute_raw_sql_write(&format!("INSERT INTO {} VALUES(-9223372036854775808,NULL,'',X'',NULL,0,'ignored'),(-2,1,'東京'||char(0)||'tail',X'0001FF',1.25,1,'ignored'),(7,'1',CAST(X'80FF' AS TEXT),NULL,-1e200,NULL,'ignored'),(9223372036854775807,X'31',NULL,X'00',0.0,1,'ignored');",quote(table))).await.unwrap();
            let mut tx = db.begin_immediate_with_retry().await.unwrap();
            sqlx::query(sqlx::AssertSqlSafe(format!("WITH RECURSIVE n(id) AS (VALUES(10) UNION ALL SELECT id+1 FROM n WHERE id<9000) INSERT INTO {}(id,untyped,\"text value\",bytes) SELECT id,id,'retained history '||id,zeroblob(128) FROM n", quote(table))))
                .execute(&mut **tx.conn()).await.unwrap();
            tx.commit().await.unwrap();
            for table in [table, "empty_receipt"] {
                let expected = legacy_receipt(&db, table).await;
                let actual = table_receipts(&db, Some(std::slice::from_ref(&expected)))
                    .await
                    .unwrap();
                assert_eq!(
                    actual,
                    vec![expected],
                    "{encoding}: old journals must remain usable"
                );
            }
            db.close().await;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StorageInitOptions {
    pub budget: StorageBudget,
    pub privacy: PrivacyPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableParity {
    pub table: String,
    pub rows: u64,
    pub sha256: String,
}

pub(super) async fn verify_integrity(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let checks: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_all(pool)
        .await?;
    if checks != ["ok"] {
        return Err(storage_error("SQLite integrity verification failed"));
    }
    // Usable legacy histories can contain dangling foreign keys. Preserve
    // those rows: migration verifies logical receipts and archive contents,
    // rather than requiring old data to satisfy today's relationship rules.
    // Ordinary writes still enforce their foreign-key constraints.
    Ok(())
}

/// Compact a closed, inactive candidate with one destination-sized scratch
/// file. Closing its manager first releases the bootstrap WAL allocation.
pub(super) async fn compact_candidate(
    index: &Path,
    storage: &std::sync::Arc<HybridStorage>,
) -> Result<(), sqlx::Error> {
    let budget = &storage.descriptor.budget;
    tracing::info!(phase = "compact_candidate", "storage migration");
    let compact = index.with_extension("compacting.sqlite");
    if compact.exists() {
        std::fs::remove_file(&compact)?;
    }
    // The last manager connection can be read-only, leaving a valid WAL on
    // disk. Checkpoint and leave WAL mode before replacing this inactive file;
    // otherwise old WAL frames can be replayed onto the compacted page layout.
    let mut conn = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new()
            .filename(index)
            .pragma("locking_mode", "EXCLUSIVE")
            .pragma("journal_mode", "DELETE"),
    )
    .await?;
    super::bulk::register_hash(&mut conn).await?;
    if storage.has_bulk() {
        super::bulk::elements::register(&mut conn, storage.clone()).await?;
    }
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&mut conn)
        .await?;
    let free: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(&mut conn)
        .await?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&mut conn)
        .await?;
    let needed = ((pages - free) as u64)
        .saturating_mul(page_size as u64)
        .saturating_add(budget.disk_reserve_bytes);
    if fs2::available_space(index.parent().unwrap())? < needed {
        conn.close().await?;
        return Err(storage_error("insufficient compact scratch reserve"));
    }
    let vacuum = sqlx::query("VACUUM INTO ?")
        .bind(
            compact
                .to_str()
                .ok_or_else(|| storage_error("non-UTF8 compact path"))?,
        )
        .execute(&mut conn)
        .await;
    conn.close().await?;
    vacuum?;
    super::sync_file(&compact)?;
    super::faults::checkpoint("candidate_compacted");
    std::fs::rename(&compact, index)?;
    sync_directory(index.parent().unwrap())
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub(super) fn hash_value(
    hash: &mut Sha256,
    row: &sqlx::sqlite::SqliteRow,
    index: usize,
    replacement: Option<&Option<String>>,
) -> Result<(), sqlx::Error> {
    use rusqlite::types::ValueRef as Cell;
    let value = if let Some(value) = replacement {
        value
            .as_ref()
            .map_or(Cell::Null, |text| Cell::Text(text.as_bytes()))
    } else {
        let raw = row.try_get_raw(index)?;
        if raw.is_null() {
            Cell::Null
        } else {
            match raw.type_info().name() {
                "INTEGER" => Cell::Integer(row.try_get(index)?),
                "REAL" => Cell::Real(row.try_get(index)?),
                "TEXT" => Cell::Text(row.try_get(index)?),
                _ => Cell::Blob(row.try_get(index)?),
            }
        }
    };
    super::parity::hash_cell(hash, value);
    Ok(())
}

pub(super) async fn logical_tables(db: &DatabaseManager) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT m.name FROM sqlite_master m WHERE m.type='table' AND (m.name NOT LIKE 'sqlite_%' OR m.name='sqlite_sequence') AND m.name NOT LIKE '%_fts%' AND m.name NOT LIKE '%_fts5%' AND (m.sql NOT LIKE 'CREATE VIRTUAL TABLE%' OR m.name='elements') ORDER BY m.name")
        .fetch_all(&db.pool).await
}

// Verification consumes history incrementally; the public response limit must
// never decide whether that history can migrate. Resident frames are hashed
// from SQLite, and only sealed frames need hydration through the payload reader.
async fn verification_frame_batch(
    db: &DatabaseManager,
    after: Option<i64>,
) -> Result<Option<(i64, Vec<i64>)>, sqlx::Error> {
    let comparison = if after.is_some() { ">" } else { ">=" };
    let (size, state, join, limit) = if let Some(storage) = &db.storage {
        (
            "p.bytes".to_owned(),
            "p.state='sealed'",
            "LEFT JOIN frame_payloads p ON p.frame_id=f.id",
            storage
                .descriptor
                .budget
                .file_bytes
                .min(storage.descriptor.budget.response_bytes),
        )
    } else {
        (
            super::schema::BYTES.replace("NEW.", "f."),
            "0",
            "",
            StorageBudget::default().file_bytes,
        )
    };
    let rows: Vec<(i64, i64, bool)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT f.id,{size},{state} FROM frames f {join} WHERE f.id{comparison}? ORDER BY f.id LIMIT 128"
    )))
    .bind(after.unwrap_or(i64::MIN))
    .fetch_all(&db.pool)
    .await?;
    let mut last = None;
    let mut ids = Vec::new();
    let mut bytes = 0usize;
    for (id, size, sealed) in rows {
        let size = usize::try_from(size).map_err(storage_error)?;
        if last.is_some() && bytes.saturating_add(size) > limit {
            break;
        }
        last = Some(id);
        bytes = bytes.saturating_add(size);
        if sealed {
            ids.push(id);
        }
    }
    Ok(last.map(|last| (last, ids)))
}

pub(super) async fn table_receipts(
    db: &DatabaseManager,
    source: Option<&[TableParity]>,
) -> Result<Vec<TableParity>, sqlx::Error> {
    let tables: Vec<String> = if let Some(source) = source {
        source.iter().map(|t| t.table.clone()).collect()
    } else {
        logical_tables(db).await?
    };
    let mut result = Vec::new();
    for table in tables {
        let mut columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
                .bind(&table)
                .fetch_all(&db.pool)
                .await?;
        if table == "frames" && db.storage.is_some() {
            columns.retain(|name| {
                !matches!(
                    name.as_str(),
                    "payload_full_text_length"
                        | "payload_accessibility_length"
                        | "payload_full_text_present"
                        | "payload_accessibility_present"
                        | "payload_detail_present"
                )
            });
        }
        columns.retain(|name| !name.starts_with("_archive_"));
        let key = if db.storage.as_ref().is_some_and(|s| s.has_bulk())
            && super::bulk::is_bulk_table(&table)
        {
            "id"
        } else {
            "rowid"
        };
        if table != "frames" || db.storage.is_none() {
            let sql = format!(
                "SELECT {key},{} FROM {} ORDER BY {key}",
                columns
                    .iter()
                    .map(|column| quote(column))
                    .collect::<Vec<_>>()
                    .join(","),
                quote(&table),
            );
            let receipt = super::parity::scan(db.pool.clone(), table.clone(), sql).await?;
            tracing::info!(table=%table, rows=receipt.rows, "storage parity receipt complete");
            result.push(receipt);
            continue;
        }
        // Hybrid frames still hydrate sealed payloads in byte-bounded batches.
        let mut hash = Sha256::new();
        let mut count = 0;
        let mut last = i64::MIN;
        let mut first = true;
        loop {
            let (through, sealed) = if table == "frames" {
                let Some(batch) = verification_frame_batch(db, (!first).then_some(last)).await?
                else {
                    break;
                };
                batch
            } else {
                (i64::MAX, Vec::new())
            };
            let sql = format!(
                "SELECT {key} AS __storage_rowid,{} FROM {} WHERE {key}{}? AND {key}<=? ORDER BY {key} LIMIT 128",
                columns
                    .iter()
                    .map(|c| quote(c))
                    .collect::<Vec<_>>()
                    .join(","),
                quote(&table),
                if first {">="} else {">"}
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(last)
                .bind(through)
                .fetch_all(&db.pool)
                .await?;
            if rows.is_empty() {
                break;
            }
            first = false;
            let payloads = if !sealed.is_empty() {
                db.frame_payloads(&sealed, Projection::All).await?
            } else {
                Default::default()
            };
            for row in rows {
                last = row.try_get("__storage_rowid")?;
                let payload = payloads.get(&last);
                for (index, column) in columns.iter().enumerate() {
                    let value = payload.and_then(|p| match column.as_str() {
                        "full_text" => Some(&p.full_text),
                        "accessibility_text" => Some(&p.accessibility_text),
                        "accessibility_tree_json" => Some(&p.accessibility_tree_json),
                        "text_json" => Some(&p.text_json),
                        _ => None,
                    });
                    hash_value(&mut hash, &row, index + 1, value)?;
                }
                hash.update(b"E");
                count += 1;
            }
        }
        tracing::info!(table=%table, rows=count, "storage parity receipt complete");
        result.push(TableParity {
            table,
            rows: count,
            sha256: format!("{:x}", hash.finalize()),
        });
    }
    Ok(result)
}

/// Owns the source connection and snapshot on the blocking thread, including
/// when its async caller is cancelled. No borrowed SQLite handle outlives it.
pub(super) async fn copy_sqlite(
    pool: SqlitePool,
    destination: PathBuf,
    timeout_secs: u64,
) -> Result<(), sqlx::Error> {
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            let mut conn = pool.acquire().await?;
            let mut tx = conn.begin().await?;
            sqlx::query("SELECT count(*) FROM sqlite_master")
                .fetch_one(&mut *tx)
                .await?;
            let mut locked = tx.lock_handle().await?;
            let name = std::ffi::CString::new(destination.to_string_lossy().as_bytes())
                .map_err(storage_error)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
            // SAFETY: the source is exclusively held by LockedSqliteHandle;
            // this thread owns the destination and backup until both close.
            let result = unsafe {
                let mut dest = std::ptr::null_mut();
                let rc = libsqlite3_sys::sqlite3_open_v2(
                    name.as_ptr(),
                    &mut dest,
                    libsqlite3_sys::SQLITE_OPEN_READWRITE | libsqlite3_sys::SQLITE_OPEN_CREATE,
                    std::ptr::null(),
                );
                if rc != libsqlite3_sys::SQLITE_OK {
                    if !dest.is_null() {
                        libsqlite3_sys::sqlite3_close(dest);
                    }
                    return Err(storage_error("cannot open backup destination"));
                }
                let backup = libsqlite3_sys::sqlite3_backup_init(
                    dest,
                    c"main".as_ptr(),
                    locked.as_raw_handle().as_ptr(),
                    c"main".as_ptr(),
                );
                if backup.is_null() {
                    libsqlite3_sys::sqlite3_close(dest);
                    return Err(storage_error("cannot initialize SQLite backup"));
                }
                let rc = loop {
                    let rc = libsqlite3_sys::sqlite3_backup_step(backup, 256);
                    if rc != libsqlite3_sys::SQLITE_OK {
                        break rc;
                    }
                    if std::time::Instant::now() >= deadline {
                        break libsqlite3_sys::SQLITE_INTERRUPT;
                    }
                    std::thread::yield_now();
                };
                let finish = libsqlite3_sys::sqlite3_backup_finish(backup);
                let close = libsqlite3_sys::sqlite3_close(dest);
                if rc == libsqlite3_sys::SQLITE_DONE && finish == 0 && close == 0 {
                    Ok(())
                } else {
                    Err(storage_error(format!(
                        "SQLite backup failed ({rc}/{finish}/{close})"
                    )))
                }
            };
            drop(locked);
            tx.commit().await?;
            result?;
            super::sync_file(&destination)?;
            sync_directory(destination.parent().unwrap())?;
            Ok(())
        })
    })
    .await
    .map_err(storage_error)?
}

use sqlx::Connection;

impl DatabaseManager {
    /// Initialize an empty root in the current format, or leave an initialized
    /// current-format root untouched. A legacy db.sqlite without a descriptor
    /// is intentionally rejected by `new_hybrid` rather than opened in place.
    pub async fn ensure_hybrid_storage(root: &Path, config: DbConfig) -> Result<(), sqlx::Error> {
        if StorageDescriptor::read(root)?.is_none() || root.join("storage-init.json").exists() {
            let database = Self::new_hybrid(root, config, StorageInitOptions::default()).await?;
            database.close().await;
        }
        Ok(())
    }

    pub async fn verify_storage(&self) -> Result<(), sqlx::Error> {
        verify_integrity(&self.pool).await?;
        if let Some(storage) = &self.storage {
            storage.verify_catalog(&self.pool).await?;
            storage.verify_bulk(&self.pool).await?;
            let mut after = None;
            while let Some((last, ids)) = verification_frame_batch(self, after).await? {
                after = Some(last);
                self.frame_payloads(&ids, Projection::All).await?;
            }
        }
        Ok(())
    }

    /// Explicit opt-in for an independent empty logical database root.
    pub async fn new_hybrid(
        root: &Path,
        config: DbConfig,
        options: StorageInitOptions,
    ) -> Result<Self, sqlx::Error> {
        std::fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        super::inventory::verify_protection(&root)?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".storage.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| storage_error("storage lifecycle is already owned"))?;
        let initialization = root.join("storage-init.json");
        if let Some(active) = StorageDescriptor::read(&root)? {
            let expected: StorageDescriptor = serde_json::from_slice(
                &std::fs::read(&initialization)
                    .map_err(|_| storage_error("root already has an active storage descriptor"))?,
            )
            .map_err(storage_error)?;
            if active != expected {
                return Err(storage_error("initialization descriptor mismatch"));
            }
            std::fs::remove_file(&initialization)?;
            sync_directory(&root)?;
            return Self::new(root.join("db.sqlite").to_str().unwrap(), config).await;
        }
        let descriptor = if initialization.exists() {
            serde_json::from_slice(&std::fs::read(&initialization)?).map_err(storage_error)?
        } else {
            if root.join("db.sqlite").exists() {
                return Err(storage_error(
                    "fresh hybrid initialization requires an empty database root",
                ));
            }
            let generation = uuid::Uuid::new_v4().to_string();
            let directory = PathBuf::from("storage").join(&generation);
            let descriptor = StorageDescriptor {
                format: 1,
                mode: StorageMode::HybridParquetV1,
                database_id: uuid::Uuid::new_v4().to_string(),
                generation,
                source_continuity: false,
                index: directory.join("index.sqlite"),
                payloads: directory.join("payloads"),
                capabilities: super::capabilities(),
                budget: options.budget,
                privacy: options.privacy,
            };
            durable_json(&initialization, &descriptor)?;
            descriptor
        };
        descriptor.validate(&root)?;
        std::fs::create_dir_all(root.join(&descriptor.payloads))?;
        let index = root.join(&descriptor.index);
        let catalog_exists = if index.exists() {
            let mut conn = sqlx::SqliteConnection::connect_with(
                &sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&index)
                    .read_only(true),
            )
            .await?;
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM sqlite_master WHERE name='_hybrid_migrations'",
            )
            .fetch_one(&mut conn)
            .await?;
            let complete = if count != 0 {
                let version = if descriptor
                    .capabilities
                    .iter()
                    .any(|c| c == super::bulk::CAPABILITY)
                {
                    2
                } else {
                    1
                };
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM _hybrid_migrations WHERE version=?",
                )
                .bind(version)
                .fetch_one(&mut conn)
                .await?
                    != 0
            } else {
                false
            };
            conn.close().await?;
            complete
        } else {
            false
        };
        if !catalog_exists {
            // Initialization owns an empty unpublished generation. Its schema
            // completion receipt determines whether construction can be reused.
            for suffix in ["", "-wal", "-shm"] {
                let path = PathBuf::from(format!("{}{suffix}", index.display()));
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
            }
        }
        let candidate = Self::new_with_storage(
            index.to_str().unwrap(),
            config.clone(),
            Some(HybridStorage::new(root.clone(), descriptor.clone())?),
            !catalog_exists,
            false,
        )
        .await?;
        let verified = verify_integrity(&candidate.pool).await;
        candidate.close().await;
        verified?;
        sync_directory(index.parent().unwrap())?;
        sync_directory(&root.join("storage"))?;
        super::faults::checkpoint("initialization_ready");
        durable_json(&root.join("storage.json"), &descriptor)?;
        super::faults::checkpoint("initialization_activated");
        std::fs::remove_file(&initialization)?;
        sync_directory(&root)?;
        Self::new(root.join("db.sqlite").to_str().unwrap(), config).await
    }
}
