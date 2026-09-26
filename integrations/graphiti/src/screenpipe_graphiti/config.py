# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="GRAPHITI_", extra="ignore")

    adapter_host: str = "127.0.0.1"
    adapter_port: int = Field(default=8765, ge=1, le=65535)
    adapter_url: str | None = None
    allow_http_localhost: bool = False
    max_batch_size: int = Field(default=32, ge=1, le=128)
    max_request_bytes: int = Field(default=2 * 1024 * 1024, ge=1024, le=16 * 1024 * 1024)
    request_timeout_seconds: float = Field(default=20, gt=0, le=300)
    sync_db_path: str = "~/Library/Application Support/screenpipe-graphiti/sync.sqlite3"

    graph_uri: str = "bolt://127.0.0.1:7687"
    graph_user: str = "neo4j"
    graph_password_keychain_service: str = "graphiti-neo4j"
    graph_password_keychain_account: str = "neo4j"
    graph_group_id: str = "screenpipe-personal"

    llm_base_url: str | None = None
    llm_model: str = "deepseek-v4-flash-0731"
    llm_keychain_service: str = "kb-model"
    llm_keychain_account: str | None = None

    embedding_base_url: str | None = None
    embedding_model: str = "qwen3-embedding-0.6b-8bit"
    embedding_dimension: int = Field(default=1024, ge=1, le=4096)
    embedding_keychain_service: str | None = None
    embedding_keychain_account: str | None = None

    graphiti_telemetry_enabled: bool = False
