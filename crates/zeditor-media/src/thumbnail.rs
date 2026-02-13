use std::path::Path;

use crate::decoder::{FfmpegDecoder, VideoDecoder, VideoFrame};
use crate::error::Result;

/// Generate a thumbnail by decoding the first frame of the video.
pub fn generate_thumbnail(path: &Path) -> Result<VideoFrame> {
    let mut decoder = FfmpegDecoder::open(path)?;
    decoder
        .decode_next_frame()?
        .ok_or_else(|| crate::error::MediaError::DecoderError("no frames in video".into()))
}

/// Generate a thumbnail at a specific timestamp.
pub fn generate_thumbnail_at(path: &Path, timestamp_secs: f64) -> Result<VideoFrame> {
    let mut decoder = FfmpegDecoder::open(path)?;
    decoder.seek_to(timestamp_secs)?;
    decoder
        .decode_next_frame()?
        .ok_or_else(|| crate::error::MediaError::DecoderError("no frame at timestamp".into()))
}
