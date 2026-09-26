# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import asyncio
import json
from collections.abc import Iterable, Iterator
from typing import Any
from urllib.parse import urlsplit

import httpx

from .models import MAX_EPISODE_BYTES


def iter_jsonl(stream: Iterable[str], max_batch_size: int = 32) -> Iterator[list[dict[str, Any]]]:
    if max_batch_size < 1:
        raise ValueError("max_batch_size must be positive")
    batch: list[dict[str, Any]] = []
    for line_number, line in enumerate(stream, 1):
        if not line.strip():
            continue
        try:
            item = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValueError(f"invalid JSONL at line {line_number}") from exc
        if not isinstance(item, dict):
            raise ValueError(f"expected a JSON object at line {line_number}")
        if len(json.dumps(item, ensure_ascii=False).encode("utf-8")) > MAX_EPISODE_BYTES:
            raise ValueError(f"episode exceeds size limit at line {line_number}")
        batch.append(item)
        if len(batch) == max_batch_size:
            yield batch
            batch = []
    if batch:
        yield batch


def validate_target(url: str, allow_http_localhost: bool = False) -> str:
    parsed = urlsplit(url)
    if (
        parsed.scheme == "https"
        and parsed.hostname
        and parsed.hostname.endswith(".ts.net")
        and parsed.username is None
        and parsed.password is None
        and not parsed.query
        and not parsed.fragment
        and parsed.path in {"", "/"}
    ):
        return url.rstrip("/")
    local_hosts = {"localhost", "127.0.0.1", "::1"}
    if allow_http_localhost and parsed.scheme == "http" and parsed.hostname in local_hosts:
        return url.rstrip("/")
    raise ValueError(
        "adapter URL must use a Tailscale .ts.net HTTPS hostname; "
        "HTTP is allowed only for explicit localhost development"
    )


async def send_batches(
    url: str,
    batches: Iterable[list[dict[str, Any]]],
    *,
    timeout_seconds: float = 20,
    allow_http_localhost: bool = False,
    client: httpx.AsyncClient | None = None,
    max_retries: int = 3,
    retry_backoff_seconds: float = 0.25,
) -> tuple[int, int]:
    if max_retries < 0 or timeout_seconds <= 0 or retry_backoff_seconds < 0:
        raise ValueError("retry count and backoff must be non-negative; timeout must be positive")
    target = validate_target(url, allow_http_localhost)
    owned_client = client is None
    client = client or httpx.AsyncClient(timeout=timeout_seconds, follow_redirects=False)
    sent = failed = 0
    try:
        for batch in batches:
            pending = list(batch)
            for attempt in range(max_retries + 1):
                try:
                    response = await client.post(f"{target}/v1/episodes:batch", json=pending)
                except httpx.TransportError:
                    if attempt == max_retries:
                        failed += len(pending)
                        break
                    await asyncio.sleep(retry_backoff_seconds * (2**attempt))
                    continue
                if response.status_code in {408, 425, 429, 500, 502, 503, 504}:
                    if attempt == max_retries:
                        failed += len(pending)
                        break
                    await asyncio.sleep(retry_backoff_seconds * (2**attempt))
                    continue
                if response.status_code >= 400:
                    failed += len(pending)
                    break
                try:
                    results = response.json().get("results", [])
                except (ValueError, AttributeError):
                    failed += len(pending)
                    break
                by_id = {
                    item.get("episode_id"): item.get("status")
                    for item in results
                    if isinstance(item, dict)
                }
                retry_items = []
                for item in pending:
                    status = by_id.pop(item.get("episode_id"), None)
                    if status in {"inserted", "updated", "noop"}:
                        sent += 1
                    elif status == "retry":
                        retry_items.append(item)
                    else:
                        failed += 1
                # Duplicate or malformed result ids cannot turn a partial response
                # into a successful acknowledgment.
                failed += len(by_id)
                pending = retry_items
                if not pending:
                    break
                if attempt == max_retries:
                    failed += len(pending)
                    break
                await asyncio.sleep(retry_backoff_seconds * (2**attempt))
    finally:
        if owned_client:
            await client.aclose()
    return sent, failed
