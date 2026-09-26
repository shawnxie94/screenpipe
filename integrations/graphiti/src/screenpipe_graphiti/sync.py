# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import asyncio
import uuid
from datetime import datetime
from typing import Protocol

from graphiti_core.errors import NodeNotFoundError

from .ledger import SyncLedger
from .models import EpisodeEnvelope

_NAMESPACE = uuid.UUID("1f6d3c4a-6cc2-4c0d-9a2c-5bda5c10f61e")


class GraphitiPort(Protocol):
    async def add_episode(
        self,
        *,
        name: str,
        body: str,
        source_description: str,
        reference_time: datetime,
        group_id: str,
        uuid: str,
    ) -> None: ...
    async def remove_episode(self, uuid: str) -> None: ...


class EpisodeIngestor:
    def __init__(self, graph: GraphitiPort, ledger: SyncLedger, group_id: str) -> None:
        self.graph = graph
        self.ledger = ledger
        self.group_id = group_id
        # One process-wide writer keeps graph mutation and local sync metadata
        # serialized without retaining a lock per historical episode.
        self._lock = asyncio.Lock()

    async def ingest(self, episode: EpisodeEnvelope) -> str:
        async with self._lock:
            state = self.ledger.get(episode.episode_id)
            if (
                state
                and state["payload_hash"] == episode.payload_hash
                and not state["pending_hash"]
            ):
                return "noop"

            next_uuid = str(uuid.uuid5(_NAMESPACE, f"{episode.episode_id}:{episode.payload_hash}"))
            old_uuid = state["graphiti_uuid"] if state and state["payload_hash"] else None
            pending_uuid = state["pending_uuid"] if state else None

            # A different retry supersedes an interrupted pending revision. It
            # is safe to remove because the committed revision is untouched
            # until the replacement episode has been stored successfully.
            if pending_uuid and pending_uuid != next_uuid:
                await self._remove_if_present(pending_uuid)

            self.ledger.set_pending(episode.episode_id, episode.payload_hash, next_uuid)
            await self.graph.add_episode(
                name=episode.name,
                body=episode.graphiti_body(),
                source_description=episode.source_description,
                reference_time=episode.reference_time,
                group_id=self.group_id,
                uuid=next_uuid,
            )
            if old_uuid and old_uuid != next_uuid:
                await self._remove_if_present(old_uuid)
            self.ledger.commit_pending(episode.episode_id)
            return "updated" if old_uuid else "inserted"

    async def _remove_if_present(self, graphiti_uuid: str) -> None:
        try:
            await self.graph.remove_episode(graphiti_uuid)
        except NodeNotFoundError:
            return

    async def close(self) -> None:
        close_graph = getattr(self.graph, "close", None)
        if close_graph is not None:
            result = close_graph()
            if asyncio.iscoroutine(result):
                await result
        self.ledger.close()
