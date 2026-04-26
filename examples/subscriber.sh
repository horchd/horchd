#!/usr/bin/env bash
# Tail horchd's `Detected` D-Bus signal and run a side-effect per fire.
# Usage:
#   ./subscriber.sh
# Replace the `notify-send` call with whatever you want to react with
# (mpv, curl, home-assistant CLI, etc.).
set -euo pipefail

gdbus monitor --session \
  --dest xyz.horchd.Daemon \
  --object-path /xyz/horchd/Daemon \
  | while read -r line; do
    case "$line" in
      *"xyz.horchd.Daemon1.Detected"*)
        # gdbus prints e.g.:
        #   /xyz/horchd/Daemon: xyz.horchd.Daemon1.Detected ('hey_jarvis', 0.83, 12345)
        name=$(echo "$line" | sed -nE "s/.*\\('([^']+)',.*/\\1/p")
        notify-send "horchd" "wake: ${name:-?}"
        ;;
    esac
  done
