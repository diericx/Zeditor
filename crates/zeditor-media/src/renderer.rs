use std::collections::HashMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};

use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;
use rsmpeg::swscale::SwsContext;

use zeditor_core::media::SourceLibrary;
use zeditor_core::timeline::{Timeline, TimelinePosition, TrackType};

use crate::audio_decoder::FfmpegAudioDecoder;
use crate::decoder::{FfmpegDecoder, VideoDecoder};
use crate::error::{MediaError, Result};

/// Configuration for timeline rendering.
pub struct RenderConfig {
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub crf: u32,
    pub preset: String,
}

impl RenderConfig {
    /// Default config: 1920x1080, 30fps, CRF 22, superfast preset.
    pub fn default_with_path(output_path: PathBuf) -> Self {
        Self {
            output_path,
            width: 1920,
            height: 1080,
            fps: 30.0,
            crf: 22,
            preset: "superfast".to_string(),
        }
    }
}

/// Derive render config from timeline content. Uses the first video clip's
/// source asset dimensions/fps, or falls back to 1920x1080@30fps.
pub fn derive_render_config(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    output_path: PathBuf,
) -> RenderConfig {
    let mut config = RenderConfig::default_with_path(output_path);

    // Find the first video clip and use its source asset dimensions
    for track in &timeline.tracks {
        if track.track_type == TrackType::Video {
            if let Some(clip) = track.clips.first() {
                if let Some(asset) = source_library.get(clip.asset_id) {
                    if asset.width > 0 && asset.height > 0 {
                        config.width = asset.width;
                        config.height = asset.height;
                    }
                    if asset.fps > 0.0 {
                        config.fps = asset.fps;
                    }
                    return config;
                }
            }
        }
    }
    config
}

/// Cached video decoder with last decoded PTS for seek optimization.
struct CachedVideoDecoder {
    decoder: FfmpegDecoder,
    last_pts: f64,
}

/// Cached audio decoder with last decoded PTS for seek optimization.
struct CachedAudioDecoder {
    decoder: FfmpegAudioDecoder,
    last_pts: f64,
}

