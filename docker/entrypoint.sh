#!/bin/sh
# Start a transient session bus on a fixed Unix socket, then exec horchd.
#
# horchd's D-Bus surface (xyz.horchd.Daemon1) targets the session bus,
# not the system bus. Containers start with no D-Bus daemon at all, so
# we boot one inline. The socket path is fixed and exposed via
# DBUS_SESSION_BUS_ADDRESS in the Dockerfile so `docker exec` shells
# (where horchctl lives) inherit it and can talk to the daemon.

set -eu

mkdir -p /run/dbus
dbus-daemon --session \
    --address="$DBUS_SESSION_BUS_ADDRESS" \
    --fork \
    --nosyslog

exec /usr/local/bin/horchd "$@"
