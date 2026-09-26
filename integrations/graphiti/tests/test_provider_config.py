# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import os
from types import SimpleNamespace
from unittest.mock import patch

import pytest
from pydantic import ValidationError

from screenpipe_graphiti.config import Settings
from screenpipe_graphiti.graphiti_backend import create_graphiti_backend


def test_request_timeout_allows_slow_graphiti_calls_with_a_hard_cap() -> None:
    assert Settings(request_timeout_seconds=240).request_timeout_seconds == 240
    with pytest.raises(ValidationError):
        Settings(request_timeout_seconds=301)


def test_provider_clients_route_to_configured_services_without_live_calls() -> None:
    captured = {}
    settings = Settings(
        llm_base_url="http://mini-gateway.local/v1",
        llm_model="deepseek-v4-flash-0731",
        llm_keychain_account="kb-account",
        embedding_base_url="http://127.0.0.1:8000/v1",
        embedding_model="qwen3-embedding-0.6b-8bit",
        embedding_dimension=1024,
    )

    def graphiti_factory(**kwargs):
        captured.update(kwargs)
        return SimpleNamespace(close=lambda: None)

    with (
        patch(
            "screenpipe_graphiti.graphiti_backend.read_keychain_secret",
            side_effect=["llm-key", "db-key"],
        ) as secret,
        patch("graphiti_core.Graphiti", side_effect=graphiti_factory),
        patch("graphiti_core.llm_client.OpenAIClient", side_effect=lambda config: ("llm", config)),
        patch(
            "graphiti_core.embedder.OpenAIEmbedder", side_effect=lambda config: ("embedder", config)
        ),
        patch(
            "graphiti_core.cross_encoder.OpenAIRerankerClient",
            side_effect=lambda config: ("reranker", config),
        ),
    ):
        create_graphiti_backend(settings)

    llm_config = captured["llm_client"][1]
    embedding_config = captured["embedder"][1]
    assert llm_config.model == "deepseek-v4-flash-0731"
    assert llm_config.base_url == "http://mini-gateway.local/v1"
    assert llm_config.api_key == "llm-key"
    assert embedding_config.embedding_model == "qwen3-embedding-0.6b-8bit"
    assert embedding_config.base_url == "http://127.0.0.1:8000/v1"
    assert embedding_config.embedding_dim == 1024
    assert embedding_config.api_key == "local-omlx"
    assert secret.call_args_list[0].args == ("kb-model", "kb-account")
    assert secret.call_args_list[1].args == ("graphiti-neo4j", "neo4j")
    assert os.environ["GRAPHITI_TELEMETRY_ENABLED"] == "false"
