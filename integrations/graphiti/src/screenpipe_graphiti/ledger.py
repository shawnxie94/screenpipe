# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import os
import sqlite3
from pathlib import Path
from typing import Any


class SyncLedger:
    """Private local metadata only; episode bodies and credentials are never stored here."""

    def __init__(self, path: str) -> None:
        self.path = Path(path).expanduser()
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        try:
            os.chmod(self.path.parent, 0o700)
        except OSError:
            pass
        self._closed = False
        self._db = sqlite3.connect(
            self.path, timeout=10, isolation_level=None, check_same_thread=False
        )
        self._db.execute("PRAGMA journal_mode=WAL")
        self._db.execute("PRAGMA busy_timeout=10000")
        self._db.execute(
            """CREATE TABLE IF NOT EXISTS episode_sync (
                episode_id TEXT PRIMARY KEY,
                payload_hash TEXT NOT NULL,
                graphiti_uuid TEXT NOT NULL,
                pending_hash TEXT,
                pending_uuid TEXT
            )"""
        )
        try:
            os.chmod(self.path, 0o600)
        except OSError:
            pass

    def get(self, episode_id: str) -> dict[str, Any] | None:
        row = self._db.execute(
            "SELECT payload_hash, graphiti_uuid, pending_hash, pending_uuid "
            "FROM episode_sync WHERE episode_id=?",
            (episode_id,),
        ).fetchone()
        if row is None:
            return None
        keys = ("payload_hash", "graphiti_uuid", "pending_hash", "pending_uuid")
        return dict(zip(keys, row, strict=True))

    def set_pending(self, episode_id: str, payload_hash: str, graphiti_uuid: str) -> None:
        self._db.execute("BEGIN IMMEDIATE")
        try:
            self._db.execute(
                """INSERT INTO episode_sync(
                       episode_id,payload_hash,graphiti_uuid,pending_hash,pending_uuid
                   ) VALUES(?,?,?, ?,?)
                   ON CONFLICT(episode_id) DO UPDATE SET
                       pending_hash=excluded.pending_hash,
                       pending_uuid=excluded.pending_uuid""",
                (episode_id, "", "", payload_hash, graphiti_uuid),
            )
            self._db.execute("COMMIT")
        except Exception:
            self._db.execute("ROLLBACK")
            raise

    def commit_pending(self, episode_id: str) -> None:
        self._db.execute("BEGIN IMMEDIATE")
        try:
            self._db.execute(
                """UPDATE episode_sync
                   SET payload_hash=pending_hash, graphiti_uuid=pending_uuid,
                       pending_hash=NULL, pending_uuid=NULL
                   WHERE episode_id=? AND pending_hash IS NOT NULL""",
                (episode_id,),
            )
            self._db.execute("COMMIT")
        except Exception:
            self._db.execute("ROLLBACK")
            raise

    def close(self) -> None:
        if not self._closed:
            self._db.close()
            self._closed = True
