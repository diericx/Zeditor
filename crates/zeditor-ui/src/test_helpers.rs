use std::sync::mpsc;

use uuid::Uuid;

use crate::app::{App, DecodedAudio, DecodedFrame};

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

/// Public mirror of `DecodedAudio` for use in integration tests.
pub struct TestAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub pts_secs: f64,
}

/// Wraps a `SyncSender<DecodedAudio>` so integration tests can inject audio frames.
pub struct TestAudioSender {
    tx: mpsc::SyncSender<DecodedAudio>,
}

impl TestAudioSender {
    pub fn send_audio(&self, audio: TestAudio) {
        let decoded = DecodedAudio {
            samples: audio.samples,
            sample_rate: audio.sample_rate,
            channels: audio.channels,
            pts_secs: audio.pts_secs,
        };
        self.tx.send(decoded).expect("test audio channel send failed");
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

    /// Create an App wired to both video and audio test channels.
    /// Returns the App, a video frame sender, and an audio sender.
    pub fn new_with_test_channels() -> (Self, TestFrameSender, TestAudioSender) {
        let (frame_tx, frame_rx) = mpsc::sync_channel::<DecodedFrame>(4);
        let (audio_tx, audio_rx) = mpsc::sync_channel::<DecodedAudio>(4);
        let mut app = Self::default();
        app.decode_rx = Some(frame_rx);
        app.audio_decode_rx = Some(audio_rx);
        (app, TestFrameSender { tx: frame_tx }, TestAudioSender { tx: audio_tx })
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

    /// Get the audio clip ID currently being decoded.
    pub fn audio_decode_clip_id(&self) -> Option<Uuid> {
        self.audio_decode_clip_id
    }

    /// Set the audio decode clip ID.
    pub fn set_audio_decode_clip_id(&mut self, clip_id: Option<Uuid>) {
        self.audio_decode_clip_id = clip_id;
    }

    /// Set the audio decode time offset.
    pub fn set_audio_decode_time_offset(&mut self, offset: f64) {
        self.audio_decode_time_offset = offset;
    }
}
