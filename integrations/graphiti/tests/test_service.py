# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import asyncio

from fastapi.testclient import TestClient

from screenpipe_graphiti.config import Settings
from screenpipe_graphiti.ledger import SyncLedger
from screenpipe_graphiti.models import GraphitiSearchHit
from screenpipe_graphiti.service import MAX_SEARCH_REQUEST_BYTES, create_app
from screenpipe_graphiti.sync import EpisodeIngestor
from tests.helpers import episode


class FakeGraph:
    async def add_episode(self, **kwargs) -> None:
        return None

    async def remove_episode(self, uuid: str) -> None:
        return None


def test_health_and_batch_ingestion(tmp_path) -> None:
    ledger = SyncLedger(str(tmp_path / "sync.sqlite3"))
    app = create_app(
        Settings(
            llm_base_url="http://localhost:3000/v1", embedding_base_url="http://localhost:8000/v1"
        ),
        lambda: EpisodeIngestor(FakeGraph(), ledger, "test"),
    )
    with TestClient(app) as client:
        assert client.get("/health").json() == {"status": "ok"}
        result = client.post("/v1/episodes:batch", json=[episode()])
        assert result.status_code == 200
        assert result.json()["results"][0]["status"] == "inserted"
    ledger.close()


def test_search_endpoint_scopes_group_and_preserves_source_refs() -> None:
    from datetime import datetime, timezone

    calls = []

    class FakeSearcher:
        async def search(self, request, group_id):
            calls.append((request, group_id))
            return [
                GraphitiSearchHit(
                    uuid="fact-1",
                    fact="Synthetic fact",
                    score=0.75,
                    reference_time=datetime(2026, 9, 22, tzinfo=timezone.utc),
                    source_refs=[{
                        "source_type": "frame",
                        "source_id": 17,
                        "occurred_at": datetime(2026, 9, 22, tzinfo=timezone.utc),
                    }],
                )
            ]

    app = create_app(
        Settings(
            llm_base_url="http://localhost:3000/v1",
            embedding_base_url="http://localhost:8000/v1",
            graph_group_id="screenpipe-test",
        ),
        searcher_factory=FakeSearcher,
    )
    with TestClient(app) as client:
        response = client.post("/v1/search", json={"query": "synthetic", "limit": 3})
        assert response.status_code == 200
        assert response.json()["hits"][0]["source_refs"][0]["source_id"] == 17
        assert calls[0][0].query == "synthetic"
        assert calls[0][1] == "screenpipe-test"
        assert client.post("/v1/search", json={"query": "  "}).status_code == 400
        invalid_scope = client.post(
            "/v1/search", json={"query": "synthetic", "group_id": "other"}
        )
        assert invalid_scope.status_code == 400
        naive_time = client.post(
            "/v1/search", json={"query": "synthetic", "start_time": "2026-09-22T10:00:00"}
        )
        assert naive_time.status_code == 400
        oversized = client.post("/v1/search", content=b" " * (MAX_SEARCH_REQUEST_BYTES + 1))
        assert oversized.status_code == 413


def test_search_failure_and_timeout_do_not_echo_provider_details() -> None:
    class FailingSearcher:
        async def search(self, _request, _group_id):
            raise RuntimeError("private query payload and provider credential")

    class SlowSearcher:
        async def search(self, _request, _group_id):
            await asyncio.sleep(0.05)
            return []

    failing_app = create_app(
        Settings(
            llm_base_url="http://localhost:3000/v1",
            embedding_base_url="http://localhost:8000/v1",
        ),
        searcher_factory=FailingSearcher,
    )
    with TestClient(failing_app) as client:
        response = client.post("/v1/search", json={"query": "synthetic"})
        assert response.status_code == 503
        assert "private query" not in response.text

    slow_app = create_app(
        Settings(
            llm_base_url="http://localhost:3000/v1",
            embedding_base_url="http://localhost:8000/v1",
            request_timeout_seconds=0.001,
        ),
        searcher_factory=SlowSearcher,
    )
    with TestClient(slow_app) as client:
        assert client.post("/v1/search", json={"query": "synthetic"}).status_code == 504


def test_rejects_bad_hash_and_oversized_request_without_echoing_payload(tmp_path) -> None:
    ledger = SyncLedger(str(tmp_path / "sync.sqlite3"))
    app = create_app(
        Settings(
            llm_base_url="http://localhost:3000/v1",
            embedding_base_url="http://localhost:8000/v1",
            max_request_bytes=4096,
        ),
        lambda: EpisodeIngestor(FakeGraph(), ledger, "test"),
    )
    with TestClient(app) as client:
        bad = episode()
        bad["payload_hash"] = "sha256:" + "0" * 64
        response = client.post("/v1/episodes:batch", json=[bad])
        assert response.status_code == 422
        assert "review release fix" not in response.text
        response = client.post("/v1/episodes:batch", content=b" " * 5000)
        assert response.status_code == 413
    ledger.close()
