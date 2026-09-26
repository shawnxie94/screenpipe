# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import asyncio
import json

import httpx
import pytest

from screenpipe_graphiti.sender import iter_jsonl, send_batches, validate_target
from tests.helpers import episode


def test_jsonl_is_bounded_into_batches() -> None:
    rows = [episode(interval_id=value) for value in (1, 2, 3)]
    batches = list(iter_jsonl([json.dumps(row) + "\n" for row in rows], max_batch_size=2))
    assert [len(batch) for batch in batches] == [2, 1]


def test_https_required_except_explicit_localhost() -> None:
    with pytest.raises(ValueError, match="Tailscale"):
        validate_target("http://mini.tailnet.ts.net")
    with pytest.raises(ValueError, match="Tailscale"):
        validate_target("https://public.example")
    assert validate_target("https://mini.tailnet.ts.net") == "https://mini.tailnet.ts.net"
    assert validate_target("http://127.0.0.1:8765", True) == "http://127.0.0.1:8765"


def test_sender_uses_configured_endpoint_and_counts_results() -> None:
    seen = []
    payload = episode()

    async def handler(request: httpx.Request) -> httpx.Response:
        seen.append(str(request.url))
        return httpx.Response(
            200,
            json={"results": [{"episode_id": payload["episode_id"], "status": "inserted"}]},
        )

    client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    sent, failed = asyncio.run(
        send_batches("https://mini.tailnet.ts.net", [[payload]], client=client)
    )
    asyncio.run(client.aclose())
    assert seen == ["https://mini.tailnet.ts.net/v1/episodes:batch"]
    assert (sent, failed) == (1, 0)


def test_sender_retries_transient_service_results() -> None:
    calls = 0
    payload = episode()

    async def handler(request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        status = "retry" if calls == 1 else "noop"
        return httpx.Response(
            200,
            json={"results": [{"episode_id": payload["episode_id"], "status": status}]},
        )

    client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    sent, failed = asyncio.run(
        send_batches(
            "https://mini.tailnet.ts.net",
            [[payload]],
            client=client,
            retry_backoff_seconds=0,
        )
    )
    asyncio.run(client.aclose())
    assert calls == 2
    assert (sent, failed) == (1, 0)


def test_sender_counts_exhausted_provider_retries_as_failed() -> None:
    calls = 0
    payload = episode()

    async def handler(request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        return httpx.Response(
            200,
            json={"results": [{"episode_id": payload["episode_id"], "status": "retry"}]},
        )

    client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    sent, failed = asyncio.run(
        send_batches(
            "https://mini.tailnet.ts.net",
            [[payload]],
            client=client,
            max_retries=1,
            retry_backoff_seconds=0,
        )
    )
    asyncio.run(client.aclose())
    assert calls == 2
    assert (sent, failed) == (0, 1)
