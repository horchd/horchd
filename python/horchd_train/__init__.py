"""horchd-gui's training subprocess.

Spawned by the Tauri `train_wakeword` command. Reads positive + negative
WAV samples from `~/.local/share/horchd/training/<name>/`, runs the
openWakeWord training pipeline against the precomputed negatives feature
file, and writes `~/.local/share/horchd/models/<name>.onnx`.

Status updates are emitted on stdout as JSON-prefixed lines
(`##HORCHD {"stage": ..., "progress": ...}`) so the Rust side can
forward them to the UI as events.
"""
