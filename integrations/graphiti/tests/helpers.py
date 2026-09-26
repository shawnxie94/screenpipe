# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

from typing import Any

from screenpipe_graphiti.models import compute_payload_hash


def episode(interval_id: int = 42, summary: str = "review release fix") -> dict[str, Any]:
    body = {
        "interval_id": interval_id,
        "kind": "work",
        "activity_type": "implementation",
        "start_at": "2026-09-22T10:00:00Z",
        "end_at": "2026-09-22T10:30:00Z",
        "title": "Fixed release validation",
        "summary": summary,
        "keywords": ["release"],
        "project_refs": ["screenpipe"],
        "outcomes": [],
        "confidence": 0.8,
        "status": "summarized",
        "producer": "activity-ledger",
    }
    refs = [
        {
            "source_type": "screen",
            "source_id": 812,
            "occurred_at": "2026-09-22T10:15:00Z",
            "frame_id": 812,
            "app_name": "Code",
            "window_title": "screenpipe",
            "browser_url": "https://example.test/path?token=secret#fragment",
        }
    ]
    return {
        "episode_id": f"screenpipe:activity:{interval_id}",
        "name": body["title"],
        "reference_time": "2026-09-22T10:30:00Z",
        "source_description": "screenpipe.activity_ledger",
        "episode_body": body,
        "source_refs": refs,
        "privacy": "private",
        "payload_hash": compute_payload_hash(body, refs, "private"),
    }
