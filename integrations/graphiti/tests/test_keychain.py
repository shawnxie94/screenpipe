# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

from unittest.mock import patch

import pytest

from screenpipe_graphiti.keychain import SecretUnavailable, read_keychain_secret


def test_reads_secret_from_keychain_without_passing_secret_on_argv() -> None:
    result = type("Result", (), {"returncode": 0, "stdout": "opaque-secret\n"})()
    with (
        patch("screenpipe_graphiti.keychain.platform.system", return_value="Darwin"),
        patch("screenpipe_graphiti.keychain.subprocess.run", return_value=result) as run,
    ):
        assert read_keychain_secret("kb-model", "gateway") == "opaque-secret"
    args = run.call_args.args[0]
    assert args == [
        "/usr/bin/security",
        "find-generic-password",
        "-s",
        "kb-model",
        "-a",
        "gateway",
        "-w",
    ]
    assert "opaque-secret" not in args


def test_missing_keychain_item_fails_closed_without_leaking_stderr() -> None:
    result = type("Result", (), {"returncode": 44, "stdout": "", "stderr": "private detail"})()
    with (
        patch("screenpipe_graphiti.keychain.platform.system", return_value="Darwin"),
        patch("screenpipe_graphiti.keychain.subprocess.run", return_value=result),
    ):
        with pytest.raises(SecretUnavailable, match="missing or ambiguous") as exc:
            read_keychain_secret("kb-model")
        assert "private detail" not in str(exc.value)
