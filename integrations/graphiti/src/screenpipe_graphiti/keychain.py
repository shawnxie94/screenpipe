# screenpipe — AI that knows everything you've seen, said, or heard
# https://screenpipe.com

from __future__ import annotations

import platform
import subprocess


class SecretUnavailable(RuntimeError):
    pass


def read_keychain_secret(service: str, account: str | None = None) -> str:
    """Read one Keychain generic-password item without exposing its value in argv/logs."""
    if platform.system() != "Darwin":
        raise SecretUnavailable("macOS Keychain is required for configured credentials")
    command = ["/usr/bin/security", "find-generic-password", "-s", service]
    if account:
        command.extend(["-a", account])
    command.append("-w")
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=8, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise SecretUnavailable("could not read the configured Keychain item") from exc
    secret = result.stdout.strip()
    if result.returncode != 0 or not secret:
        raise SecretUnavailable("configured Keychain item is missing or ambiguous")
    return secret
