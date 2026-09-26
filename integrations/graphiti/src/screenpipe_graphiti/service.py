# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator, Callable
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI, HTTPException, Request
from pydantic import TypeAdapter, ValidationError

from .config import Settings
from .graphiti_backend import create_graphiti_backend
from .ledger import SyncLedger
from .models import EpisodeEnvelope, GraphitiSearchRequest
from .sync import EpisodeIngestor

_BATCH_ADAPTER = TypeAdapter(list[dict[str, Any]])
_SEARCH_ADAPTER = TypeAdapter(GraphitiSearchRequest)
MAX_SEARCH_REQUEST_BYTES = 16 * 1024


def create_app(
    settings: Settings | None = None,
    ingestor_factory: Callable[[], EpisodeIngestor] | None = None,
    searcher_factory: Callable[[], Any] | None = None,
) -> FastAPI:
    config = settings or Settings()

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        yield
        if app.state.ingestor is not None:
            await app.state.ingestor.close()
        elif app.state.backend is not None:
            await app.state.backend.close()

    app = FastAPI(
        title="screenpipe Graphiti adapter",
        docs_url=None,
        redoc_url=None,
        lifespan=lifespan,
    )
    app.state.ingestor = None
    app.state.ingestor_factory = ingestor_factory
    app.state.ingestor_lock = asyncio.Lock()
    app.state.backend = None
    app.state.backend_lock = asyncio.Lock()
    app.state.searcher_factory = searcher_factory

    @app.get("/health")
    async def health() -> dict[str, str]:
        return {"status": "ok"}

    async def get_backend() -> Any:
        if app.state.backend is not None:
            return app.state.backend
        async with app.state.backend_lock:
            if app.state.backend is None:
                app.state.backend = create_graphiti_backend(config)
        return app.state.backend

    async def get_searcher() -> Any:
        if app.state.searcher_factory is not None:
            return app.state.searcher_factory()
        return await get_backend()

    async def get_ingestor() -> EpisodeIngestor:
        if app.state.ingestor is not None:
            return app.state.ingestor
        async with app.state.ingestor_lock:
            if app.state.ingestor is None:
                if app.state.ingestor_factory:
                    app.state.ingestor = app.state.ingestor_factory()
                else:
                    graph = await get_backend()
                    ledger = SyncLedger(config.sync_db_path)
                    app.state.ingestor = EpisodeIngestor(graph, ledger, config.graph_group_id)
        return app.state.ingestor

    @app.post("/v1/search")
    async def search_graphiti(request: Request) -> dict[str, Any]:
        raw_body = bytearray()
        async for chunk in request.stream():
            raw_body.extend(chunk)
            if len(raw_body) > MAX_SEARCH_REQUEST_BYTES:
                raise HTTPException(status_code=413, detail="search request exceeds size limit")
        try:
            payload = _SEARCH_ADAPTER.validate_python(json.loads(raw_body))
        except (json.JSONDecodeError, ValidationError, TypeError, ValueError):
            raise HTTPException(status_code=400, detail="invalid Graphiti search request") from None
        try:
            searcher = await get_searcher()
            response = await asyncio.wait_for(
                searcher.search(payload, config.graph_group_id),
                timeout=config.request_timeout_seconds,
            )
        except TimeoutError:
            raise HTTPException(status_code=504, detail="Graphiti search timed out") from None
        except Exception:
            # Never echo graph/provider errors or the private query context.
            raise HTTPException(status_code=503, detail="Graphiti search is unavailable") from None
        return {"hits": [hit.model_dump(mode="json") for hit in response]}

    @app.post("/v1/episodes:batch")
    async def ingest_batch(request: Request) -> dict[str, Any]:
        raw_body = bytearray()
        async for chunk in request.stream():
            raw_body.extend(chunk)
            if len(raw_body) > config.max_request_bytes:
                raise HTTPException(status_code=413, detail="request exceeds configured size limit")
        try:
            payload = json.loads(raw_body)
            raw_episodes = _BATCH_ADAPTER.validate_python(payload)
        except (json.JSONDecodeError, ValidationError, TypeError, ValueError):
            raise HTTPException(status_code=400, detail="invalid episode batch") from None
        if not raw_episodes or len(raw_episodes) > config.max_batch_size:
            raise HTTPException(status_code=413, detail="batch count exceeds configured limit")
        try:
            episodes = [EpisodeEnvelope.from_payload(raw) for raw in raw_episodes]
        except (ValidationError, KeyError, ValueError, TypeError):
            raise HTTPException(
                status_code=422, detail="episode failed schema or hash validation"
            ) from None
        try:
            ingestor = await get_ingestor()
        except Exception:
            raise HTTPException(status_code=503, detail="Graphiti service is unavailable") from None
        results = []
        for episode in episodes:
            try:
                outcome = await asyncio.wait_for(
                    ingestor.ingest(episode), timeout=config.request_timeout_seconds
                )
                results.append({"episode_id": episode.episode_id, "status": outcome})
            except TimeoutError:
                results.append({"episode_id": episode.episode_id, "status": "retry"})
            except Exception:
                # Do not return provider exceptions, which may contain request
                # bodies, credentials, internal hosts or graph details.
                results.append({"episode_id": episode.episode_id, "status": "retry"})
        return {"results": results}

    return app
