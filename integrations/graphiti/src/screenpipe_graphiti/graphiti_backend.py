# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import asyncio
import json
import math
import os
from datetime import datetime, timedelta
from typing import Any

from .config import Settings
from .keychain import read_keychain_secret
from .models import GraphitiSearchHit, GraphitiSearchRequest, SourceRef, redact_source_ref


class GraphitiBackend:
    def __init__(self, graphiti: Any) -> None:
        self.graphiti = graphiti
        self._schema_ready = False
        self._schema_lock = asyncio.Lock()

    async def _ensure_schema(self) -> None:
        if self._schema_ready:
            return
        async with self._schema_lock:
            if not self._schema_ready:
                await self.graphiti.build_indices_and_constraints()
                self._schema_ready = True

    async def add_episode(
        self,
        *,
        name: str,
        body: str,
        source_description: str,
        reference_time: Any,
        group_id: str,
        uuid: str,
    ) -> None:
        from graphiti_core.errors import NodeNotFoundError
        from graphiti_core.nodes import EpisodeType, EpisodicNode

        await self._ensure_schema()
        driver = self.graphiti.driver
        # Graphiti 0.30.2 interprets add_episode(uuid=...) as an update and
        # requires that the episode node already exists. Keep its deterministic
        # UUID by writing a retryable placeholder just beyond the reference
        # time, so Graphiti's previous-episode query cannot retrieve itself.
        valid_at = reference_time + timedelta(microseconds=1)
        try:
            episode = await EpisodicNode.get_by_uuid(driver, uuid)
            episode = episode.model_copy(update={"valid_at": valid_at})
        except NodeNotFoundError:
            episode = EpisodicNode(
                uuid=uuid,
                name=name,
                group_id=group_id,
                source=EpisodeType.json,
                source_description=source_description,
                content=body,
                valid_at=valid_at,
            )
        await episode.save(driver)

        result = await self.graphiti.add_episode(
            name=name,
            episode_body=body,
            source_description=source_description,
            reference_time=reference_time,
            source=EpisodeType.json,
            group_id=group_id,
            uuid=uuid,
        )
        # Restore the event's exact timestamp after Graphiti has excluded the
        # placeholder from its own temporal context query.
        stored_episode = result.episode.model_copy(update={"valid_at": reference_time})
        await stored_episode.save(driver)

    async def search(
        self, request: GraphitiSearchRequest, group_id: str
    ) -> list[GraphitiSearchHit]:
        from graphiti_core.nodes import EpisodicNode
        from graphiti_core.search.search_config_recipes import COMBINED_HYBRID_SEARCH_CROSS_ENCODER

        config = COMBINED_HYBRID_SEARCH_CROSS_ENCODER.model_copy(
            deep=True, update={"limit": request.limit}
        )
        result = await self.graphiti.search_(
            request.query, config=config, group_ids=[group_id]
        )

        episode_nodes = {node.uuid: node for node in result.episodes}
        referenced_ids = list(
            dict.fromkeys(
                episode_id
                for edge in result.edges
                for episode_id in edge.episodes[:8]
            )
        )[: request.limit * 8]
        missing_ids = [
            episode_id for episode_id in referenced_ids if episode_id not in episode_nodes
        ]
        if missing_ids:
            fetched = await asyncio.gather(
                *(
                    EpisodicNode.get_by_uuid(self.graphiti.driver, episode_id)
                    for episode_id in missing_ids
                ),
                return_exceptions=True,
            )
            for episode_id, node in zip(missing_ids, fetched, strict=True):
                if not isinstance(node, BaseException):
                    episode_nodes[episode_id] = node

        def refs_for(episode_ids: list[str]) -> tuple[list[SourceRef], list[datetime]]:
            refs: list[SourceRef] = []
            times: list[datetime] = []
            seen: set[tuple[str, int, str]] = set()
            for episode_id in episode_ids[:8]:
                node = episode_nodes.get(episode_id)
                if node is None:
                    continue
                if node.group_id != group_id:
                    continue
                reference_time = node.valid_at
                if request.start_time is not None and reference_time < request.start_time:
                    continue
                if request.end_time is not None and reference_time > request.end_time:
                    continue
                try:
                    payload = json.loads(node.content)
                except (TypeError, json.JSONDecodeError):
                    continue
                if not isinstance(payload, dict):
                    continue
                if (
                    payload.get("source_description") != "screenpipe.activity_ledger"
                    or payload.get("privacy") != "private"
                ):
                    continue
                times.append(reference_time)
                raw_refs = payload.get("source_refs", [])
                if not isinstance(raw_refs, list):
                    continue
                for raw_ref in raw_refs:
                    try:
                        ref = SourceRef.model_validate(
                            redact_source_ref(dict(raw_ref))
                        )
                    except (TypeError, ValueError):
                        continue
                    identity = (ref.source_type, ref.source_id, ref.occurred_at.isoformat())
                    if identity not in seen:
                        seen.add(identity)
                        refs.append(ref)
                    if len(refs) >= 3:
                        break
                if len(refs) >= 3:
                    break
            return refs, times

        hits: list[GraphitiSearchHit] = []
        for index, edge in enumerate(result.edges):
            fact = (edge.fact or "").strip()
            source_refs, times = refs_for(edge.episodes)
            if not fact or not source_refs or not times:
                continue
            raw_score = (
                result.edge_reranker_scores[index]
                if index < len(result.edge_reranker_scores)
                else 1 / (index + 1)
            )
            numeric_score = float(raw_score)
            score = min(1.0, max(0.0, numeric_score)) if math.isfinite(numeric_score) else 0.0
            hits.append(
                GraphitiSearchHit(
                    uuid=edge.uuid,
                    fact=fact[:4096],
                    score=score,
                    reference_time=max(times),
                    source_refs=source_refs,
                )
            )
            if len(hits) >= request.limit:
                break

        if hits:
            return hits

        # Graphiti may find the source episode before entity extraction has
        # produced a fact edge. Returning the finalized summary keeps that
        # indexed content searchable while preserving its local citations.
        for index, node in enumerate(result.episodes):
            source_refs, times = refs_for([node.uuid])
            if not source_refs or not times:
                continue
            try:
                payload = json.loads(node.content)
                summary = str(
                    payload["episode"].get("summary")
                    or payload["episode"].get("title")
                    or ""
                ).strip()
            except (KeyError, TypeError, json.JSONDecodeError):
                continue
            if not summary:
                continue
            raw_score = (
                result.episode_reranker_scores[index]
                if index < len(result.episode_reranker_scores)
                else 1 / (index + 1)
            )
            numeric_score = float(raw_score)
            score = min(1.0, max(0.0, numeric_score)) if math.isfinite(numeric_score) else 0.0
            hits.append(
                GraphitiSearchHit(
                    uuid=node.uuid,
                    fact=summary[:4096],
                    score=score,
                    reference_time=max(times),
                    source_refs=source_refs,
                )
            )
            if len(hits) >= request.limit:
                break
        return hits

    async def remove_episode(self, uuid: str) -> None:
        from graphiti_core.errors import NodeNotFoundError

        try:
            await self.graphiti.remove_episode(uuid)
        except NodeNotFoundError:
            # Removal is intentionally idempotent for crash recovery.
            return

    async def close(self) -> None:
        await self.graphiti.close()


