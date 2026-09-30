# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import hashlib
import json
import re
from datetime import datetime
from typing import Any, Literal
from urllib.parse import urlsplit, urlunsplit

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

EPISODE_ID = re.compile(r"^screenpipe:activity_history:.+$")
PAYLOAD_HASH = re.compile(r"^sha256:[0-9a-f]{64}$")
MAX_EPISODE_BYTES = 256 * 1024
MAX_ACTIVITY_HISTORY_EVIDENCE = 3


class ActivityOutcome(BaseModel):
    model_config = ConfigDict(extra="forbid")

    outcome_type: Literal[
        "decision", "deliverable", "commitment", "blocker", "next_step", "unknown"
    ] = Field(alias="type")
    status: Literal["observed", "summarized", "inferred", "confirmed"]
    confidence: float = Field(ge=0, le=1)
    provenance: str = Field(min_length=1, max_length=200)


class SourceRef(BaseModel):
    """Search-result provenance; browser URLs are redacted before returning them."""

    model_config = ConfigDict(extra="forbid")

    source_type: str = Field(min_length=1, max_length=40)
    source_id: int = Field(gt=0)
    occurred_at: datetime
    frame_id: int | None = Field(default=None, gt=0)
    app_name: str | None = Field(default=None, max_length=256)
    window_title: str | None = Field(default=None, max_length=512)
    browser_url: str | None = Field(default=None, max_length=4096)

    @field_validator("occurred_at")
    @classmethod
    def time_must_include_timezone(cls, value: datetime) -> datetime:
        if value.tzinfo is None or value.utcoffset() is None:
            raise ValueError("time must include a timezone")
        return value


class ActivityHistorySourceRef(BaseModel):
    model_config = ConfigDict(extra="forbid")

    source_type: Literal["frame", "audio", "meeting", "ui_event", "parsed"]
    source_id: int = Field(gt=0)
    occurred_at: datetime
    frame_id: int | None = Field(default=None, gt=0)
    app_name: str | None = Field(default=None, max_length=160)

    @field_validator("occurred_at")
    @classmethod
    def time_must_include_timezone(cls, value: datetime) -> datetime:
        if value.tzinfo is None or value.utcoffset() is None:
            raise ValueError("time must include a timezone")
        return value


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


class ActivityHistoryEpisodeBody(BaseModel):
    model_config = ConfigDict(extra="forbid")

    activity_id: str = Field(min_length=1, max_length=160)
    kind: Literal["work", "meeting"]
    meeting_id: int | None = Field(default=None, gt=0)
    activity_type: Literal[
        "meeting",
        "research",
        "implementation",
        "planning",
        "communication",
        "learning",
        "administrative",
        "unknown",
    ]
    start_at: datetime
    end_at: datetime
    title: str = Field(min_length=1, max_length=240)
    summary: str = Field(min_length=1, max_length=8000)
    project_refs: list[str] = Field(max_length=32)
    outcomes: list[ActivityOutcome] = Field(max_length=12)
    confidence: float = Field(ge=0, le=1)
    status: Literal["summarized", "inferred"]

    @field_validator("start_at", "end_at")
    @classmethod
    def times_must_include_timezone(cls, value: datetime) -> datetime:
        if value.tzinfo is None or value.utcoffset() is None:
            raise ValueError("episode times must include a timezone")
        return value

    @model_validator(mode="after")
    def validate_interval(self) -> ActivityHistoryEpisodeBody:
        if self.start_at >= self.end_at:
            raise ValueError("episode end_at must be later than start_at")
        if self.kind == "meeting" and self.meeting_id is None:
            raise ValueError("meeting episode requires meeting_id")
        if any(not value.strip() or len(value) > 120 for value in self.project_refs):
            raise ValueError("invalid project_refs")
        if len(set(self.project_refs)) != len(self.project_refs):
            raise ValueError("duplicate project_refs")
        return self


class EpisodeEnvelope(BaseModel):
    model_config = ConfigDict(extra="forbid")

    episode_id: str = Field(pattern=EPISODE_ID.pattern, max_length=200)
    name: str = Field(min_length=1, max_length=240)
    reference_time: datetime
    source_description: Literal["screenpipe.activity_history"]
    episode_body: ActivityHistoryEpisodeBody
    source_refs: list[ActivityHistorySourceRef] = Field(
        min_length=1, max_length=MAX_ACTIVITY_HISTORY_EVIDENCE
    )
    privacy: Literal["private"]
    payload_hash: str = Field(pattern=PAYLOAD_HASH.pattern)

    @field_validator("reference_time")
    @classmethod
    def reference_time_must_include_timezone(cls, value: datetime) -> datetime:
        if value.tzinfo is None or value.utcoffset() is None:
            raise ValueError("reference_time must include a timezone")
        return value

    @classmethod
    def from_payload(cls, raw: dict[str, Any]) -> EpisodeEnvelope:
        encoded = json.dumps(
            raw, ensure_ascii=False, separators=(",", ":"), allow_nan=False
        ).encode()
        if len(encoded) > MAX_EPISODE_BYTES:
            raise ValueError("episode exceeds the maximum payload size")
        episode = cls.model_validate(raw)
        expected = compute_payload_hash(
            raw["episode_body"], raw["source_refs"], raw["privacy"]
        )
        if episode.payload_hash != expected:
            raise ValueError("payload_hash does not match episode content")
        body = episode.episode_body
        if episode.episode_id != f"screenpipe:activity_history:{body.activity_id}":
            raise ValueError("episode_id does not match activity_id")
        if episode.reference_time != body.end_at:
            raise ValueError("reference_time must match episode end_at")
        if episode.name != body.title:
            raise ValueError("name must match episode title")
        if any(
            ref.occurred_at < body.start_at or ref.occurred_at > body.end_at
            for ref in episode.source_refs
        ):
            raise ValueError("source_ref is outside the episode interval")
        identities = {(ref.source_type, ref.source_id) for ref in episode.source_refs}
        if len(identities) != len(episode.source_refs):
            raise ValueError("duplicate source_refs")
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
            "episode": self.episode_body.model_dump(
                mode="json", exclude_none=True, by_alias=True
            ),
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
