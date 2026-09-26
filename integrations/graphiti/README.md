# Screenpipe → Graphiti adapter

Optional service for importing finalized Screenpipe Activity Ledger summaries and serving Graphiti-backed relevance search. It is outside screenpipe capture and is not required by the app; both Screenpipe integrations default off.

## Layout and data flow

```text
screenpipe finalized Activity Ledger summaries (auto-sync opt-in)
  → POST /v1/episodes:batch
  → Tailscale Serve / ACL
  → loopback-only adapter on Mac mini
  → Graphiti + local Neo4j

screenpipe relevance query (search opt-in)
  → POST /v1/search
  → Tailscale Serve / ACL
  → loopback-only adapter on Mac mini
  → Graphiti + local Neo4j
       ├─ embeddings: oMLX, qwen3-embedding-0.6b-8bit
       └─ extraction: kb New API, deepseek-v4-flash-0731
```

Both Screenpipe features are independently opt-in and default off. Auto-sync sends only finalized Activity Ledger summary fields and the summary's existing cited source references; raw frames, OCR, audio, and full evidence bodies are never sent. Missing citations are skipped, and first enable establishes a local checkpoint without historical backfill. URL credentials, query strings, and fragments are stripped before transport. Search sends the user's query to Graphiti and returns bounded facts with citations. `mode=relevance` may combine Graphiti with local retrieval and degrades to local results when the adapter is unavailable; explicit `mode=graphiti` returns Graphiti results only and reports adapter failures without local fallback. Graphiti uses its configured LLM for extraction; with DeepSeek this means selected summaries and their metadata may leave the Mac mini for the model provider. Embedding requests stay on the mini when `GRAPHITI_EMBEDDING_BASE_URL` points to its local oMLX listener.

## Development install

Requires Python 3.10+ and `uv`. Graphiti is pinned to 0.30.2; this adapter directly includes `httpx` because that release imports it without declaring it as a dependency.

```bash
cd integrations/graphiti
uv sync --python 3.12
uv run --python 3.12 pytest -q
uv run --python 3.12 ruff check .
```

The adapter does not install Neo4j, change Tailscale, or make live model calls in its test suite.

## Non-secret settings

Set these in the Mac mini service environment. The URLs are examples; confirm the actual oMLX and kb New API listener addresses on the mini before deployment.

```sh
GRAPHITI_ADAPTER_HOST=127.0.0.1
GRAPHITI_ADAPTER_PORT=8765
GRAPHITI_GRAPH_URI=bolt://127.0.0.1:7687
GRAPHITI_GRAPH_USER=neo4j
GRAPHITI_GRAPH_GROUP_ID=screenpipe-personal
GRAPHITI_REQUEST_TIMEOUT_SECONDS=240
GRAPHITI_LLM_BASE_URL=http://127.0.0.1:3000/v1
GRAPHITI_LLM_MODEL=deepseek-v4-flash-0731
GRAPHITI_LLM_KEYCHAIN_SERVICE=kb-model
# Optional non-secret Keychain account selector; omit only if service is unique.
GRAPHITI_LLM_KEYCHAIN_ACCOUNT=<keychain-account>
GRAPHITI_EMBEDDING_BASE_URL=http://127.0.0.1:8000/v1
GRAPHITI_EMBEDDING_MODEL=qwen3-embedding-0.6b-8bit
GRAPHITI_EMBEDDING_DIMENSION=1024
# Optional: set if the oMLX listener requires auth; secret itself stays in Keychain.
GRAPHITI_EMBEDDING_KEYCHAIN_SERVICE=omlx-embedding
GRAPHITI_EMBEDDING_KEYCHAIN_ACCOUNT=<keychain-account>
GRAPHITI_GRAPH_PASSWORD_KEYCHAIN_SERVICE=graphiti-neo4j
GRAPHITI_GRAPH_PASSWORD_KEYCHAIN_ACCOUNT=neo4j
GRAPHITI_TELEMETRY_ENABLED=false
```

`kb-model` is read from macOS Keychain at runtime. Store the local Neo4j password in Keychain under service `graphiti-neo4j`, account `neo4j`. Do not put either credential in `.env`, a launch script, command arguments, logs, or this repository. Missing or ambiguous Keychain entries fail closed. oMLX defaults to the non-secret placeholder `local-omlx` for an unauthenticated local endpoint; if authentication is enabled, set the optional embedding Keychain service/account selectors and keep the secret in Keychain.

