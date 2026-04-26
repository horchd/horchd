#!/usr/bin/env python3
"""Subscribe to horchd's Detected signal via dbus-next.

    pip install dbus-next
    python examples/subscriber.py

Each fire prints `<wake>  score=<f>  ts=<us>`.
"""

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