/// Render the timeline to an output video file.
///
/// Walks the timeline frame-by-frame, decoding source clips for video and audio,
/// encoding to h264+AAC, and muxing into MKV.
pub fn render_timeline(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    config: &RenderConfig,
) -> Result<()> {
    let total_duration = timeline.duration();
    if total_duration.as_secs_f64() <= 0.0 {
        return Err(MediaError::EncoderError("Timeline is empty".into()));
    }

    let total_frames =
        (total_duration.as_secs_f64() * config.fps).ceil() as u64;

    // Ensure dimensions are even (required by x264)
    let width = (config.width & !1).max(2) as i32;
    let height = (config.height & !1).max(2) as i32;

    // --- Open output format context ---
    let output_path_str = config.output_path.to_string_lossy().to_string();
    let c_output_path = CString::new(output_path_str.clone())
        .map_err(|_| MediaError::EncoderError(format!("Invalid path: {output_path_str}")))?;
    let mut output_ctx = AVFormatContextOutput::create(&c_output_path)
        .map_err(|e| MediaError::EncoderError(format!("Failed to create output: {e}")))?;

    // Check global header flag before creating streams (avoids borrow conflicts)
    let needs_global_header =
        output_ctx.oformat().flags & ffi::AVFMT_GLOBALHEADER as i32 != 0;

    // --- Video encoder setup ---
    let video_codec = AVCodec::find_encoder_by_name(c"libx264")
        .ok_or_else(|| MediaError::EncoderError("libx264 encoder not found".into()))?;

    let mut video_enc_ctx = AVCodecContext::new(&video_codec);
    video_enc_ctx.set_width(width);
    video_enc_ctx.set_height(height);
    video_enc_ctx.set_pix_fmt(ffi::AV_PIX_FMT_YUV420P);
    let fps_num = (config.fps * 1000.0).round() as i32;
    let fps_den = 1000;
    video_enc_ctx.set_time_base(ffi::AVRational { num: fps_den, den: fps_num });
    video_enc_ctx.set_framerate(ffi::AVRational { num: fps_num, den: fps_den });

    if needs_global_header {
        unsafe {
            use rsmpeg::UnsafeDerefMut;
            video_enc_ctx.deref_mut().flags |= ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }
    }

    // Build options dictionary: preset and CRF
    let c_preset = CString::new(config.preset.as_str())
        .map_err(|_| MediaError::EncoderError("Invalid preset".into()))?;
    let crf_str = config.crf.to_string();
    let c_crf = CString::new(crf_str.as_str())
        .map_err(|_| MediaError::EncoderError("Invalid CRF".into()))?;
    let opts = rsmpeg::avutil::AVDictionary::new(c"preset", &c_preset, 0);
    // AVDictionary::new returns an owned dict. set() consumes and returns a new one.
    let opts = opts.set(c"crf", &c_crf, 0);

    video_enc_ctx
        .open(Some(opts))
        .map_err(|e| MediaError::EncoderError(format!("Failed to open video encoder: {e}")))?;

    // --- Audio encoder setup ---
    let audio_codec = AVCodec::find_encoder_by_name(c"aac")
        .ok_or_else(|| MediaError::EncoderError("AAC encoder not found".into()))?;

    let mut audio_enc_ctx = AVCodecContext::new(&audio_codec);
    audio_enc_ctx.set_sample_rate(48000);
    audio_enc_ctx.set_sample_fmt(ffi::AV_SAMPLE_FMT_FLTP);
    audio_enc_ctx.set_time_base(ffi::AVRational { num: 1, den: 48000 });

    let stereo_layout = AVChannelLayout::from_nb_channels(2);
    unsafe {
        ffi::av_channel_layout_copy(
            &mut (*audio_enc_ctx.as_mut_ptr()).ch_layout,
            stereo_layout.as_ptr(),
        );
    }

    if needs_global_header {
        unsafe {
            use rsmpeg::UnsafeDerefMut;
            audio_enc_ctx.deref_mut().flags |= ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }
    }

    audio_enc_ctx
        .open(None)
        .map_err(|e| MediaError::EncoderError(format!("Failed to open audio encoder: {e}")))?;

    let audio_frame_size = unsafe { (*audio_enc_ctx.as_ptr()).frame_size };

    // --- Create streams (each scoped to release borrow before next) ---
    let video_stream_index = 0i32;
    let audio_stream_index = 1i32;
    {
        let codecpar = video_enc_ctx.extract_codecpar();
        let mut stream = output_ctx.new_stream();
        stream.set_codecpar(codecpar);
    }
    {
        let codecpar = audio_enc_ctx.extract_codecpar();
        let mut stream = output_ctx.new_stream();
        stream.set_codecpar(codecpar);
    }

    // --- Write header ---
    output_ctx
        .write_header(&mut None)
        .map_err(|e| MediaError::EncoderError(format!("Failed to write header: {e}")))?;

    // Get stream time bases after write_header (they may have been adjusted)
    let video_stream_tb = output_ctx.streams()[video_stream_index as usize].time_base;
    let audio_stream_tb = output_ctx.streams()[audio_stream_index as usize].time_base;

    // --- Create SWS context for RGB24 → YUV420P ---
    let mut sws_ctx = SwsContext::get_context(
        width,
        height,
        ffi::AV_PIX_FMT_RGB24,
        width,
        height,
        ffi::AV_PIX_FMT_YUV420P,
        ffi::SWS_FAST_BILINEAR,
        None,
        None,
        None,
    )
    .ok_or_else(|| MediaError::EncoderError("Failed to create SWS context".into()))?;

    // --- Decoder caches ---
    let mut video_decoders: HashMap<PathBuf, CachedVideoDecoder> = HashMap::new();
    let mut audio_decoders: HashMap<PathBuf, CachedAudioDecoder> = HashMap::new();

    // Audio resampler for converting decoded f32 interleaved → FLTP for AAC
    // We'll handle this manually when creating audio frames.

    // --- Video encoding loop ---
    encode_video_frames(
        timeline,
        source_library,
        config,
        total_frames,
        width,
        height,
        &mut sws_ctx,
        &mut video_enc_ctx,
        &mut output_ctx,
        video_stream_index,
        video_stream_tb,
        &mut video_decoders,
    )?;

    // --- Audio encoding loop ---
    encode_audio_frames(
        timeline,
        source_library,
        total_duration.as_secs_f64(),
        audio_frame_size,
        &mut audio_enc_ctx,
        &mut output_ctx,
        audio_stream_index,
        audio_stream_tb,
        &mut audio_decoders,
    )?;

    // --- Flush video encoder ---
    flush_encoder(
        &mut video_enc_ctx,
        &mut output_ctx,
        video_stream_index,
        video_stream_tb,
    )?;

    // --- Flush audio encoder ---
    flush_encoder(
        &mut audio_enc_ctx,
        &mut output_ctx,
        audio_stream_index,
        audio_stream_tb,
    )?;

    // --- Write trailer ---
    output_ctx
        .write_trailer()
        .map_err(|e| MediaError::EncoderError(format!("Failed to write trailer: {e}")))?;

    Ok(())
}

