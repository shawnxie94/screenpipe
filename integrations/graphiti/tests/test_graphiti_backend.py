# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import json
from datetime import datetime, timedelta, timezone
from types import SimpleNamespace

from graphiti_core.errors import NodeNotFoundError

from screenpipe_graphiti.graphiti_backend import GraphitiBackend
from screenpipe_graphiti.models import GraphitiSearchRequest


def fake_episode_type(initial_episode=None):
    saved = []

    class FakeEpisodicNode:
        def __init__(self, **values):
            self.values = values

        @classmethod
        async def get_by_uuid(cls, driver, uuid):
            if cls.initial_episode is None:
                raise NodeNotFoundError(uuid)
            return cls.initial_episode

        def model_copy(self, *, update):
            return FakeEpisodicNode(**{**self.values, **update})

        async def save(self, driver):
            saved.append((self.values.copy(), driver))

    FakeEpisodicNode.initial_episode = initial_episode
    return FakeEpisodicNode, saved


def fake_graphiti(reference_time):
    calls = {"schema": 0, "add": []}
    driver = object()

    class FakeGraphiti:
        def __init__(self):
            self.driver = driver

        async def build_indices_and_constraints(self):
            calls["schema"] += 1

        async def add_episode(self, **kwargs):
            calls["add"].append(kwargs)
            episode = SimpleNamespace(
                model_copy=lambda *, update: FakeNode(**{**kwargs, "valid_at": update["valid_at"]})
            )
            return SimpleNamespace(episode=episode)

    class FakeNode:
        def __init__(self, **values):
            self.values = values

        async def save(self, _driver):
            calls.setdefault("result_saves", []).append(self.values.copy())

    return FakeGraphiti(), calls, driver


async def test_add_episode_precreates_deterministic_uuid_outside_temporal_context(monkeypatch):
    reference_time = datetime(2026, 9, 22, 10, 30, tzinfo=timezone.utc)
    FakeNode, saved = fake_episode_type()
    monkeypatch.setattr("graphiti_core.nodes.EpisodicNode", FakeNode)
    graph, calls, driver = fake_graphiti(reference_time)
    backend = GraphitiBackend(graph)

    await backend.add_episode(
        name="synthetic",
        body='{"synthetic":true}',
        source_description="screenpipe.activity_ledger",
        reference_time=reference_time,
        group_id="screenpipe-personal",
        uuid="episode-uuid",
    )

    assert calls["schema"] == 1
    assert calls["add"][0]["uuid"] == "episode-uuid"
    assert saved[0][0]["valid_at"] == reference_time + timedelta(microseconds=1)
    assert saved[0][0]["content"] == '{"synthetic":true}'
    assert calls["result_saves"][-1]["valid_at"] == reference_time
    assert calls["result_saves"][-1]["uuid"] == "episode-uuid"
    assert saved[-1][1] is driver


async def test_search_returns_scoped_facts_with_redacted_source_refs():
    reference_time = datetime(2026, 9, 22, 10, 30, tzinfo=timezone.utc)
    source_ref = {
        "source_type": "frame",
        "source_id": 7,
        "occurred_at": reference_time.isoformat(),
        "browser_url": "https://example.com/path?private=query#fragment",
    }
    node = SimpleNamespace(
        uuid="episode-1",
        group_id="screenpipe-test",
        valid_at=reference_time,
        content=json.dumps({
            "source_description": "screenpipe.activity_ledger",
            "privacy": "private",
            "episode": {"summary": "Synthetic summary"},
            "source_refs": [source_ref],
        }),
    )

    class FakeGraphiti:
        driver = object()

        async def search_(self, query, *, config, group_ids):
            assert query == "synthetic query"
            assert group_ids == ["screenpipe-test"]
            assert config.limit == 4
            return SimpleNamespace(
                edges=[
                    SimpleNamespace(
                        uuid="fact-1",
                        fact="Synthetic fact",
                        episodes=["episode-1"],
                    )
                ],
                edge_reranker_scores=[0.8],
                episodes=[node],
                episode_reranker_scores=[0.7],
            )

    backend = GraphitiBackend(FakeGraphiti())
    hits = await backend.search(
        GraphitiSearchRequest(query="synthetic query", limit=4), "screenpipe-test"
    )

    assert len(hits) == 1
    assert hits[0].uuid == "fact-1"
    assert hits[0].source_refs[0].browser_url == "https://example.com/path"
    assert "private=query" not in hits[0].source_refs[0].browser_url


async def test_add_episode_reuses_pending_node_and_builds_schema_once(monkeypatch):
    reference_time = datetime(2026, 9, 22, 10, 30, tzinfo=timezone.utc)
    FakeNode, saved = fake_episode_type()
    FakeNode.initial_episode = FakeNode(
        uuid="episode-uuid", valid_at=reference_time, name="synthetic"
    )
    monkeypatch.setattr("graphiti_core.nodes.EpisodicNode", FakeNode)
    graph, calls, _ = fake_graphiti(reference_time)
    backend = GraphitiBackend(graph)

    for _ in range(2):
        await backend.add_episode(
            name="synthetic",
            body='{"synthetic":true}',
            source_description="screenpipe.activity_ledger",
            reference_time=reference_time,
            group_id="screenpipe-personal",
            uuid="episode-uuid",
        )

    assert calls["schema"] == 1
    assert len(calls["add"]) == 2
    assert [entry[0]["valid_at"] for entry in saved] == [
        reference_time + timedelta(microseconds=1),
        reference_time + timedelta(microseconds=1),
    ]
    assert calls["result_saves"][-1]["valid_at"] == reference_time
