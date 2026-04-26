"""Audiomentations wrapper.

Reads every WAV in `src_dir`, applies a randomised augmentation chain
`per_file` times, writes the results into `dst_dir`. Mirrors Lyna's
augmentation profile (gain, pitch, time-stretch, room reverb, noise) so
trained models see the same input distribution.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import soundfile as sf
from audiomentations import (
    AddGaussianSNR,
    Compose,
    Gain,
    PitchShift,
    RoomSimulator,
    TimeStretch,
)

SAMPLE_RATE = 16_000


def _build(seed: int) -> Compose:
    rng = np.random.default_rng(seed)
    return Compose([
        Gain(min_gain_db=-6, max_gain_db=6, p=0.7),
        PitchShift(min_semitones=-2, max_semitones=2, p=0.5),
        TimeStretch(min_rate=0.92, max_rate=1.08, p=0.4, leave_length_unchanged=False),
        AddGaussianSNR(min_snr_db=10, max_snr_db=40, p=0.6),
        RoomSimulator(
            min_size_x=2.0, max_size_x=8.0,
            min_size_y=2.0, max_size_y=8.0,
            min_size_z=2.2, max_size_z=4.0,
            p=0.3,
        ),
    ])


def _load_mono(path: Path) -> tuple[np.ndarray, int]:
    audio, sr = sf.read(str(path), dtype="float32", always_2d=False)
    if audio.ndim == 2:
        audio = audio.mean(axis=1)
    if sr != SAMPLE_RATE:
        import librosa
        audio = librosa.resample(audio, orig_sr=sr, target_sr=SAMPLE_RATE)
        sr = SAMPLE_RATE
    return audio.astype(np.float32), sr


def augment_directory(src_dir: Path, dst_dir: Path, *, per_file: int, seed: int) -> int:
    src_dir = Path(src_dir)
    dst_dir = Path(dst_dir)
    dst_dir.mkdir(parents=True, exist_ok=True)

    chain = _build(seed)
    out = 0
    for src in sorted(src_dir.glob("*.wav")):
        audio, sr = _load_mono(src)
        for i in range(per_file):
            out_audio = chain(samples=audio, sample_rate=sr)
            stem = src.stem
            dst = dst_dir / f"{stem}__aug{i:03d}.wav"
            sf.write(str(dst), out_audio, sr, subtype="PCM_16")
            out += 1
    return out
