use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::time::Duration;

/// Wrapper around rodio for audio playback.
pub struct AudioPlayer {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Sink,
}

impl AudioPlayer {
    pub fn new() -> Option<Self> {
        let (stream, stream_handle) = OutputStream::try_default().ok()?;
        let sink = Sink::try_new(&stream_handle).ok()?;
        Some(Self {
            _stream: stream,
            _stream_handle: stream_handle,
            sink,
        })
    }

    /// Queue interleaved f32 PCM audio samples for playback.
    pub fn queue_audio(&self, samples: Vec<f32>, sample_rate: u32, channels: u16) {
        let source = rodio::buffer::SamplesBuffer::new(channels, sample_rate, samples);
        self.sink.append(source);
    }

    pub fn stop(&self) {
        self.sink.stop();
    }

    pub fn pause(&self) {
        self.sink.pause();
    }

    pub fn play(&self) {
        self.sink.play();
    }

    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }

    /// Get the approximate buffered duration remaining in the sink.
    pub fn buffered_duration(&self) -> Duration {
        // Rodio Sink doesn't expose this directly, so we can't get a precise value.
        // Return Duration::ZERO as a placeholder — the decode loop will use
        // its own prebuffer tracking instead.
        Duration::ZERO
    }
}
