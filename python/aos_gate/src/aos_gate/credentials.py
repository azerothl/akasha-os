"""Out-of-band credential injection and proposal secret hygiene.

The agent proposes tool name + non-secret arguments. The host injects
secrets from a vault/session at ``execute`` time. Proposal arguments and
outcome logs must never carry long-lived credentials.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any, Mapping, MutableMapping

# Argument key names (case-insensitive) that must not appear in proposals.
FORBIDDEN_CREDENTIAL_KEYS = frozenset(
    {
        "authorization",
        "api_key",
        "apikey",
        "api-key",
        "x-api-key",
        "token",
        "access_token",
        "refresh_token",
        "id_token",
        "password",
        "passwd",
        "secret",
        "client_secret",
        "private_key",
        "cookie",
        "cookies",
        "bearer",
        "auth_token",
        "session_token",
    }
)

_REDACTED = "***REDACTED***"
_BEARER_RE = re.compile(r"(?i)\bBearer\s+[A-Za-z0-9\-._~+/]+=*")


@dataclass
class CredentialVault:
    """Simple in-process vault: tool_name → secret key → value.

    Production hosts should wrap the OS secret store; this type is the
    contract used by ``AkashaOsToolHost`` and tests.
    """

    secrets: dict[str, dict[str, str]] = field(default_factory=dict)
    #: Global secrets merged into every tool (e.g. default API token name).
    global_secrets: dict[str, str] = field(default_factory=dict)

    def get_for_tool(self, tool_name: str) -> dict[str, str]:
        merged = dict(self.global_secrets)
        merged.update(self.secrets.get(tool_name, {}))
        return merged


def normalize_arg_key(key: str) -> str:
    return key.strip().lower().replace(" ", "_")


def find_credential_keys(arguments: Mapping[str, Any] | None) -> list[str]:
    """Return forbidden credential-shaped keys present in proposal args."""
    if not arguments:
        return []
    found: list[str] = []
    for key in arguments:
        if normalize_arg_key(str(key)) in FORBIDDEN_CREDENTIAL_KEYS:
            found.append(str(key))
    return found


def reject_credential_arguments(
    arguments: Mapping[str, Any] | None,
) -> tuple[bool, str]:
    """Fail closed when proposal args include credential-like keys."""
    bad = find_credential_keys(arguments)
    if not bad:
        return True, "no credential keys in proposal"
    return False, (
        "proposal must not carry credentials; host injects secrets out-of-band "
        f"(forbidden keys: {', '.join(sorted(bad))})"
    )


def redact_value(value: Any) -> Any:
    """Best-effort redaction for logs / outcomes (never persist raw secrets)."""
    if isinstance(value, Mapping):
        return redact_arguments(value)
    if isinstance(value, list):
        return [redact_value(item) for item in value]
    if isinstance(value, str):
        if _BEARER_RE.search(value):
            return _BEARER_RE.sub("Bearer ***REDACTED***", value)
        if len(value) >= 24 and any(ch.isdigit() for ch in value) and any(
            ch.isalpha() for ch in value
        ):
            # Long opaque tokens — keep prefix for debugging only.
            return value[:4] + _REDACTED
        return value
    return value


def redact_arguments(
    arguments: Mapping[str, Any] | None,
) -> dict[str, Any] | None:
    """Copy arguments with credential keys and opaque tokens redacted."""
    if arguments is None:
        return None
    out: dict[str, Any] = {}
    for key, value in arguments.items():
        if normalize_arg_key(str(key)) in FORBIDDEN_CREDENTIAL_KEYS:
            out[str(key)] = _REDACTED
        else:
            out[str(key)] = redact_value(value)
    return out


def inject_credentials(
    tool_name: str,
    arguments: Mapping[str, Any] | None,
    vault: CredentialVault | None,
) -> dict[str, Any]:
    """Merge vault secrets into a copy of arguments for execute only."""
    merged: dict[str, Any] = dict(arguments or {})
    if vault is None:
        return merged
    for key, value in vault.get_for_tool(tool_name).items():
        # Never overwrite a non-secret proposal field accidentally named the same
        # unless the vault explicitly targets it; vault wins for known secret keys.
        merged[key] = value
    return merged


def strip_credentials_from_proposal(
    arguments: Mapping[str, Any] | None,
) -> dict[str, Any]:
    """Drop forbidden keys from a proposal (sanitizer, not a substitute for reject)."""
    if not arguments:
        return {}
    return {
        str(k): v
        for k, v in arguments.items()
        if normalize_arg_key(str(k)) not in FORBIDDEN_CREDENTIAL_KEYS
    }


def mutate_strip_credentials(arguments: MutableMapping[str, Any]) -> list[str]:
    """Remove forbidden keys in-place; return the removed key names."""
    removed: list[str] = []
    for key in list(arguments.keys()):
        if normalize_arg_key(str(key)) in FORBIDDEN_CREDENTIAL_KEYS:
            del arguments[key]
            removed.append(str(key))
    return removed
