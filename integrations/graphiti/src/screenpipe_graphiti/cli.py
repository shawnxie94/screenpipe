# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import argparse
import asyncio
import os
import sys
from pathlib import Path

from .config import Settings
from .sender import iter_jsonl, send_batches


def main() -> int:
    parser = argparse.ArgumentParser(prog="screenpipe-graphiti")
    sub = parser.add_subparsers(dest="command", required=True)
    serve = sub.add_parser("serve", help="run the Graphiti receiver on loopback")
    serve.add_argument("--host", default=None)
    serve.add_argument("--port", type=int, default=None)
    sync = sub.add_parser("sync", help="send graphiti-episode-v1 JSONL to the receiver")
    sync.add_argument("input", nargs="?", default="-")
    sync.add_argument("--url", default=None)
    sync.add_argument("--batch-size", type=int, default=None)
    sync.add_argument("--allow-http-localhost", action="store_true")
    args = parser.parse_args()

    if args.command == "serve":
        import uvicorn

        settings = Settings()
        uvicorn.run(
            "screenpipe_graphiti.service:create_app",
            factory=True,
            host=args.host or settings.adapter_host,
            port=args.port or settings.adapter_port,
            workers=1,
            access_log=False,
        )
        return 0

    settings = Settings()
    url = args.url or settings.adapter_url or os.environ.get("GRAPHITI_ADAPTER_URL")
    if not url:
        parser.error("set --url or GRAPHITI_ADAPTER_URL")
    if args.batch_size is not None and not 1 <= args.batch_size <= settings.max_batch_size:
        parser.error(f"--batch-size must be between 1 and {settings.max_batch_size}")
    stream = sys.stdin if args.input == "-" else Path(args.input).open(encoding="utf-8")
    try:
        sent, failed = asyncio.run(
            send_batches(
                url,
                iter_jsonl(stream, args.batch_size or settings.max_batch_size),
                timeout_seconds=settings.request_timeout_seconds,
                allow_http_localhost=args.allow_http_localhost,
            )
        )
    except (OSError, ValueError) as exc:
        print(f"sync failed: {exc}", file=sys.stderr)
        return 1
    finally:
        if stream is not sys.stdin:
            stream.close()
    print(f"accepted={sent} failed={failed}")
    return 1 if failed else 0