fn encode_video_frames(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    config: &RenderConfig,
    total_frames: u64,
    width: i32,
    height: i32,
    sws_ctx: &mut SwsContext,
    video_enc_ctx: &mut AVCodecContext,
    output_ctx: &mut AVFormatContextOutput,
    stream_index: i32,
    stream_tb: ffi::AVRational,
    video_decoders: &mut HashMap<PathBuf, CachedVideoDecoder>,
) -> Result<()> {
    for frame_idx in 0..total_frames {
        let timeline_time = frame_idx as f64 / config.fps;
        let pos = TimelinePosition::from_secs_f64(timeline_time);

        // Find video clip at this timeline position
        let clip_info = find_video_clip_at(timeline, source_library, pos);

        let yuv_frame = if let Some((source_path, source_time)) = clip_info {
            decode_and_convert_video_frame(
                &source_path,
                source_time,
                width,
                height,
                &mut *sws_ctx,
                video_decoders,
            )?
        } else {
            // Black frame
            create_black_yuv_frame(width, height)?
        };

        // Set PTS and encode
        let mut frame = yuv_frame;
        frame.set_pts(frame_idx as i64);

        encode_frame(
            video_enc_ctx,
            output_ctx,
            Some(&frame),
            stream_index,
            stream_tb,
        )?;
    }
    Ok(())
}

fn encode_audio_frames(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    total_duration_secs: f64,
    frame_size: i32,
    audio_enc_ctx: &mut AVCodecContext,
    output_ctx: &mut AVFormatContextOutput,
    stream_index: i32,
    stream_tb: ffi::AVRational,
    audio_decoders: &mut HashMap<PathBuf, CachedAudioDecoder>,
) -> Result<()> {
    let sample_rate = 48000i32;
    let channels = 2i32;
    let samples_per_frame = if frame_size > 0 { frame_size } else { 1024 };

    let mut audio_pts: i64 = 0;
    let total_samples = (total_duration_secs * sample_rate as f64).ceil() as i64;

    while audio_pts < total_samples {
        let timeline_time = audio_pts as f64 / sample_rate as f64;
        let pos = TimelinePosition::from_secs_f64(timeline_time);

        // Find audio clip at this timeline position
        let clip_info = find_audio_clip_at(timeline, source_library, pos);

        let audio_frame = if let Some((source_path, source_time)) = clip_info {
            decode_and_create_audio_frame(
                &source_path,
                source_time,
                samples_per_frame,
                sample_rate,
                channels,
                audio_pts,
                audio_decoders,
            )?
        } else {
            // Silence frame
            create_silence_frame(samples_per_frame, sample_rate, channels, audio_pts)?
        };

        encode_frame(
            audio_enc_ctx,
            output_ctx,
            Some(&audio_frame),
            stream_index,
            stream_tb,
        )?;

        audio_pts += samples_per_frame as i64;
    }

    Ok(())
}

/// Find the video clip at a timeline position and return (source_path, source_time).
fn find_video_clip_at(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    pos: TimelinePosition,
) -> Option<(PathBuf, f64)> {
    for track in &timeline.tracks {
        if track.track_type == TrackType::Video {
            if let Some(clip) = track.clip_at(pos) {
                if let Some(asset) = source_library.get(clip.asset_id) {
                    let source_time = clip.source_range.start.as_secs_f64()
                        + (pos.as_secs_f64() - clip.timeline_range.start.as_secs_f64());
                    return Some((asset.path.clone(), source_time));
                }
            }
        }
    }
    None
}

