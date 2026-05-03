# syntax=docker/dockerfile:1.7
#
# horchd container image — wyoming-server-only deployment.
#
# Use case: drop into a HA voice pipeline as a Wyoming wake-word engine.
# Audio arrives via TCP from HA satellites; no host audio device needed.
#
# Build:
#   docker build -t horchd:dev .
# Run:
#   docker run --rm -p 10400:10400 -v horchd-data:/data ghcr.io/newtthewolf/horchd:latest
#
# Per-arch builds use cargo's host toolchain — multi-arch via buildx
# happens in .forgejo/workflows/docker.yml using QEMU emulation.

# ---------- builder ----------
FROM rust:1.88-slim-trixie AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libasound2-dev \
        ca-certificates \
        cmake \
        build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

# Cache deps separately from source so iterating on .rs doesn't
# redownload the registry. Copy manifests first.
COPY Cargo.toml Cargo.lock ./
COPY crates/client/Cargo.toml crates/client/Cargo.toml
COPY crates/horchctl/Cargo.toml crates/horchctl/Cargo.toml
COPY crates/horchd/Cargo.toml crates/horchd/Cargo.toml
COPY crates/wyoming/Cargo.toml crates/wyoming/Cargo.toml
COPY crates/gui/src-tauri/Cargo.toml crates/gui/src-tauri/Cargo.toml

# Stub the binaries so cargo can resolve the workspace and build deps
# without needing the real source yet.
RUN mkdir -p crates/client/src crates/horchctl/src crates/horchd/src crates/horchd/benches \
        crates/wyoming/src crates/gui/src-tauri/src \
    && echo 'fn main() {}' > crates/horchctl/src/main.rs \
    && echo 'fn main() {}' > crates/horchd/src/main.rs \
    && echo '' > crates/horchd/src/lib.rs \
    && echo '' > crates/client/src/lib.rs \
    && echo '' > crates/wyoming/src/lib.rs \
    && echo 'fn main() {}' > crates/gui/src-tauri/src/main.rs \
    && echo '' > crates/gui/src-tauri/src/lib.rs \
    && echo 'fn main() {}' > crates/horchd/benches/detector.rs \
    && echo 'fn main() {}' > crates/horchd/benches/audio_callback.rs \
    && cargo fetch --locked

# Now copy the real source and build.
COPY crates/ crates/

# Touch the real sources to force rebuild past the stub fingerprints.
RUN find crates -name '*.rs' -exec touch {} +

RUN cargo build --release -p horchd --bin horchd \
    && cargo build --release -p horchctl --bin horchctl

# ---------- runtime ----------
FROM debian:trixie-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        libasound2 \
        ca-certificates \
        netcat-openbsd \
        dbus \
        tini \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /etc/horchd /usr/local/share/horchd /data/horchd/models \
    && dbus-uuidgen --ensure

# Bake the openWakeWord universal preprocessing models into the image.
# Apache-2.0; see /usr/local/share/horchd/ATTRIBUTION.md.
# Pulled from the v0.5.1 GitHub Release (the upstream package's own
# bootstrap downloads from the same URL pattern).
# (ADD downloads with mode 0644 by default — no --chmod needed and it
# would force BuildKit-only Dockerfile interpretation.)
ADD https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/melspectrogram.onnx \
    /usr/local/share/horchd/melspectrogram.onnx
ADD https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/embedding_model.onnx \
    /usr/local/share/horchd/embedding_model.onnx

COPY --from=builder /src/target/release/horchd /usr/local/bin/horchd
COPY --from=builder /src/target/release/horchctl /usr/local/bin/horchctl
COPY docker/config.toml /etc/horchd/config.toml
COPY docker/ATTRIBUTION.md /usr/local/share/horchd/ATTRIBUTION.md
COPY docker/entrypoint.sh /usr/local/bin/horchd-entrypoint
RUN chmod 0755 /usr/local/bin/horchd-entrypoint

ENV XDG_DATA_HOME=/data \
    RUST_LOG=info \
    DBUS_SESSION_BUS_ADDRESS=unix:path=/run/dbus/session.sock

EXPOSE 10400/tcp
VOLUME ["/data"]

# Wyoming `describe` round-trip is the simplest liveness probe: if the
# listener is up and the inference state can answer info queries, the
# daemon is healthy enough for HA's voice pipeline to talk to it.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD echo '{"type":"describe"}' | nc -q1 localhost 10400 | grep -q '"type":"info"' || exit 1

# tini reaps the dbus-daemon child cleanly on SIGTERM.
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/horchd-entrypoint"]
CMD ["-c", "/etc/horchd/config.toml"]
