# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import json

import pytest

from screenpipe_graphiti.models import EpisodeEnvelope
from tests.helpers import episode


def test_validates_export_hash_and_redacts_url_secrets() -> None:
    payload = episode()
    parsed = EpisodeEnvelope.from_payload(payload)
    body = json.loads(parsed.graphiti_body())
    assert body["screenpipe_episode_id"] == payload["episode_id"]
    assert body["source_refs"][0]["browser_url"] == "https://example.test/path"
    assert "token=secret" not in parsed.graphiti_body()


def test_rejects_hash_mismatch_before_ingestion() -> None:
    payload = episode()
    payload["episode_body"]["summary"] = "changed without updating the hash"
    with pytest.raises(ValueError, match="payload_hash"):
        EpisodeEnvelope.from_payload(payload)


def test_requires_private_episode_and_source_provenance() -> None:
    payload = episode()
    payload["privacy"] = "public"
    payload["payload_hash"] = "sha256:" + "0" * 64
    with pytest.raises(ValueError):
        EpisodeEnvelope.from_payload(payload)

    payload = episode()
    payload["source_refs"] = []
    payload["payload_hash"] = "sha256:" + "0" * 64
    with pytest.raises(ValueError):
        EpisodeEnvelope.from_payload(payload)


def test_rejects_bad_interval() -> None:
    payload = episode()
    payload["episode_body"]["end_at"] = "2026-09-22T09:00:00Z"
    payload["payload_hash"] = "sha256:" + "0" * 64
    with pytest.raises(ValueError):
        EpisodeEnvelope.from_payload(payload)


def test_rejects_episode_interval_identity_mismatch() -> None:
    payload = episode()
    payload["episode_id"] = "screenpipe:activity:99"
    with pytest.raises(ValueError, match="interval_id"):
        EpisodeEnvelope.from_payload(payload)
