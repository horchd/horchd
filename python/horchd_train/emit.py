"""Tiny stdout helpers consumed by the Rust subprocess wrapper."""

from __future__ import annotations

import json
import sys


def emit(payload: dict) -> None:
    """Emit a structured status line. The Rust side parses lines starting
    with the marker; everything else flows through as plain log text."""
    print(f"##HORCHD {json.dumps(payload, separators=(',', ':'))}", flush=True)


def log(msg: str) -> None:
    print(msg, flush=True)


def warn(msg: str) -> None:
    print(f"⚠ {msg}", file=sys.stderr, flush=True)
