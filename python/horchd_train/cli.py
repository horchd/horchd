"""End-to-end training CLI.

Usage:
    python -m horchd_train --name jarvis --target-phrase "hey jarvis"

Reads positives + negatives from the canonical training dir, runs
augmentation + openWakeWord training, exports the .onnx into the models
dir, then prints the final path.
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
import traceback
from dataclasses import dataclass
from pathlib import Path

from .emit import emit, log
from .paths import (
    models_dir,
    negative_features_path,
    training_dir,
    validation_features_path,
)


@dataclass
class TrainingConfig:
    name: str
    target_phrase: str
    augment_per_recording: int = 30
    steps: int = 5000
    use_negatives_dir_too: bool = True


def parse_args() -> TrainingConfig:
    p = argparse.ArgumentParser()
    p.add_argument("--name", required=True, help="wakeword id; output is <name>.onnx")
    p.add_argument("--target-phrase", required=True, help="purely informational, recorded in metadata")
    p.add_argument("--augment-per-recording", type=int, default=30)
    p.add_argument("--steps", type=int, default=5000, help="training steps for sequence 1; 2 and 3 use steps/10")
    p.add_argument("--no-negatives-dir", action="store_true",
                   help="ignore the user's negative WAV samples, use only the precomputed features")
    a = p.parse_args()
    return TrainingConfig(
        name=a.name,
        target_phrase=a.target_phrase,
        augment_per_recording=a.augment_per_recording,
        steps=a.steps,
        use_negatives_dir_too=not a.no_negatives_dir,
    )


def main() -> int:
    cfg = parse_args()
    emit({"stage": "start", "name": cfg.name, "phrase": cfg.target_phrase})

    try:
        out = train_and_export(cfg)
    except SystemExit:
        raise
    except KeyboardInterrupt:
        log("interrupted")
        return 130
    except Exception as e:  # noqa: BLE001
        traceback.print_exc()
        emit({"stage": "error", "error": repr(e)})
        return 1

    emit({"stage": "done", "model": str(out)})
    log(f"✓ wrote {out}")
    return 0


# ---------------------------------------------------------------------------


def train_and_export(cfg: TrainingConfig) -> Path:
    """Run augmentation + training + ONNX export. Returns the produced .onnx."""

    word_dir = training_dir() / cfg.name
    pos_dir = word_dir / "positive"
    neg_dir = word_dir / "negative"
    if not pos_dir.is_dir() or not any(pos_dir.glob("*.wav")):
        raise FileNotFoundError(f"no positive samples under {pos_dir}")

    neg_features = negative_features_path()
    if not neg_features.is_file():
        raise FileNotFoundError(
            f"precomputed openWakeWord negative features not found at {neg_features}. "
            "Run `horchd-fetch-negatives` first."
        )

    work_dir = word_dir / "_work"
    if work_dir.exists():
        shutil.rmtree(work_dir)
    work_dir.mkdir(parents=True)

    # Save the metadata up front so a failed run still leaves it on disk.
    meta = {
        "name": cfg.name,
        "target_phrase": cfg.target_phrase,
    }
    (word_dir / "meta.json").write_text(json.dumps(meta, indent=2))

    # 1) collect + augment positives -----------------------------------------
    emit({"stage": "augment-pos", "progress": 0.05})
    log(f"augmenting positives ({cfg.augment_per_recording}× each)")
    aug_pos_dir = work_dir / "augmented_positives"
    from .augment import augment_directory
    n_aug_pos = augment_directory(pos_dir, aug_pos_dir, per_file=cfg.augment_per_recording, seed=42)
    log(f"  → {n_aug_pos} augmented positive clips")

    # Pad/truncate every positive to a fixed 2 s window for feature extraction.
    from .features import (
        CLIP_SAMPLES,
        compute_positive_features,
        normalise_clip,
    )
    norm_pos_dir = work_dir / "positives"
    norm_pos_dir.mkdir(parents=True, exist_ok=True)
    pos_paths: list[Path] = []
    for src_dir in [pos_dir, aug_pos_dir]:
        for wav in sorted(src_dir.glob("*.wav")):
            dst = norm_pos_dir / f"{src_dir.name}__{wav.name}"
            normalise_clip(wav, dst, CLIP_SAMPLES)
            pos_paths.append(dst)
    log(f"  total positives after normalisation: {len(pos_paths)}")
    if len(pos_paths) < 32:
        raise ValueError(
            f"only {len(pos_paths)} positive clips after augmentation — "
            "record more takes or raise --augment-per-recording"
        )

    # 2) optionally fold user negatives into the training stream as feature additions
    aug_neg_dir = work_dir / "augmented_negatives"
    extra_neg_paths: list[Path] = []
    if cfg.use_negatives_dir_too and neg_dir.is_dir() and any(neg_dir.glob("*.wav")):
        emit({"stage": "augment-neg", "progress": 0.15})
        log("augmenting user negatives")
        n_aug_neg = augment_directory(neg_dir, aug_neg_dir, per_file=4, seed=7)
        log(f"  → {n_aug_neg} augmented negative clips")
        norm_neg_dir = work_dir / "negatives"
        norm_neg_dir.mkdir(parents=True, exist_ok=True)
        for src_dir in [neg_dir, aug_neg_dir]:
            for wav in sorted(src_dir.glob("*.wav")):
                dst = norm_neg_dir / f"{src_dir.name}__{wav.name}"
                normalise_clip(wav, dst, CLIP_SAMPLES)
                extra_neg_paths.append(dst)

    # 3) compute mel + embedding features for positives ----------------------
    emit({"stage": "features-pos", "progress": 0.25})
    log(f"computing features for {len(pos_paths)} positives")
    pos_feat_path = work_dir / "positives.npy"

    last_pct = -1

    def _progress(done: int, total: int) -> None:
        nonlocal last_pct
        pct = int(100 * done / total)
        if pct >= last_pct + 5:
            last_pct = pct
            log(f"  features: {done}/{total} ({pct}%)")
            emit({"stage": "features-pos", "progress": 0.25 + 0.10 * (done / total)})

    n_pos = compute_positive_features(pos_paths, pos_feat_path, progress_cb=_progress)
    log(f"  positive features: {n_pos} rows")

    extra_neg_feat_path = None
    if extra_neg_paths:
        emit({"stage": "features-neg", "progress": 0.36})
        log(f"computing features for {len(extra_neg_paths)} user negatives")
        extra_neg_feat_path = work_dir / "user_negatives.npy"
        compute_positive_features(extra_neg_paths, extra_neg_feat_path)

    # 4) train ----------------------------------------------------------------
    emit({"stage": "train", "progress": 0.45})
    onnx_path = _run_openwakeword_training(
        cfg=cfg,
        work_dir=work_dir,
        pos_feat_path=pos_feat_path,
        neg_feat_path=neg_features,
        extra_neg_feat_path=extra_neg_feat_path,
        val_feat_path=validation_features_path() if validation_features_path().is_file() else None,
    )

    # 5) move the export into the canonical models dir
    emit({"stage": "export", "progress": 0.95})
    models_dir().mkdir(parents=True, exist_ok=True)
    dest = models_dir() / f"{cfg.name}.onnx"
    if dest.exists():
        dest.unlink()
    sidecar = dest.with_suffix(".onnx.data")
    if sidecar.exists():
        sidecar.unlink()
    shutil.move(str(onnx_path), dest)
    side_src = onnx_path.with_suffix(".onnx.data")
    if side_src.exists():
        shutil.move(str(side_src), sidecar)

    return dest


def _run_openwakeword_training(
    *,
    cfg: TrainingConfig,
    work_dir: Path,
    pos_feat_path: Path,
    neg_feat_path: Path,
    extra_neg_feat_path: Path | None,
    val_feat_path: Path | None,
) -> Path:
    """Run the actual openWakeWord auto_train and return path to the
    fresh `.onnx` inside `work_dir`."""

    import logging
    import time

    import numpy as np
    import torch

    # Quiet tqdm — its in-place updates collide with our log lines when
    # the Rust subprocess pipes both streams.
    import tqdm as _tqdm
    _orig_init = _tqdm.tqdm.__init__

    def _quiet(self, *args, **kwargs):
        kwargs["disable"] = True
        _orig_init(self, *args, **kwargs)

    _tqdm.tqdm.__init__ = _quiet
    logging.basicConfig(level=logging.INFO, format="%(message)s", stream=sys.stdout, force=True)

    from openwakeword.data import mmap_batch_generator
    from openwakeword.train import Model

    pos = np.load(pos_feat_path, mmap_mode="r")
    neg = np.load(neg_feat_path, mmap_mode="r")
    val = np.load(val_feat_path, mmap_mode="r") if val_feat_path else None

    extra_neg = None
    if extra_neg_feat_path and extra_neg_feat_path.is_file():
        extra_neg = np.load(extra_neg_feat_path, mmap_mode="r")
        log(f"  user negatives shape: {extra_neg.shape}")

    log(
        f"  shapes — positives: {pos.shape}, negatives: {neg.shape}"
        + (f", validation: {val.shape}" if val is not None else "")
    )

    seq1, seq2, seq3 = cfg.steps, max(1, cfg.steps // 10), max(1, cfg.steps // 10)
    total_batches = seq1 + seq2 + seq3
    boundaries = {seq1: 2, seq1 + seq2: 3}

    def _to_torch(gen):
        for data, labels in gen:
            yield (
                torch.from_numpy(np.asarray(data)).float(),
                torch.from_numpy(np.asarray(labels).astype(np.int64)),
            )

    data_files = {"1": str(pos_feat_path), "0": str(neg_feat_path)}
    n_per_class = {"1": 256, "0": 768}
    if extra_neg is not None and extra_neg_feat_path is not None:
        # Mix user negatives in alongside the precomputed corpus so the
        # model gets pressure on the user's actual room/voice noise.
        data_files["2"] = str(extra_neg_feat_path)
        n_per_class["2"] = 64
        # openwakeword's labelling: anything not "1" is negative.
    raw_train = mmap_batch_generator(
        data_files=data_files,
        n_per_class=n_per_class,
        batch_size=sum(n_per_class.values()),
    )
    train_iter = _to_torch(raw_train)

    fp_val_loader = None
    if val is not None:
        from numpy.lib.stride_tricks import sliding_window_view

        WINDOW = 16
        windows = sliding_window_view(val, (WINDOW, val.shape[1]))[:, 0]
        val_arr = np.ascontiguousarray(windows, dtype=np.float32)
        val_lbls = np.zeros(val_arr.shape[0], dtype=np.float32)
        fp_val_loader = torch.utils.data.DataLoader(
            torch.utils.data.TensorDataset(
                torch.from_numpy(val_arr), torch.from_numpy(val_lbls),
            ),
            batch_size=8192,
        )

    n_neg_val = min(neg.shape[0], max(1024, pos.shape[0] * 4))
    neg_val_idx = np.random.default_rng(0).choice(neg.shape[0], size=n_neg_val, replace=False)
    neg_val_idx.sort()
    val_pos = np.asarray(pos, dtype=np.float32)
    val_neg = np.asarray(neg[neg_val_idx], dtype=np.float32)
    val_x = np.concatenate([val_pos, val_neg], axis=0)
    val_y = np.concatenate([
        np.ones(val_pos.shape[0], dtype=np.float32),
        np.zeros(val_neg.shape[0], dtype=np.float32),
    ])
    x_val_loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(torch.from_numpy(val_x), torch.from_numpy(val_y)),
        batch_size=val_x.shape[0],
    )

    log(
        f"  training: 3 sequences × ({seq1} + {seq2} + {seq3} = {total_batches} steps), "
        f"layer_dim=32, batch={sum(n_per_class.values())}"
    )
    model = Model(
        n_classes=1,
        input_shape=(16, 96),
        model_type="dnn",
        layer_dim=32,
        n_blocks=1,
        seconds_per_example=2.0,
    )

    def _wrap_with_logs(gen):
        t0 = time.time()
        last_log = -1
        last_emit = -1
        for step, batch in enumerate(gen):
            if step in boundaries:
                seq_n = boundaries[step]
                log(f"  ── sequence {seq_n}/3 starting at step {step}")
            if step - last_log >= 50:
                last_log = step
                hist = model.history
                pieces = [f"step {step}/{total_batches}"]
                for key in ("loss", "recall", "val_recall", "val_accuracy", "val_fp_per_hr"):
                    if hist.get(key):
                        pieces.append(f"{key}={float(hist[key][-1]):.3f}")
                if step > 0:
                    eta = (time.time() - t0) * (total_batches - step) / step
                    pieces.append(f"eta={int(eta)}s")
                log("  " + "  ".join(pieces))
            if step - last_emit >= 25:
                last_emit = step
                emit({"stage": "train", "progress": 0.45 + 0.45 * (step / total_batches)})
            yield batch

    model.auto_train(
        X_train=_wrap_with_logs(train_iter),
        X_val=x_val_loader,
        false_positive_val_data=fp_val_loader,
        steps=cfg.steps,
    )

    hist = model.history
    if hist.get("val_recall"):
        log(
            f"  best val_recall={max(map(float, hist['val_recall'])):.3f}  "
            f"best val_acc={max(map(float, hist['val_accuracy'])):.3f}  "
            f"best fp/h={min(map(float, hist['val_fp_per_hr'])):.2f}"
        )

    export_dir = work_dir / "export"
    export_dir.mkdir(parents=True, exist_ok=True)
    model.export_model(model.model, cfg.name, str(export_dir))

    onnx_files = list(export_dir.rglob(f"{cfg.name}.onnx"))
    if not onnx_files:
        raise RuntimeError(f"no .onnx produced under {export_dir}")
    return onnx_files[0]


if __name__ == "__main__":
    sys.exit(main())
