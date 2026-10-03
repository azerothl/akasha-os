"""Host ASR → text for Path A evaluate_gate.

No audio encoder and no bundled STT weights. The host (OS STT, Whisper the
operator installed, or a paste) produces a transcript; the gate sees the same
JSON context/proposal as typed chat. Transcripts are untrusted evidence.
"""

from __future__ import annotations

import os
import shlex
import subprocess
from dataclasses import replace
from typing import Any, Mapping

from .signals import GateContext
from .trust import SourceTrust, parse_source_trust

ASR_COMMAND_ENV = "AOS_ASR_COMMAND"
TRANSCRIPT_MAX_CHARS = 8000


def extract_asr_transcript(data: Mapping[str, Any] | None) -> str:
    data = data or {}
    asr = data.get("asr")
    raw = data.get("asr_transcript")
    if raw is None and isinstance(asr, Mapping):
        raw = asr.get("transcript")
    if raw is None:
        ctx = data.get("context")
        if isinstance(ctx, Mapping):
            raw = ctx.get("asr_transcript")
    text = str(raw or "").strip()
    if len(text) > TRANSCRIPT_MAX_CHARS:
        text = text[:TRANSCRIPT_MAX_CHARS]
    return text


def typed_operator_text(data: Mapping[str, Any] | None) -> str:
    data = data or {}
    typed = str(data.get("typed_text") or "").strip()
    if typed:
        return typed
    ctx = data.get("context")
    if isinstance(ctx, Mapping):
        return str(ctx.get("typed_text") or "").strip()
    return ""


def overlay_asr_transcript(
    ctx: GateContext, data: Mapping[str, Any] | None
) -> GateContext:
    """Mark ASR text as untrusted (or mixed if the operator also typed)."""
    transcript = extract_asr_transcript(data)
    if not transcript:
        return ctx
    typed = typed_operator_text(data)
    asr_trust = SourceTrust.MIXED if typed else SourceTrust.UNTRUSTED
    if ctx.source_trust is SourceTrust.UNTRUSTED:
        asr_trust = SourceTrust.UNTRUSTED
    elif ctx.source_trust is SourceTrust.MIXED:
        asr_trust = SourceTrust.MIXED
    return replace(ctx, source_trust=asr_trust)


def transcribe_via_host_command(
    path: str,
    *,
    command: str | None = None,
    env: Mapping[str, str] | None = None,
    timeout_sec: float = 30.0,
) -> dict[str, Any]:
    """Run an opt-in host STT command. Never downloads or vendors a model."""
    environ = env if env is not None else os.environ
    cmd = (command if command is not None else environ.get(ASR_COMMAND_ENV) or "").strip()
    if not cmd:
        return {
            "ok": False,
            "reason": "asr_not_configured",
            "message": (
                "No bundled STT. Set AOS_ASR_COMMAND to an opt-in host transcriber "
                "(stdout = transcript), or paste OS STT text as asr_transcript."
            ),
            "source_trust": SourceTrust.UNTRUSTED.value,
        }
    if not path or not os.path.isfile(path):
        return {
            "ok": False,
            "reason": "missing_audio",
            "message": "audio path missing or not a file (host FS; do not commit media)",
            "source_trust": SourceTrust.UNTRUSTED.value,
        }
    argv = shlex.split(cmd) + [path]
    try:
        proc = subprocess.run(
            argv,
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout_sec,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {
            "ok": False,
            "reason": "asr_command_failed",
            "message": str(exc),
            "source_trust": SourceTrust.UNTRUSTED.value,
        }
    if proc.returncode != 0:
        err = (proc.stderr or "").strip()[:240]
        return {
            "ok": False,
            "reason": "asr_command_failed",
            "message": err or f"exit {proc.returncode}",
            "source_trust": SourceTrust.UNTRUSTED.value,
        }
    transcript = (proc.stdout or "").strip()
    if not transcript:
        return {
            "ok": False,
            "reason": "empty_transcript",
            "message": "ASR command produced no text",
            "source_trust": SourceTrust.UNTRUSTED.value,
        }
    if len(transcript) > TRANSCRIPT_MAX_CHARS:
        transcript = transcript[:TRANSCRIPT_MAX_CHARS]
    return {
        "ok": True,
        "transcript": transcript,
        "source_trust": SourceTrust.UNTRUSTED.value,
        "message": "host command transcript; treat as untrusted chat text",
    }


def parse_source_trust_with_asr(value: object | None) -> SourceTrust:
    text = str(value or "").strip().lower()
    if text in {"asr", "stt", "speech", "transcript", "whisper", "os_stt"}:
        return SourceTrust.UNTRUSTED
    return parse_source_trust(value)
