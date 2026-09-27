"""``ToolHost`` implementation for Akasha OS (permissions + side effects)."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, Mapping, Protocol

from akasha_model.tool_calling import ToolSpec

from .credentials import (
    CredentialVault,
    inject_credentials,
    reject_credential_arguments,
)
from .trust import SourceTrust, trust_allows_execution

# Mirror crates/aos-agent/src/policy.rs — keep in sync when AgentPolicy changes.
NET_TOOLS = frozenset({"web.search", "web.browse", "net.fetch"})

HOST_PATH_PREFIXES = (
    "e:/",
    "e:\\",
    "c:/",
    "c:\\",
    "/mnt/",
    "/media/",
    "/Volumes/",
)


class OsExecutor(Protocol):
    """OS-side tool runner. Must be the same path audits already protect."""

    def execute(self, tool_name: str, arguments: Mapping[str, Any] | None) -> Any:
        """Perform the real side effect (bus invoke / module / native)."""


@dataclass
class RecordingExecutor:
    """Test / demo executor that records calls and never touches the bus."""

    log: list[dict[str, Any]] = field(default_factory=list)
    handler: Callable[[str, Mapping[str, Any] | None], Any] | None = None

    def execute(self, tool_name: str, arguments: Mapping[str, Any] | None) -> Any:
        record = {"tool": tool_name, "arguments": dict(arguments or {})}
        self.log.append(record)
        if self.handler is not None:
            return self.handler(tool_name, arguments)
        return {"ok": True, "echo": record}


@dataclass
class AkashaOsToolHost:
    """Akasha OS ``ToolHost``: existing policy/caps checks + OS executor.

    ``dispatch_plan`` / ``run_gated_call`` call ``execute`` only after
    ``plan.status == \"ready\"`` and ``check_permissions`` succeeds.

    Secrets: proposals must not carry credential keys; ``credential_vault``
    injects them only inside ``execute``.
    """

    executor: OsExecutor
    #: Logical capabilities currently held by the actor (cap:// tokens / globs).
    actor_caps: set[str] = field(default_factory=set)
    #: ``AgentPolicy.net == deny``
    net_deny: bool = False
    #: ``AgentPolicy.fs_write == false``
    fs_write: bool = True
    #: Optional tool allowlist (``AgentPolicy.tool_allowlist``).
    tool_allowlist: set[str] | None = None
    #: tool_name → required capability string (from ToolSpec / ToolDesc).
    tool_capabilities: dict[str, str | None] = field(default_factory=dict)
    #: When True, host path denylist applies to path/prefix args.
    enforce_host_path_denylist: bool = True
    #: Reject credential-shaped keys in proposal arguments (default on).
    enforce_credential_policy: bool = True
    #: Out-of-band secrets injected at execute time only.
    credential_vault: CredentialVault | None = None
    #: Provenance of the current proposal (set per call by the OS bridge).
    source_trust: SourceTrust = SourceTrust.TRUSTED
    #: Whether a human already confirmed this attempt.
    confirmation_given: bool = False
    #: Optional ToolSpec map for high-impact / confirmation detection.
    tools: dict[str, ToolSpec] = field(default_factory=dict)
    #: Extra high-impact tool names beyond ToolSpec flags.
    high_impact_names: set[str] = field(default_factory=set)

    def check_permissions(
        self, tool_name: str, arguments: Mapping[str, Any] | None,
    ) -> tuple[bool, str]:
        """Final OS authorization (policy + caps + trust + path denylist)."""
        if self.enforce_credential_policy:
            ok, reason = reject_credential_arguments(arguments)
            if not ok:
                return False, reason

        allowed, trust_reason = trust_allows_execution(
            self.source_trust,
            tool_name,
            tools=self.tools or None,
            confirmation_given=self.confirmation_given,
            high_impact_names=self.high_impact_names or None,
        )
        if not allowed:
            return False, trust_reason

        denial = self._policy_deny(tool_name)
        if denial is not None:
            return False, denial

        required = self.tool_capabilities.get(tool_name)
        if required and not self._cap_allows(required):
            return False, f"host capability missing: {required}"

        if self.enforce_host_path_denylist:
            path = self._storage_path_arg(tool_name, arguments)
            if path and self._is_disallowed_storage_path(path):
                return False, f"host path denylist blocked: {path}"

        return True, "host permissions ok"

    def execute(
        self, tool_name: str, arguments: Mapping[str, Any] | None,
    ) -> Any:
        """Run via the OS executor; inject vault secrets out-of-band."""
        merged = inject_credentials(tool_name, arguments, self.credential_vault)
        return self.executor.execute(tool_name, merged)

    def _policy_deny(self, tool: str) -> str | None:
        if self.net_deny and tool in NET_TOOLS:
            return (
                f"outil {tool} bloqué par la politique réseau de l'agent "
                "(net:deny)"
            )
        if not self.fs_write and tool == "fs.write":
            return (
                f"outil {tool} bloqué par la politique de l'agent "
                "(fs.write:deny)"
            )
        if self.tool_allowlist is not None and tool not in self.tool_allowlist:
            return (
                f"outil {tool} hors allowlist de l'agent "
                f"({len(self.tool_allowlist)} outil(s) autorisé(s))"
            )
        return None

    def _cap_allows(self, required: str) -> bool:
        if required in self.actor_caps:
            return True
        # Glob-style OS caps: ``fs.read:**`` covers ``fs.read:/documents/**``.
        for held in self.actor_caps:
            if held.endswith(":**") and required.startswith(held[:-2]):
                return True
            if held.endswith(":*") and required.startswith(held[:-1]):
                return True
            if required.endswith(":**") and held.startswith(required[:-2]):
                return True
        return False

    @staticmethod
    def _storage_path_arg(
        tool: str, arguments: Mapping[str, Any] | None,
    ) -> str | None:
        if not arguments:
            return None
        key = {
            "fs.read": "path",
            "fs.write": "path",
            "files.generate": "path",
            "fs.list": "prefix",
        }.get(tool)
        if key is None:
            return None
        value = arguments.get(key)
        if isinstance(value, str) and value.strip():
            return value
        return None

    @staticmethod
    def _is_disallowed_storage_path(path: str) -> bool:
        lower = path.strip().lower()
        return any(lower.startswith(p.lower()) for p in HOST_PATH_PREFIXES)