## Start the service locally

Start and initialize Neo4j on the mini first, bound to loopback. Configure its password as above. Then start the adapter in the foreground for initial validation:

```sh
cd /absolute/path/to/screenpipe/integrations/graphiti
uv run --python 3.12 screenpipe-graphiti serve
curl http://127.0.0.1:8765/health
```

`/health` reports process liveness only; it does not make a Graphiti write or verify model credentials. The service listens on loopback by default and runs one worker so graph mutations and its local SQLite sync ledger are serialized. `POST /v1/search` uses the adapter-owned `GRAPHITI_GRAPH_GROUP_ID`; callers cannot select another group. Search results are bounded and include only Screenpipe episodes with valid cited source references.

## Share only over Tailscale

Use **Tailscale Serve**, not Funnel. Tailscale Serve provides tailnet HTTPS and obeys tailnet access-control rules. On the mini, after enabling HTTPS certificates and authorizing the intended clients in the tailnet ACL, the current CLI form is:

```sh
tailscale serve 8765
tailscale serve status
```

The Serve URL will look like `https://<mini>.<tailnet>.ts.net`. Keep the adapter and Neo4j bound to `127.0.0.1`; do not expose port 8765 or 7687 directly on LAN or the public internet. See [Tailscale Serve](https://tailscale.com/kb/1312/serve) and verify syntax with `tailscale serve --help` on the installed version. Persistent launchd setup and ACL changes are intentionally not automated here.

## Export and submit

The adapter URL is configurable on each sender invocation; no source change is required:

```sh
screenpipe activity export \
  --start "2h ago" --end now \
  --format graphiti-episode-v1 \
  | uv run --project "/absolute/path/to/screenpipe/integrations/graphiti" \
      --python 3.12 screenpipe-graphiti sync \
      --url "https://<mini>.<tailnet>.ts.net"
```

Or write an inspectable file and replay it:

```sh
screenpipe activity export --start "1d ago" --end now \
  --format graphiti-episode-v1 --out "/absolute/path/activity.jsonl"
uv run --project "/absolute/path/to/screenpipe/integrations/graphiti" \
  --python 3.12 screenpipe-graphiti sync \
  --url "https://<mini>.<tailnet>.ts.net" "/absolute/path/activity.jsonl"
```

Sender batches are bounded, `.ts.net` HTTPS is required except explicit localhost development, and transient HTTP/provider retry results are retried a limited number of times. Results print counts and IDs only, not episode bodies.

The receiver checks the exported payload hash, accepts only private episodes with bounded source references, and stores only episode IDs, hashes and Graphiti UUIDs in its local sync SQLite database. Identical exports are no-ops. Changed payloads use deterministic revision UUIDs: the replacement episode is written before the old episode is removed. Graphiti telemetry is disabled by default; enable it only by explicit configuration.

## Screenpipe settings

Open Settings → AI → Graphiti. Both auto-sync and search default off. Configure the adapter root URL, independent feature toggles, and a sync interval from 30 seconds to 24 hours. The settings are atomically persisted with mode `0600` in `<active data directory>/graphiti-settings.json` and reload at runtime without restarting capture. HTTPS `*.ts.net` is accepted by default; HTTP is restricted to localhost/127.0.0.1/::1 and requires the explicit local-development toggle. Before this file is created, the legacy `SCREENPIPE_GRAPHITI_*` environment variables are used as bootstrap defaults.

`GET /search/records?q=<query>&mode=graphiti` selects Graphiti only. It requires a non-empty query and supports JSON output, limit (capped at 20), time range, app, and window filters. Unsupported filters return `400`; a disabled or unavailable adapter returns an error rather than local results. `mode=relevance` retains its existing mixed local retrieval behavior.

The auto-sync worker starts with Activity Ledger API use, reads finalized summaries through the existing read path, sends one bounded episode at a time, and stores only an atomic cursor checkpoint in the Screenpipe data directory. Configure the adapter's `GRAPHITI_REQUEST_TIMEOUT_SECONDS` for the actual model latency (up to 300 seconds); the sender retries without blocking capture or SQLite writes.

## Not included

This repository change does not install services or models on the Mac mini, configure Keychain items, or change Tailscale ACLs. Automated validation uses synthetic fixtures and mock transports only; no real activity is uploaded.
