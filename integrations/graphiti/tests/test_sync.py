# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

from graphiti_core.errors import NodeNotFoundError

from screenpipe_graphiti.ledger import SyncLedger
from screenpipe_graphiti.models import EpisodeEnvelope
from screenpipe_graphiti.sync import EpisodeIngestor
from tests.helpers import episode


class FakeGraph:
    def __init__(self) -> None:
        self.episodes: dict[str, str] = {}
        self.add_calls = 0
        self.fail_next_add = False

    async def add_episode(
        self, *, name, body, source_description, reference_time, group_id, uuid
    ) -> None:
        self.add_calls += 1
        if self.fail_next_add:
            self.fail_next_add = False
            raise RuntimeError("synthetic provider failure")
        self.episodes[uuid] = body

    async def remove_episode(self, uuid: str) -> None:
        if uuid not in self.episodes:
            raise NodeNotFoundError(uuid)
        del self.episodes[uuid]


def test_new_duplicate_and_changed_episode_are_idempotent(tmp_path) -> None:
    graph = FakeGraph()
    ledger = SyncLedger(str(tmp_path / "sync.sqlite3"))
    ingestor = EpisodeIngestor(graph, ledger, "screenpipe")
    first = EpisodeEnvelope.from_payload(episode())

    assert run(ingestor.ingest(first)) == "inserted"
    first_uuid = ledger.get(first.episode_id)["graphiti_uuid"]
    assert run(ingestor.ingest(first)) == "noop"
    assert graph.add_calls == 1

    changed_raw = episode(summary="confirmed release validation fix")
    changed = EpisodeEnvelope.from_payload(changed_raw)
    assert run(ingestor.ingest(changed)) == "updated"
    state = ledger.get(changed.episode_id)
    assert state["payload_hash"] == changed.payload_hash
    assert first_uuid not in graph.episodes
    assert len(graph.episodes) == 1
    ledger.close()


def test_failed_add_keeps_old_episode_and_retry_recovers(tmp_path) -> None:
    graph = FakeGraph()
    ledger = SyncLedger(str(tmp_path / "sync.sqlite3"))
    ingestor = EpisodeIngestor(graph, ledger, "screenpipe")
    first = EpisodeEnvelope.from_payload(episode())
    run(ingestor.ingest(first))
    old_uuid = ledger.get(first.episode_id)["graphiti_uuid"]

    graph.fail_next_add = True
    changed = EpisodeEnvelope.from_payload(episode(summary="updated conclusion"))
    try:
        run(ingestor.ingest(changed))
        raise AssertionError("expected synthetic provider failure")
    except RuntimeError:
        pass
    assert old_uuid in graph.episodes
    assert ledger.get(first.episode_id)["payload_hash"] == first.payload_hash

    assert run(ingestor.ingest(changed)) == "updated"
    assert old_uuid not in graph.episodes
    assert ledger.get(first.episode_id)["payload_hash"] == changed.payload_hash
    ledger.close()


def run(awaitable):
    import asyncio

    return asyncio.run(awaitable)
