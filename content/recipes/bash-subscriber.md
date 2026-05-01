---
eleventyNavigation:
  key: bash-subscriber
  title: Bash
  parent: recipes
  order: 10
description: "Subscribe to the horchd D-Bus Detected signal from a Bash one-liner using gdbus monitor and react to wakewords from shell scripts."
---

## One-liner monitor

```bash
busctl --user monitor xyz.horchd.Daemon
```

Prints every D-Bus message touching the daemon. Useful for debugging.

## React to fires with `gdbus`

```bash
gdbus monitor --session \
  --dest xyz.horchd.Daemon \
  --object-path /xyz/horchd/Daemon \
  | while read -r line; do
    case "$line" in
      *"xyz.horchd.Daemon1.Detected"*)
        name=$(echo "$line" | sed -nE "s/.*\\('([^']+)',.*/\\1/p")
        notify-send "horchd" "wake: ${name:-?}"
        ;;
    esac
  done
```

Adapt the `notify-send` line to whatever side-effect you want (mpv, curl,
systemd-run, home-assistant CLI, etc.).

The same script ships in the daemon repo under `examples/subscriber.sh`.

## Read the score too

```bash
gdbus monitor --session \
  --dest xyz.horchd.Daemon \
  --object-path /xyz/horchd/Daemon \
  | sed -nE "s/.*xyz\\.horchd\\.Daemon1\\.Detected \\(\\('([^']+)', ([^,]+), ([0-9]+)\\),\\).*/\\1\\t\\2\\t\\3/p" \
  | while read -r name score ts; do
      printf 'wake=%s score=%s ts=%s\n' "$name" "$score" "$ts"
    done
```