/// Find the audio clip at a timeline position and return (source_path, source_time).
fn find_audio_clip_at(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    pos: TimelinePosition,
) -> Option<(PathBuf, f64)> {
    for track in &timeline.tracks {
        if track.track_type == TrackType::Audio {
            if let Some(clip) = track.clip_at(pos) {
                if let Some(asset) = source_library.get(clip.asset_id) {
                    let source_time = clip.source_range.start.as_secs_f64()
                        + (pos.as_secs_f64() - clip.timeline_range.start.as_secs_f64());
                    return Some((asset.path.clone(), source_time));
                }
            }
        }
    }
    None
}

/// Decode a video frame from source and convert to YUV420P at the target dimensions.
fn decode_and_convert_video_frame(
    source_path: &Path,
    source_time: f64,
    width: i32,
    height: i32,
    sws_ctx: &mut SwsContext,
    decoders: &mut HashMap<PathBuf, CachedVideoDecoder>,
) -> Result<AVFrame> {
    let path_key = source_path.to_path_buf();

    // Open or reuse decoder
    if !decoders.contains_key(&path_key) {
        let decoder = FfmpegDecoder::open(source_path)?;
        decoders.insert(
            path_key.clone(),
            CachedVideoDecoder {
                decoder,
                last_pts: -1.0,
            },
        );
    }

    let cached = decoders.get_mut(&path_key).unwrap();

    // Seek if needed
    let needs_seek = source_time < cached.last_pts
        || (source_time - cached.last_pts) > 2.0
        || cached.last_pts < 0.0;

    if needs_seek {
        cached.decoder.seek_to(source_time)?;
        cached.last_pts = -1.0;
    }

    // Decode frames until we get one at or past the target time
    loop {
        match cached.decoder.decode_next_frame()? {
            Some(frame) => {
                cached.last_pts = frame.pts_secs;
                // Accept the frame if it's close enough to the target time
                if frame.pts_secs >= source_time - 0.05 {
                    // Convert RGB24 → YUV420P
                    return rgb_to_yuv(
                        &frame.data,
                        frame.width as i32,
                        frame.height as i32,
                        width,
                        height,
                        sws_ctx,
                    );
                }
                // Otherwise skip (seeking landed before target)
            }
            None => {
                // EOF — return black frame
                return create_black_yuv_frame(width, height);
            }
        }
    }
}

/// Convert RGB24 source frame data to a YUV420P AVFrame at the target dimensions.
fn rgb_to_yuv(
    rgb_data: &[u8],
    src_width: i32,
    src_height: i32,
    dst_width: i32,
    dst_height: i32,
    sws_ctx: &mut SwsContext,
) -> Result<AVFrame> {
    // Create source AVFrame with RGB24 data
    let mut src_frame = AVFrame::new();
    src_frame.set_width(src_width);
    src_frame.set_height(src_height);
    src_frame.set_format(ffi::AV_PIX_FMT_RGB24);
    src_frame
        .alloc_buffer()
        .map_err(|e| MediaError::EncoderError(format!("alloc src frame: {e}")))?;

    // Copy RGB data into frame, handling stride differences
    let linesize = unsafe { (*src_frame.as_ptr()).linesize[0] } as usize;
    let row_bytes = (src_width * 3) as usize;
    unsafe {
        let dst_ptr = src_frame.data[0] as *mut u8;
        for y in 0..src_height as usize {
            std::ptr::copy_nonoverlapping(
                rgb_data.as_ptr().add(y * row_bytes),
                dst_ptr.add(y * linesize),
                row_bytes,
            );
        }
    }

    // If dimensions differ, we need a separate scaling SWS context
    if src_width != dst_width || src_height != dst_height {
        let mut scale_sws = SwsContext::get_context(
            src_width,
            src_height,
            ffi::AV_PIX_FMT_RGB24,
            dst_width,
            dst_height,
            ffi::AV_PIX_FMT_YUV420P,
            ffi::SWS_FAST_BILINEAR,
            None,
            None,
            None,
        )
        .ok_or_else(|| MediaError::EncoderError("Failed to create scale SWS context".into()))?;

        let mut dst_frame = AVFrame::new();
        dst_frame.set_width(dst_width);
        dst_frame.set_height(dst_height);
        dst_frame.set_format(ffi::AV_PIX_FMT_YUV420P);
        dst_frame
            .alloc_buffer()
            .map_err(|e| MediaError::EncoderError(format!("alloc dst frame: {e}")))?;

        scale_sws
            .scale_frame(&src_frame, 0, src_height, &mut dst_frame)
            .map_err(|e| MediaError::EncoderError(format!("scale_frame: {e}")))?;

        Ok(dst_frame)
    } else {
        // Same dimensions — use the provided SWS context
        let mut dst_frame = AVFrame::new();
        dst_frame.set_width(dst_width);
        dst_frame.set_height(dst_height);
        dst_frame.set_format(ffi::AV_PIX_FMT_YUV420P);
        dst_frame
            .alloc_buffer()
            .map_err(|e| MediaError::EncoderError(format!("alloc dst frame: {e}")))?;

        sws_ctx
            .scale_frame(&src_frame, 0, src_height, &mut dst_frame)
            .map_err(|e| MediaError::EncoderError(format!("scale_frame: {e}")))?;

        Ok(dst_frame)
    }
}

