"""One-shot downloader for the openWakeWord precomputed negative
features bundle. Pulled from HuggingFace so users don't have to assemble
the 30 GB raw corpus themselves."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import requests

from .emit import emit, log
from .paths import (
    negative_features_path,
    negatives_dir,
    validation_features_path,
)

NEG_URL = (
    "https://huggingface.co/datasets/davidscripka/openwakeword_features/"
    "resolve/main/openwakeword_features_ACAV100M_2000_hrs_16bit.npy"
)
VAL_URL = (
    "https://huggingface.co/datasets/davidscripka/openwakeword_features/"
    "resolve/main/validation_set_features.npy"
)


def _download(url: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists() and dest.stat().st_size > 0:
        log(f"  ✓ already present: {dest.name} ({dest.stat().st_size / 1e6:.1f} MB)")
        return
    log(f"  ↓ {url}")
    tmp = dest.with_suffix(dest.suffix + ".part")
    with requests.get(url, stream=True, timeout=120) as r:
        r.raise_for_status()
        total = int(r.headers.get("Content-Length") or 0)
        done = 0
        last_pct = -1
        with tmp.open("wb") as f:
            for chunk in r.iter_content(chunk_size=4 * 1024 * 1024):
                if not chunk:
                    continue
                f.write(chunk)
                done += len(chunk)
                if total:
                    pct = int(100 * done / total)
                    if pct >= last_pct + 5:
                        last_pct = pct
                        emit({"stage": "fetch", "file": dest.name, "progress": done / total})
                        log(f"    {pct}%  ({done / 1e6:.1f} / {total / 1e6:.1f} MB)")
    tmp.rename(dest)
    log(f"  ✓ {dest.name} ({dest.stat().st_size / 1e6:.1f} MB)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--validation",
        action="store_true",
        help="also fetch the optional validation features file (~2 GB)",
    )
    args = parser.parse_args()

    log(f"target dir: {negatives_dir()}")
    try:
        _download(NEG_URL, negative_features_path())
        if args.validation:
            _download(VAL_URL, validation_features_path())
    except Exception as e:  # noqa: BLE001
        emit({"stage": "error", "error": repr(e)})
        log(f"✗ download failed: {e}")
        return 1
    emit({"stage": "done"})
    return 0


if __name__ == "__main__":
    sys.exit(main())
