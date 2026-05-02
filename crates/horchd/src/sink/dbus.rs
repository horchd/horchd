//! D-Bus sink: emits `xyz.horchd.Daemon1.Detected` and `ScoreSnapshot`
//! signals on the session bus.

use async_trait::async_trait;
use horchd_client::{Detection, DetectionSink, ScoreSnapshot};
use zbus::Connection;
use zbus::object_server::SignalEmitter;

use crate::service;

const DBUS_PATH: &str = "/xyz/horchd/Daemon";

pub struct DBusSink {
    conn: Connection,
}

impl DBusSink {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl DetectionSink for DBusSink {
    async fn emit_detection(&self, det: &Detection) {
        let emitter = match SignalEmitter::new(&self.conn, DBUS_PATH) {
            Ok(e) => e,
            Err(err) => {
                tracing::error!(?err, "creating SignalEmitter");
                return;
            }
        };
        if let Err(err) =
            service::Daemon::detected(&emitter, &det.name, det.score, det.timestamp_us).await
        {
            tracing::error!(?err, name = %det.name, "emitting Detected signal");
        }
    }

    async fn emit_snapshot(&self, snap: &ScoreSnapshot) {
        let emitter = match SignalEmitter::new(&self.conn, DBUS_PATH) {
            Ok(e) => e,
            Err(err) => {
                tracing::error!(?err, "creating SignalEmitter");
                return;
            }
        };
        if let Err(err) = service::Daemon::score_snapshot(&emitter, &snap.name, snap.score).await {
            tracing::warn!(?err, name = %snap.name, "emitting ScoreSnapshot signal");
        }
    }

    fn name(&self) -> &'static str {
        "dbus"
    }
}