/// Create a black YUV420P frame.
fn create_black_yuv_frame(width: i32, height: i32) -> Result<AVFrame> {
    let mut frame = AVFrame::new();
    frame.set_width(width);
    frame.set_height(height);
    frame.set_format(ffi::AV_PIX_FMT_YUV420P);
    frame
        .alloc_buffer()
        .map_err(|e| MediaError::EncoderError(format!("alloc black frame: {e}")))?;

    unsafe {
        let y_linesize = (*frame.as_ptr()).linesize[0] as usize;
        let u_linesize = (*frame.as_ptr()).linesize[1] as usize;
        let v_linesize = (*frame.as_ptr()).linesize[2] as usize;

        // Y plane: 0 = black
        let y_ptr = frame.data[0] as *mut u8;
        for row in 0..height as usize {
            std::ptr::write_bytes(y_ptr.add(row * y_linesize), 0, width as usize);
        }

        // U and V planes: 128 = neutral chroma
        let half_h = (height / 2) as usize;
        let half_w = (width / 2) as usize;
        let u_ptr = frame.data[1] as *mut u8;
        let v_ptr = frame.data[2] as *mut u8;
        for row in 0..half_h {
            std::ptr::write_bytes(u_ptr.add(row * u_linesize), 128, half_w);
            std::ptr::write_bytes(v_ptr.add(row * v_linesize), 128, half_w);
        }
    }

    Ok(frame)
}

/// Decode audio from source and create a FLTP AVFrame suitable for AAC encoding.
fn decode_and_create_audio_frame(
    source_path: &Path,
    source_time: f64,
    nb_samples: i32,
    sample_rate: i32,
    channels: i32,
    pts: i64,
    decoders: &mut HashMap<PathBuf, CachedAudioDecoder>,
) -> Result<AVFrame> {
    let path_key = source_path.to_path_buf();

    // Open or reuse decoder
    if !decoders.contains_key(&path_key) {
        let decoder = FfmpegAudioDecoder::open(source_path)?;
        decoders.insert(
            path_key.clone(),
            CachedAudioDecoder {
                decoder,
                last_pts: -1.0,
            },
        );
    }

    let cached = decoders.get_mut(&path_key).unwrap();

    // Seek if needed
    let needs_seek = source_time < cached.last_pts
        || (source_time - cached.last_pts) > 2.0
        || cached.last_pts < 0.0;

    if needs_seek {
        cached.decoder.seek_to(source_time)?;
        cached.last_pts = -1.0;
    }

    // Decode until we get audio near the target time
    loop {
        match cached.decoder.decode_next_audio_frame() {
            Ok(Some(frame)) => {
                cached.last_pts = frame.pts_secs;
                if frame.pts_secs >= source_time - 0.05 {
                    // Convert interleaved f32 samples to FLTP AVFrame
                    return interleaved_f32_to_fltp_frame(
                        &frame.samples,
                        frame.channels as i32,
                        nb_samples,
                        sample_rate,
                        channels,
                        pts,
                    );
                }
            }
            Ok(None) | Err(_) => {
                // EOF or error — return silence
                return create_silence_frame(nb_samples, sample_rate, channels, pts);
            }
        }
    }
}

