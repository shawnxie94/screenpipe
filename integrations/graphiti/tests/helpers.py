# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

from typing import Any

from screenpipe_graphiti.models import compute_payload_hash


def episode(
    activity_id: str = "work:42",
    summary: str = "review release fix",
    interval_id: int | None = None,
) -> dict[str, Any]:
    if interval_id is not None:
        activity_id = f"work:{interval_id}"
    body = {
        "activity_id": activity_id,
        "kind": "work",
        "meeting_id": None,
        "activity_type": "implementation",
        "start_at": "2026-09-22T10:00:00Z",
        "end_at": "2026-09-22T10:30:00Z",
        "title": "Fixed release validation",
        "summary": summary,
        "project_refs": ["screenpipe"],
        "outcomes": [
            {
                "type": "deliverable",
                "status": "observed",
                "confidence": 0.9,
                "provenance": "directly stated",
            }
        ],
        "confidence": 0.8,
        "status": "summarized",
    }
    refs = [
        {
            "source_type": "frame",
            "source_id": 812,
            "occurred_at": "2026-09-22T10:15:00Z",
            "frame_id": 812,
            "app_name": "Code",
        }
    ]
    return {
        "episode_id": f"screenpipe:activity_history:{activity_id}",
        "name": body["title"],
        "reference_time": body["end_at"],
        "source_description": "screenpipe.activity_history",
        "episode_body": body,
        "source_refs": refs,
        "privacy": "private",
        "payload_hash": compute_payload_hash(body, refs, "private"),
    }
