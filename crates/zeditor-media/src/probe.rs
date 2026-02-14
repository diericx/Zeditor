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
    let mut rotation = 0u32;

    for stream in streams.iter() {
        let codecpar = stream.codecpar();
        if codecpar.codec_type == rsmpeg::ffi::AVMEDIA_TYPE_VIDEO {
            width = codecpar.width as u32;
            height = codecpar.height as u32;
            let r = stream.r_frame_rate;
            if r.den > 0 {
                fps = r.num as f64 / r.den as f64;
            }
            // Extract rotation from stream side data
            rotation = extract_rotation_from_side_data(&stream);
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

    let mut asset = MediaAsset::new(
        name,
        path.to_path_buf(),
        duration,
        width,
        height,
        fps,
        has_audio,
    );
    asset.rotation = rotation;
    Ok(asset)
}

/// Extract rotation from stream display matrix side data.
pub fn extract_rotation_from_side_data(stream: &rsmpeg::avformat::AVStreamRef<'_>) -> u32 {
    // Try to get the display matrix from stream side data
    unsafe {
        let codecpar = stream.codecpar();
        let side_data = rsmpeg::ffi::av_packet_side_data_get(
            (*codecpar.as_ptr()).coded_side_data,
            (*codecpar.as_ptr()).nb_coded_side_data as i32,
            rsmpeg::ffi::AV_PKT_DATA_DISPLAYMATRIX,
        );
        if !side_data.is_null() {
            let matrix = (*side_data).data as *const i32;
            let angle = -rsmpeg::ffi::av_display_rotation_get(matrix);
            return normalize_rotation(angle as i32);
        }
    }
    0
}

/// Normalize a rotation angle to one of 0, 90, 180, 270.
fn normalize_rotation(degrees: i32) -> u32 {
    let normalized = ((degrees % 360) + 360) % 360;
    // Round to nearest 90
    match normalized {
        0..=44 | 316..=360 => 0,
        45..=134 => 90,
        135..=224 => 180,
        225..=315 => 270,
        _ => 0,
    }
}