/// Convert interleaved f32 PCM samples to a planar float (FLTP) AVFrame.
fn interleaved_f32_to_fltp_frame(
    samples: &[f32],
    src_channels: i32,
    nb_samples: i32,
    sample_rate: i32,
    dst_channels: i32,
    pts: i64,
) -> Result<AVFrame> {
    let mut frame = AVFrame::new();
    frame.set_format(ffi::AV_SAMPLE_FMT_FLTP);
    frame.set_sample_rate(sample_rate);
    frame.set_nb_samples(nb_samples);

    let ch_layout = AVChannelLayout::from_nb_channels(dst_channels);
    unsafe {
        ffi::av_channel_layout_copy(
            &mut (*frame.as_mut_ptr()).ch_layout,
            ch_layout.as_ptr(),
        );
    }

    frame
        .alloc_buffer()
        .map_err(|e| MediaError::EncoderError(format!("alloc audio frame: {e}")))?;

    frame.set_pts(pts);

    // Fill planar channels from interleaved source
    let actual_samples = nb_samples.min(samples.len() as i32 / src_channels.max(1));
    for ch in 0..dst_channels {
        let src_ch = if ch < src_channels { ch } else { 0 }; // duplicate mono to stereo
        unsafe {
            let plane_ptr = frame.data[ch as usize] as *mut f32;
            for s in 0..nb_samples {
                let val = if s < actual_samples {
                    let idx = s as usize * src_channels as usize + src_ch as usize;
                    if idx < samples.len() { samples[idx] } else { 0.0 }
                } else {
                    0.0
                };
                *plane_ptr.add(s as usize) = val;
            }
        }
    }

    Ok(frame)
}

/// Create a silent FLTP audio frame.
fn create_silence_frame(
    nb_samples: i32,
    sample_rate: i32,
    channels: i32,
    pts: i64,
) -> Result<AVFrame> {
    let mut frame = AVFrame::new();
    frame.set_format(ffi::AV_SAMPLE_FMT_FLTP);
    frame.set_sample_rate(sample_rate);
    frame.set_nb_samples(nb_samples);

    let ch_layout = AVChannelLayout::from_nb_channels(channels);
    unsafe {
        ffi::av_channel_layout_copy(
            &mut (*frame.as_mut_ptr()).ch_layout,
            ch_layout.as_ptr(),
        );
    }

    frame
        .alloc_buffer()
        .map_err(|e| MediaError::EncoderError(format!("alloc silence frame: {e}")))?;

    frame.set_pts(pts);

    // Zero all planes
    for ch in 0..channels {
        unsafe {
            let plane_ptr = frame.data[ch as usize] as *mut f32;
            std::ptr::write_bytes(plane_ptr, 0, nb_samples as usize);
        }
    }

    Ok(frame)
}

/// Encode a single frame and write any resulting packets to the output.
fn encode_frame(
    enc_ctx: &mut AVCodecContext,
    output_ctx: &mut AVFormatContextOutput,
    frame: Option<&AVFrame>,
    stream_index: i32,
    stream_tb: ffi::AVRational,
) -> Result<()> {
    enc_ctx.send_frame(frame).map_err(|e| {
        MediaError::EncoderError(format!("send_frame: {e}"))
    })?;

    loop {
        match enc_ctx.receive_packet() {
            Ok(mut packet) => {
                packet.rescale_ts(enc_ctx.time_base, stream_tb);
                packet.set_stream_index(stream_index);
                output_ctx
                    .interleaved_write_frame(&mut packet)
                    .map_err(|e| {
                        MediaError::EncoderError(format!("write_frame: {e}"))
                    })?;
            }
            Err(_) => break, // EAGAIN or EOF
        }
    }

    Ok(())
}

/// Flush an encoder (send None frame) and write remaining packets.
fn flush_encoder(
    enc_ctx: &mut AVCodecContext,
    output_ctx: &mut AVFormatContextOutput,
    stream_index: i32,
    stream_tb: ffi::AVRational,
) -> Result<()> {
    encode_frame(enc_ctx, output_ctx, None, stream_index, stream_tb)
}
