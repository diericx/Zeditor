use std::path::Path;
use std::process::Command;

use crate::error::{MediaError, Result};

/// Export a video using ffmpeg CLI subprocess (crash isolation).
pub struct FfmpegExporter {
    input_path: String,
    output_path: String,
    start_secs: Option<f64>,
    duration_secs: Option<f64>,
}

impl FfmpegExporter {
    pub fn new(input: &Path, output: &Path) -> Self {
        Self {
            input_path: input.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            start_secs: None,
            duration_secs: None,
        }
    }

    pub fn start(mut self, secs: f64) -> Self {
        self.start_secs = Some(secs);
        self
    }

    pub fn duration(mut self, secs: f64) -> Self {
        self.duration_secs = Some(secs);
        self
    }

    pub fn run(&self) -> Result<()> {
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y");

        if let Some(ss) = self.start_secs {
            cmd.args(["-ss", &ss.to_string()]);
        }

        cmd.args(["-i", &self.input_path]);

        if let Some(d) = self.duration_secs {
            cmd.args(["-t", &d.to_string()]);
        }

        cmd.arg(&self.output_path);

        let output = cmd
            .output()
            .map_err(|e| MediaError::EncoderError(format!("failed to spawn ffmpeg: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MediaError::EncoderError(format!(
                "ffmpeg exited with {}: {}",
                output.status, stderr
            )));
        }

        Ok(())
    }
}
