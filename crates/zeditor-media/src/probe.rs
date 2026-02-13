use std::ffi::CString;
use std::path::Path;
use std::time::Duration;

use zeditor_core::media::MediaAsset;

use crate::error::{MediaError, Result};

/// Probe a media file and return a MediaAsset with metadata.
pub fn probe(path: &Path) -> Result<MediaAsset> {
    let path_str = path.to_string_lossy().to_string();
    let c_path =
        CString::new(path_str.clone()).map_err(|_| MediaError::ProbeError(path_str.clone()))?;

    let input_ctx = rsmpeg::avformat::AVFormatContextInput::open(&c_path)
        .map_err(|e| MediaError::ProbeError(format!("{path_str}: {e}")))?;

    let streams = input_ctx.streams();
    let mut width = 0u32;
    let mut height = 0u32;
    let mut fps = 30.0f64;
    let mut has_audio = false;

    for stream in streams.iter() {
        let codecpar = stream.codecpar();
        if codecpar.codec_type == rsmpeg::ffi::AVMEDIA_TYPE_VIDEO {
            width = codecpar.width as u32;
            height = codecpar.height as u32;
            let r = stream.r_frame_rate;
            if r.den > 0 {
                fps = r.num as f64 / r.den as f64;
            }
        } else if codecpar.codec_type == rsmpeg::ffi::AVMEDIA_TYPE_AUDIO {
            has_audio = true;
        }
    }

    let duration_secs = input_ctx.duration as f64 / rsmpeg::ffi::AV_TIME_BASE as f64;
    let duration = Duration::from_secs_f64(duration_secs.max(0.0));

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    Ok(MediaAsset::new(
        name,
        path.to_path_buf(),
        duration,
        width,
        height,
        fps,
        has_audio,
    ))
}
