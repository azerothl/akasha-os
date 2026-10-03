"""Host ASR overlay: untrusted transcripts, no bundled STT weights."""

from __future__ import annotations

from aos_gate.asr import (
    extract_asr_transcript,
    overlay_asr_transcript,
    transcribe_via_host_command,
)
from aos_gate.signals import GateContext, context_from_mapping
from aos_gate.trust import SourceTrust


def test_asr_only_is_untrusted():
    ctx = overlay_asr_transcript(
        GateContext(),
        {"asr_transcript": "delete all notes"},
    )
    assert ctx.source_trust is SourceTrust.UNTRUSTED
    assert extract_asr_transcript({"asr": {"transcript": " hi "}}) == "hi"


def test_asr_plus_typed_is_mixed():
    ctx = overlay_asr_transcript(
        GateContext(),
        {"asr_transcript": "export pdf", "typed_text": "please do it"},
    )
    assert ctx.source_trust is SourceTrust.MIXED


def test_no_asr_leaves_trusted():
    ctx = overlay_asr_transcript(GateContext(), {})
    assert ctx.source_trust is SourceTrust.TRUSTED


def test_transcribe_not_configured(tmp_path):
    wav = tmp_path / "clip.wav"
    wav.write_bytes(b"RIFF")
    out = transcribe_via_host_command(str(wav), command="", env={})
    assert out["ok"] is False
    assert out["reason"] == "asr_not_configured"


def test_transcribe_echo(tmp_path):
    wav = tmp_path / "clip.wav"
    wav.write_bytes(b"RIFF")
    out = transcribe_via_host_command(str(wav), command="echo host-stt-ok")
    assert out["ok"] is True
    assert "host-stt-ok" in out["transcript"]
    assert out["source_trust"] == "untrusted"


def test_evaluate_uses_asr_overlay():
    from aos_gate.cli import cmd_evaluate
    from aos_gate.catalog import tools_from_tool_descs

    tools = tools_from_tool_descs(
        [
            {
                "name": "fs.read",
                "description": "read",
                "input_schema": {"type": "object"},
                "required_caps": [],
                "backend": "Native",
            }
        ]
    )
    # cmd_evaluate reads catalog from request
    result = cmd_evaluate(
        {
            "catalog": [
                {
                    "name": "fs.read",
                    "description": "read",
                    "input_schema": {"type": "object"},
                    "required_caps": [],
                    "backend": "Native",
                }
            ],
            "proposal": {"tool_name": "fs.read", "arguments": {"path": "/documents/a.md"}},
            "asr_transcript": "read my notes please",
        }
    )
    assert result["context"]["source_trust"] == "untrusted"
    assert tools  # catalog helper still used
    ctx = context_from_mapping(result["context"])
    assert ctx.source_trust is SourceTrust.UNTRUSTED
