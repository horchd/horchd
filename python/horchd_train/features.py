"""openWakeWord feature extraction.

Wraps `openwakeword.utils.AudioFeatures` (mel + embedding) so we can
compute the (N, 16, 96) tensors openWakeWord's classifier head trains
on. Output is saved as a (N, 16, 96) float32 numpy file.
"""

from __future__ import annotations

from pathlib import Path
from typing import Callable

import numpy as np
import soundfile as sf

SAMPLE_RATE = 16_000
CLIP_SECONDS = 2.0
CLIP_SAMPLES = int(SAMPLE_RATE * CLIP_SECONDS)


def normalise_clip(src: Path, dst: Path, target_samples: int = CLIP_SAMPLES) -> None:
    audio, sr = sf.read(str(src), dtype="float32", always_2d=False)
    if audio.ndim == 2:
        audio = audio.mean(axis=1)
    if sr != SAMPLE_RATE:
        import librosa
        audio = librosa.resample(audio, orig_sr=sr, target_sr=SAMPLE_RATE)
    if len(audio) < target_samples:
        pad = target_samples - len(audio)
        audio = np.concatenate([np.zeros(pad, dtype=np.float32), audio])
    elif len(audio) > target_samples:
        audio = audio[-target_samples:]
    sf.write(str(dst), audio.astype(np.float32), SAMPLE_RATE, subtype="PCM_16")


def compute_positive_features(
    paths: list[Path],
    out_path: Path,
    *,
    progress_cb: Callable[[int, int], None] | None = None,
) -> int:
    """Compute (N, 16, 96) features for each clip and save as one .npy file."""
    from openwakeword.utils import AudioFeatures

    af = AudioFeatures()
    feats: list[np.ndarray] = []
    total = len(paths)
    for i, path in enumerate(paths, 1):
        audio, sr = sf.read(str(path), dtype="float32", always_2d=False)
        if audio.ndim == 2:
            audio = audio.mean(axis=1)
        if sr != SAMPLE_RATE:
            import librosa
            audio = librosa.resample(audio, orig_sr=sr, target_sr=SAMPLE_RATE)
        # AudioFeatures.embed_clips returns (B, N, 16, 96) – we only feed one clip.
        emb = af.embed_clips([audio.astype(np.float32)], batch_size=1)
        # emb is shape (1, n_windows, 16, 96); treat each window as one
        # training example.
        feats.append(emb[0])
        if progress_cb:
            progress_cb(i, total)

    if not feats:
        raise ValueError("no features produced — empty input set")
    stacked = np.concatenate(feats, axis=0)
    np.save(out_path, stacked)
    return int(stacked.shape[0])
