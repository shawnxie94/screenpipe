# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import json

import pytest
from pydantic import ValidationError

from screenpipe_graphiti.models import EpisodeEnvelope, compute_payload_hash
from tests.helpers import episode


def test_validates_activity_history_hash_and_keeps_payload_private() -> None:
    payload = episode()
    parsed = EpisodeEnvelope.from_payload(payload)
    body = json.loads(parsed.graphiti_body())
    assert body["screenpipe_episode_id"] == payload["episode_id"]
    assert body["source_description"] == "screenpipe.activity_history"
    assert body["episode"]["activity_id"] == "work:42"
    assert body["episode"]["outcomes"][0]["type"] == "deliverable"
    assert "outcome_type" not in body["episode"]["outcomes"][0]
    assert body["privacy"] == "private"
    assert body["source_refs"][0]["source_type"] == "frame"


def test_rejects_hash_mismatch_before_ingestion() -> None:
    payload = episode()
    payload["episode_body"]["summary"] = "changed without updating the hash"
    with pytest.raises(ValueError, match="payload_hash"):
        EpisodeEnvelope.from_payload(payload)


def test_requires_private_episode_and_source_provenance() -> None:
    payload = episode()
    payload["privacy"] = "public"
    with pytest.raises(ValidationError):
        EpisodeEnvelope.from_payload(payload)

    payload = episode()
    payload["source_refs"] = []
    with pytest.raises(ValidationError):
        EpisodeEnvelope.from_payload(payload)


def test_rejects_bad_interval_and_out_of_window_evidence() -> None:
    payload = episode()
    payload["episode_body"]["end_at"] = "2026-09-22T09:00:00Z"
    payload["payload_hash"] = "sha256:" + "0" * 64
    with pytest.raises(ValidationError):
        EpisodeEnvelope.from_payload(payload)

    payload = episode()
    payload["source_refs"][0]["occurred_at"] = "2026-09-22T10:45:00Z"
    payload["payload_hash"] = compute_payload_hash(
        payload["episode_body"], payload["source_refs"], payload["privacy"]
    )
    with pytest.raises(ValueError, match="outside"):
        EpisodeEnvelope.from_payload(payload)


def test_rejects_episode_activity_identity_mismatch() -> None:
    payload = episode()
    payload["episode_id"] = "screenpipe:activity_history:meeting:99"
    with pytest.raises(ValueError, match="activity_id"):
        EpisodeEnvelope.from_payload(payload)


def test_rejects_activity_ledger_payload_instead_of_supporting_legacy_format() -> None:
    payload = episode()
    payload["source_description"] = "screenpipe.activity_ledger"
    payload["episode_body"]["interval_id"] = 42
    payload["episode_body"].pop("activity_id")
    payload["episode_id"] = "screenpipe:activity:42"
    with pytest.raises(ValidationError):
        EpisodeEnvelope.from_payload(payload)


def test_rejects_extra_evidence_fields_and_duplicate_source_refs() -> None:
    payload = episode()
    payload["source_refs"][0]["browser_url"] = "https://example.test/private"
    with pytest.raises(ValidationError):
        EpisodeEnvelope.from_payload(payload)

    payload = episode()
    payload["source_refs"].append(payload["source_refs"][0].copy())
    payload["payload_hash"] = compute_payload_hash(
        payload["episode_body"], payload["source_refs"], payload["privacy"]
    )
    with pytest.raises(ValueError, match="duplicate"):
        EpisodeEnvelope.from_payload(payload)
