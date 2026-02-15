use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Progress update sent over a channel during rendering.
#[derive(Debug, Clone)]
pub struct RenderProgress {
    pub current_frame: u64,
    pub total_frames: u64,
    pub elapsed: Duration,
    pub stage: RenderStage,
}

/// Current stage of the render pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderStage {
    Setup,
    VideoEncoding,
    AudioEncoding,
    Flushing,
    Complete,
}

/// Snapshot of render configuration for the profile report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub crf: u32,
    pub preset: String,
}

/// High-level stage timings in milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageTimings {
    pub setup_ms: f64,
    pub video_encode_ms: f64,
    pub audio_encode_ms: f64,
    pub flush_ms: f64,
    pub write_trailer_ms: f64,
}

/// Per-frame timing breakdown in milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMetrics {
    pub frame_index: u64,
    pub timeline_time_secs: f64,
    pub total_ms: f64,
    pub find_clips_ms: f64,
    pub decode_ms: f64,
    pub effects_ms: f64,
    pub composite_ms: f64,
    pub color_convert_ms: f64,
    pub encode_ms: f64,
    pub clip_count: usize,
    pub used_effects_path: bool,
}

/// Full profiling report, serialized to JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderProfile {
    pub config: ProfileConfig,
    pub stages: StageTimings,
    pub frames: Vec<FrameMetrics>,
    pub total_frames: u64,
    pub total_duration_secs: f64,
    pub avg_frame_ms: f64,
    pub median_frame_ms: f64,
    pub p95_frame_ms: f64,
    pub max_frame_ms: f64,
    pub slowest_frame_index: u64,
}

/// Accumulates profiling data during a render. All recording is gated
/// behind `is_enabled()` so there is zero overhead when profiling is off.
pub struct ProfileCollector {
    enabled: bool,
    pub stages: StageTimings,
    frames: Vec<FrameMetrics>,
    config: Option<ProfileConfig>,
    render_start: Option<Instant>,
}

impl ProfileCollector {
    /// Create a new collector. If `enabled` is false, all recording is a no-op.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            stages: StageTimings::default(),
            frames: Vec::new(),
            config: None,
            render_start: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_config(&mut self, config: ProfileConfig) {
        if self.enabled {
            self.config = Some(config);
        }
    }

    pub fn set_render_start(&mut self, instant: Instant) {
        if self.enabled {
            self.render_start = Some(instant);
        }
    }

    pub fn record_frame(&mut self, metrics: FrameMetrics) {
        if self.enabled {
            self.frames.push(metrics);
        }
    }

    /// Finalize and return the profile report. Returns `None` if profiling
    /// is disabled or no config was set.
    pub fn finish(&self) -> Option<RenderProfile> {
        if !self.enabled {
            return None;
        }
        let config = self.config.clone()?;

        let total_duration_secs = self
            .render_start
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.0);

        let total_frames = self.frames.len() as u64;

        let (avg, median, p95, max_ms, slowest_idx) = if self.frames.is_empty() {
            (0.0, 0.0, 0.0, 0.0, 0)
        } else {
            compute_stats(&self.frames)
        };

        Some(RenderProfile {
            config,
            stages: self.stages.clone(),
            frames: self.frames.clone(),
            total_frames,
            total_duration_secs,
            avg_frame_ms: avg,
            median_frame_ms: median,
            p95_frame_ms: p95,
            max_frame_ms: max_ms,
            slowest_frame_index: slowest_idx,
        })
    }
}

/// Compute avg, median, p95, max, and slowest frame index from frame metrics.
fn compute_stats(frames: &[FrameMetrics]) -> (f64, f64, f64, f64, u64) {
    let mut times: Vec<f64> = frames.iter().map(|f| f.total_ms).collect();
    let n = times.len();
    let avg = times.iter().sum::<f64>() / n as f64;

    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = times[n / 2];
    let p95_idx = ((n as f64 * 0.95).ceil() as usize).min(n - 1);
    let p95 = times[p95_idx];
    let max_ms = times[n - 1];

    let slowest_idx = frames
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            a.total_ms
                .partial_cmp(&b.total_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(_, f)| f.frame_index)
        .unwrap_or(0);

    (avg, median, p95, max_ms, slowest_idx)
}

/// Check if profiling is enabled via the `ZEDITOR_PROFILE` env var.
/// Accepts `1` or `true` (case-insensitive).
pub fn is_profiling_enabled() -> bool {
    std::env::var("ZEDITOR_PROFILE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Determine the output path for the profile JSON.
///
/// If `ZEDITOR_PROFILE_DIR` is set, the profile is written into that directory
/// with the render filename + `.profile.json`. Otherwise it is placed next to
/// the render output as `<render_output>.profile.json`.
pub fn profile_output_path(render_path: &Path) -> PathBuf {
    if let Ok(dir) = std::env::var("ZEDITOR_PROFILE_DIR") {
        let dir = PathBuf::from(dir);
        let filename = render_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        dir.join(format!("{filename}.profile.json"))
    } else {
        let mut p = render_path.as_os_str().to_owned();
        p.push(".profile.json");
        PathBuf::from(p)
    }
}

/// Serialize a profile to pretty JSON and write it to `path`.
pub fn write_profile(profile: &RenderProfile, path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(profile)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, json)
}
