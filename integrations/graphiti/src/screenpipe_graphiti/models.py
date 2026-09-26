# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import hashlib
import json
import re
from datetime import datetime
from typing import Any, Literal
from urllib.parse import urlsplit, urlunsplit

from pydantic import BaseModel, ConfigDict, Field, field_validator

EPISODE_ID = re.compile(r"^screenpipe:activity:[1-9][0-9]*$")
PAYLOAD_HASH = re.compile(r"^sha256:[0-9a-f]{64}$")
MAX_EPISODE_BYTES = 256 * 1024
MAX_SOURCE_REFS = 128


class ActivityOutcome(BaseModel):
    model_config = ConfigDict(extra="forbid")

    outcome_type: str = Field(min_length=1, max_length=128)
    status: str = Field(min_length=1, max_length=80)
    confidence: float = Field(ge=0, le=1)
    provenance: str = Field(min_length=1, max_length=512)


class SourceRef(BaseModel):
    model_config = ConfigDict(extra="forbid")

    source_type: str = Field(min_length=1, max_length=40)
    source_id: int
    occurred_at: datetime
    frame_id: int | None = None
    app_name: str | None = Field(default=None, max_length=256)
    window_title: str | None = Field(default=None, max_length=512)
    browser_url: str | None = Field(default=None, max_length=4096)


class GraphitiSearchRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    query: str = Field(min_length=1, max_length=2048)
    limit: int = Field(default=10, ge=1, le=20)
    start_time: datetime | None = None
    end_time: datetime | None = None

    @field_validator("query")
    @classmethod
    def query_must_not_be_blank(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("query must not be blank")
        return value.strip()

    @field_validator("start_time", "end_time")
    @classmethod
    def times_must_include_timezone(cls, value: datetime | None) -> datetime | None:
        if value is not None and (value.tzinfo is None or value.utcoffset() is None):
            raise ValueError("time bounds must include a timezone")
        return value

    @field_validator("end_time")
    @classmethod
    def end_must_follow_start(cls, value: datetime | None, info: Any) -> datetime | None:
        start = info.data.get("start_time")
        if value is not None and start is not None and value < start:
            raise ValueError("end_time must not precede start_time")
        return value


class GraphitiSearchHit(BaseModel):
    model_config = ConfigDict(extra="forbid")

    uuid: str = Field(min_length=1, max_length=128)
    fact: str = Field(min_length=1, max_length=4096)
    score: float = Field(ge=0, le=1)
    reference_time: datetime
    source_refs: list[SourceRef] = Field(min_length=1, max_length=3)


class GraphitiSearchResponse(BaseModel):
    model_config = ConfigDict(extra="forbid")

    hits: list[GraphitiSearchHit] = Field(max_length=20)


class EpisodeBody(BaseModel):
    model_config = ConfigDict(extra="forbid")

    interval_id: int
    kind: str = Field(min_length=1, max_length=40)
    activity_type: str = Field(min_length=1, max_length=80)
    start_at: datetime
    end_at: datetime
    title: str = Field(max_length=512)
    summary: str | None = Field(default=None, max_length=4096)
    keywords: list[str] = Field(default_factory=list, max_length=64)
    project_refs: list[str] = Field(default_factory=list, max_length=64)
    outcomes: list[ActivityOutcome] = Field(default_factory=list, max_length=64)
    confidence: float = Field(ge=0, le=1)
    status: str = Field(min_length=1, max_length=40)
    producer: str = Field(min_length=1, max_length=128)

    @field_validator("end_at")
    @classmethod
    def end_must_follow_start(cls, value: datetime, info: Any) -> datetime:
        start = info.data.get("start_at")
        if start is not None and value <= start:
            raise ValueError("episode end_at must be later than start_at")
        return value


class EpisodeEnvelope(BaseModel):
    model_config = ConfigDict(extra="forbid")

    episode_id: str = Field(pattern=EPISODE_ID.pattern, max_length=96)
    name: str = Field(min_length=1, max_length=512)
    reference_time: datetime
    source_description: str = Field(min_length=1, max_length=128)
    episode_body: EpisodeBody
    source_refs: list[SourceRef] = Field(min_length=1, max_length=MAX_SOURCE_REFS)
    privacy: Literal["private"]
    payload_hash: str = Field(pattern=PAYLOAD_HASH.pattern)

    @classmethod
    def from_payload(cls, raw: dict[str, Any]) -> EpisodeEnvelope:
        encoded = json.dumps(
            raw, ensure_ascii=False, separators=(",", ":"), allow_nan=False
        ).encode()
        if len(encoded) > MAX_EPISODE_BYTES:
            raise ValueError("episode exceeds the maximum payload size")
        episode = cls.model_validate(raw)
        expected = compute_payload_hash(raw["episode_body"], raw["source_refs"], raw["privacy"])
        if episode.payload_hash != expected:
            raise ValueError("payload_hash does not match episode content")
        if episode.episode_id != f"screenpipe:activity:{episode.episode_body.interval_id}":
            raise ValueError("episode_id does not match interval_id")
        if episode.source_description != "screenpipe.activity_ledger":
            raise ValueError("unsupported source_description")
        if episode.reference_time != episode.episode_body.end_at:
            raise ValueError("reference_time must match episode end_at")
        return episode

    def graphiti_body(self) -> str:
        refs = [
            redact_source_ref(ref.model_dump(mode="json", exclude_none=True))
            for ref in self.source_refs
        ]
        body = {
            "screenpipe_episode_id": self.episode_id,
            "screenpipe_payload_hash": self.payload_hash,
            "source_description": self.source_description,
            "episode": self.episode_body.model_dump(mode="json", exclude_none=True),
            "source_refs": refs,
            "privacy": self.privacy,
        }
        return json.dumps(body, ensure_ascii=False, separators=(",", ":"), allow_nan=False)


def compute_payload_hash(
    body: dict[str, Any], source_refs: list[dict[str, Any]], privacy: str
) -> str:
    """Match the Rust exporter hash: SHA-256 of (body, refs, privacy) JSON tuple."""
    unsigned = json.dumps(
        [body, source_refs, privacy], ensure_ascii=False, separators=(",", ":"), allow_nan=False
    ).encode("utf-8")
    return "sha256:" + hashlib.sha256(unsigned).hexdigest()


def redact_source_ref(ref: dict[str, Any]) -> dict[str, Any]:
    """Drop URL credentials, query strings and fragments before graph/model ingestion."""
    url = ref.get("browser_url")
    if not url:
        return ref
    try:
        parsed = urlsplit(url)
        if parsed.scheme not in {"http", "https"} or not parsed.hostname:
            ref["browser_url"] = None
        else:
            host = parsed.hostname
            if parsed.port:
                host = f"{host}:{parsed.port}"
            ref["browser_url"] = urlunsplit((parsed.scheme, host, parsed.path, "", ""))
    except ValueError:
        ref["browser_url"] = None
    return {key: value for key, value in ref.items() if value is not None}
