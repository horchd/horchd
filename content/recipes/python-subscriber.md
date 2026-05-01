---
eleventyNavigation:
  key: python-subscriber
  title: Python
  parent: recipes
  order: 20
---

## With `dbus-next`

```bash
pip install dbus-next
```

```python
import asyncio
from dbus_next.aio import MessageBus


async def main() -> None:
    bus = await MessageBus().connect()
    intro = await bus.introspect("xyz.horchd.Daemon", "/xyz/horchd/Daemon")
    proxy = bus.get_proxy_object("xyz.horchd.Daemon", "/xyz/horchd/Daemon", intro)
    iface = proxy.get_interface("xyz.horchd.Daemon1")

    def on_detected(name: str, score: float, timestamp_us: int) -> None:
        print(f"{name}\tscore={score:.4f}\tts={timestamp_us}")

    iface.on_detected(on_detected)
    print("subscribed; waiting for fires (Ctrl-C to exit)")
    await asyncio.Event().wait()


if __name__ == "__main__":
    asyncio.run(main())
```

## With `jeepney` (synchronous)

```python
from jeepney import DBusAddress, MatchRule, message_bus, new_method_call
from jeepney.io.blocking import open_dbus_connection

addr = DBusAddress(
    "/xyz/horchd/Daemon",
    bus_name="xyz.horchd.Daemon",
    interface="xyz.horchd.Daemon1",
)
conn = open_dbus_connection(bus="SESSION")
rule = MatchRule(
    type="signal", interface="xyz.horchd.Daemon1", member="Detected"
)
conn.send_and_get_reply(message_bus.AddMatch(rule))
print("subscribed")

while True:
    msg = conn.receive()
    if msg.header.message_type.name != "signal":
        continue
    name, score, ts = msg.body
    print(f"{name}\tscore={score:.4f}\tts={ts}")
```

## Polling state via `horchctl status`

If you only want a periodic health pulse, shelling out to `horchctl status`
is the lowest-friction way:

```python
import json, subprocess

def horchd_status() -> dict:
    raw = subprocess.check_output(["horchctl", "status", "--json"])
    return json.loads(raw)
```