def create_graphiti_backend(settings: Settings) -> GraphitiBackend:
    if not settings.llm_base_url or not settings.embedding_base_url:
        raise ValueError("Graphiti LLM and embedding base URLs must be configured")

    # Graphiti 0.30.x enables anonymous PostHog telemetry by default. Keep the
    # optional adapter fail-closed unless an operator explicitly opts in.
    os.environ["GRAPHITI_TELEMETRY_ENABLED"] = str(settings.graphiti_telemetry_enabled).lower()

    from graphiti_core import Graphiti
    from graphiti_core.cross_encoder import OpenAIRerankerClient
    from graphiti_core.embedder import OpenAIEmbedder, OpenAIEmbedderConfig
    from graphiti_core.llm_client import OpenAIClient
    from graphiti_core.llm_client.config import LLMConfig

    llm_key = read_keychain_secret(settings.llm_keychain_service, settings.llm_keychain_account)
    graph_password = read_keychain_secret(
        settings.graph_password_keychain_service, settings.graph_password_keychain_account
    )
    embedding_key = (
        read_keychain_secret(
            settings.embedding_keychain_service, settings.embedding_keychain_account
        )
        if settings.embedding_keychain_service
        else "local-omlx"
    )
    llm_config = LLMConfig(
        api_key=llm_key,
        model=settings.llm_model,
        small_model=settings.llm_model,
        base_url=settings.llm_base_url,
    )
    llm = OpenAIClient(config=llm_config)
    embedder = OpenAIEmbedder(
        config=OpenAIEmbedderConfig(
            api_key=embedding_key,
            embedding_model=settings.embedding_model,
            embedding_dim=settings.embedding_dimension,
            base_url=settings.embedding_base_url,
        )
    )
    reranker = OpenAIRerankerClient(config=llm_config)
    graph = Graphiti(
        uri=settings.graph_uri,
        user=settings.graph_user,
        password=graph_password,
        llm_client=llm,
        embedder=embedder,
        cross_encoder=reranker,
    )
    return GraphitiBackend(graph)
