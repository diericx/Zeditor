use std::sync::mpsc;

use uuid::Uuid;

use crate::app::{App, DecodedFrame};

/// Public mirror of `DecodedFrame` for use in integration tests.
pub struct TestFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub pts_secs: f64,
}

/// Wraps a `SyncSender<DecodedFrame>` so integration tests can inject frames.
pub struct TestFrameSender {
    tx: mpsc::SyncSender<DecodedFrame>,
}

impl TestFrameSender {
    pub fn send_frame(&self, frame: TestFrame) {
        let decoded = DecodedFrame {
            rgba: frame.rgba,
            width: frame.width,
            height: frame.height,
            pts_secs: frame.pts_secs,
        };
        self.tx.send(decoded).expect("test channel send failed");
    }
}

impl App {
    /// Create an App wired to a test channel (no decode thread spawned).
    /// Returns the App and a sender for injecting frames.
    pub fn new_with_test_channel() -> (Self, TestFrameSender) {
        let (frame_tx, frame_rx) = mpsc::sync_channel::<DecodedFrame>(4);
        let mut app = Self::default();
        app.decode_rx = Some(frame_rx);
        (app, TestFrameSender { tx: frame_tx })
    }

    /// Set the decode time offset (source PTS + offset = timeline time).
    pub fn set_decode_time_offset(&mut self, offset: f64) {
        self.decode_time_offset = offset;
    }

    /// Set the clip ID that the decode channel is currently associated with.
    pub fn set_decode_clip_id(&mut self, clip_id: Option<Uuid>) {
        self.decode_clip_id = clip_id;
    }

    /// Get the clip ID currently being decoded.
    pub fn decode_clip_id(&self) -> Option<Uuid> {
        self.decode_clip_id
    }
}
