"""Canonical filesystem layout shared with the Rust side."""

from __future__ import annotations

import os
from pathlib import Path


def data_dir() -> Path:
    base = os.environ.get("XDG_DATA_HOME")
    if base:
        return Path(base) / "horchd"
    return Path.home() / ".local" / "share" / "horchd"


def training_dir() -> Path:
    return data_dir() / "training"


def models_dir() -> Path:
    return data_dir() / "models"


def negatives_dir() -> Path:
    return data_dir() / "negatives"


def negative_features_path() -> Path:
    return negatives_dir() / "openwakeword_features_ACAV100M_2000_hrs_16bit.npy"


def validation_features_path() -> Path:
    return negatives_dir() / "validation_set_features.npy"
