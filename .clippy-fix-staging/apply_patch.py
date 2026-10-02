#!/usr/bin/env python3
"""Surgical clippy fix: needless_borrows_for_generic_args on bbox and_then."""
from pathlib import Path

path = Path("crates/aos-proto/src/lib.rs")
text = path.read_text(encoding="utf-8")
old = "let head_x = head.and_then(&bbox).map(|b| (b.x0 + b.x1) * 0.5);"
new = "let head_x = head.and_then(bbox).map(|b| (b.x0 + b.x1) * 0.5);"
if new in text and old not in text:
    print("already fixed")
elif old not in text:
    raise SystemExit("target line missing in aos-proto lib.rs")
else:
    text = text.replace(old, new, 1)
    if "head.and_then(&bbox)" in text:
        raise SystemExit("still has and_then(&bbox) after replace")
    if "head.and_then(bbox)" not in text:
        raise SystemExit("fix line missing after replace")
    path.write_text(text, encoding="utf-8")
    print("patched", path, path.stat().st_size)
