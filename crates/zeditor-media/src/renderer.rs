use std::collections::HashMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};

use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::{AVFormatContextInput, AVFormatContextOutput};
use rsmpeg::avutil::{AVChannelLayout, AVFrame};
use rsmpeg::ffi;
use rsmpeg::swresample::SwrContext;
use rsmpeg::swscale::SwsContext;

use zeditor_core::effects::{self, EffectInstance, ResolvedTransform};
use zeditor_core::media::SourceLibrary;
use zeditor_core::project::ProjectSettings;
use zeditor_core::timeline::{Timeline, TimelinePosition, TrackType};

use crate::decoder::{FfmpegDecoder, VideoDecoder};
use crate::error::{MediaError, Result};

/// Scaling algorithm for video frame resizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalingAlgorithm {
    FastBilinear,
    Bilinear,
    Bicubic,
    Lanczos,
}

impl ScalingAlgorithm {
    pub fn to_sws_flags(self) -> ffi::SwsFlags {
        match self {
            Self::FastBilinear => ffi::SWS_FAST_BILINEAR,
            Self::Bilinear => ffi::SWS_BILINEAR,
            Self::Bicubic => ffi::SWS_BICUBIC,
            Self::Lanczos => ffi::SWS_LANCZOS,
        }
    }
}

/// Configuration for timeline rendering.
///
/// Future fields: video_codec, audio_codec, container_format,
/// audio_sample_rate, audio_channels, pixel_format.
pub struct RenderConfig {
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub fps: f64,
    pub crf: u32,
    pub preset: String,
    pub scaling: ScalingAlgorithm,
}

impl RenderConfig {
    /// Default config: 1920x1080 canvas & render, 30fps, CRF 22, superfast preset, Lanczos scaling.
    pub fn default_with_path(output_path: PathBuf) -> Self {
        Self {
            output_path,
            width: 1920,
            height: 1080,
            canvas_width: 1920,
            canvas_height: 1080,
            fps: 30.0,
            crf: 22,
            preset: "superfast".to_string(),
            scaling: ScalingAlgorithm::Lanczos,
        }
    }
}

/// Derive render config from timeline content and project settings.
/// Uses the project canvas dimensions for both canvas and render output,
/// and derives FPS from the first video clip's source (to avoid temporal artifacts).
pub fn derive_render_config(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    settings: &ProjectSettings,
    output_path: PathBuf,
) -> RenderConfig {
    let mut config = RenderConfig::default_with_path(output_path);
    config.canvas_width = settings.canvas_width;
    config.canvas_height = settings.canvas_height;
    config.width = settings.canvas_width;
    config.height = settings.canvas_height;
    config.fps = settings.fps;

    // Override FPS from first source clip if available (to avoid temporal artifacts)
    for track in &timeline.tracks {
        if track.track_type == TrackType::Video {
            if let Some(clip) = track.clips.first() {
                if let Some(asset) = source_library.get(clip.asset_id) {
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

/// Cached video decoder with per-source SWS context for direct pixel format conversion.
struct CachedVideoDecoder {
    decoder: FfmpegDecoder,
    last_pts: f64,
    /// SWS context for converting from source pixel format to YUV420P at target dimensions.
    /// Created lazily on first decoded frame (needs source format/dimensions).
    sws_ctx: Option<SwsContext>,
    /// Rotation in degrees (0, 90, 180, 270) from stream metadata.
    rotation: u32,
}

const OUTPUT_SAMPLE_RATE: i32 = 48000;
const OUTPUT_CHANNELS: i32 = 2;

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
    let opts = opts.set(c"crf", &c_crf, 0);

    video_enc_ctx
        .open(Some(opts))
        .map_err(|e| MediaError::EncoderError(format!("Failed to open video encoder: {e}")))?;

    // --- Audio encoder setup ---
    let audio_codec = AVCodec::find_encoder_by_name(c"aac")
        .ok_or_else(|| MediaError::EncoderError("AAC encoder not found".into()))?;

    let mut audio_enc_ctx = AVCodecContext::new(&audio_codec);
    audio_enc_ctx.set_sample_rate(OUTPUT_SAMPLE_RATE);
    audio_enc_ctx.set_sample_fmt(ffi::AV_SAMPLE_FMT_FLTP);
    audio_enc_ctx.set_time_base(ffi::AVRational { num: 1, den: OUTPUT_SAMPLE_RATE });

    let stereo_layout = AVChannelLayout::from_nb_channels(OUTPUT_CHANNELS);
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

    // --- Decoder cache for video ---
    let mut video_decoders: HashMap<PathBuf, CachedVideoDecoder> = HashMap::new();

    // --- Video encoding loop ---
    encode_video_frames(
        timeline,
        source_library,
        config,
        total_frames,
        width,
        height,
        &mut video_enc_ctx,
        &mut output_ctx,
        video_stream_index,
        video_stream_tb,
        &mut video_decoders,
    )?;

    // --- Audio encoding: pre-render all clips into buffer, then encode ---
    encode_audio_offline(
        timeline,
        source_library,
        total_duration.as_secs_f64(),
        audio_frame_size,
        &mut audio_enc_ctx,
        &mut output_ctx,
        audio_stream_index,
        audio_stream_tb,
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

// =============================================================================
// Video encoding — uses raw AVFrames with per-source SWS contexts
// =============================================================================

fn encode_video_frames(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    config: &RenderConfig,
    total_frames: u64,
    width: i32,
    height: i32,
    video_enc_ctx: &mut AVCodecContext,
    output_ctx: &mut AVFormatContextOutput,
    stream_index: i32,
    stream_tb: ffi::AVRational,
    video_decoders: &mut HashMap<PathBuf, CachedVideoDecoder>,
) -> Result<()> {
    let canvas_w = config.canvas_width;
    let canvas_h = config.canvas_height;

    for frame_idx in 0..total_frames {
        let timeline_time = frame_idx as f64 / config.fps;
        let pos = TimelinePosition::from_secs_f64(timeline_time);

        let clip_info = find_video_clip_at(timeline, source_library, pos);

        let yuv_frame = if let Some((source_path, source_time, clip_effects)) = clip_info {
            let transform = effects::resolve_transform(&clip_effects);
            decode_and_convert_video_frame(
                &source_path,
                source_time,
                width,
                height,
                canvas_w,
                canvas_h,
                video_decoders,
                config.scaling.to_sws_flags(),
                &transform,
            )?
        } else {
            create_black_yuv_frame(width, height)?
        };

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

/// Decode a raw video frame from source and compose it onto a canvas at render dimensions.
///
/// The source is scaled to fit within the project canvas (preserving aspect ratio),
/// centered, then the canvas is mapped onto the render output.
fn decode_and_convert_video_frame(
    source_path: &Path,
    source_time: f64,
    render_w: i32,
    render_h: i32,
    canvas_w: u32,
    canvas_h: u32,
    decoders: &mut HashMap<PathBuf, CachedVideoDecoder>,
    sws_flags: ffi::SwsFlags,
    transform: &ResolvedTransform,
) -> Result<AVFrame> {
    let path_key = source_path.to_path_buf();

    // Open or reuse decoder
    if !decoders.contains_key(&path_key) {
        let decoder = FfmpegDecoder::open(source_path)?;
        let rotation = decoder.stream_info().rotation;
        decoders.insert(
            path_key.clone(),
            CachedVideoDecoder {
                decoder,
                last_pts: -1.0,
                sws_ctx: None,
                rotation,
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
        // Invalidate SWS context in case source format changed after seek
        cached.sws_ctx = None;
    }

    // Decode raw frames until we get one at or past the target time
    loop {
        match cached.decoder.decode_next_raw_frame()? {
            Some((raw_frame, pts_secs)) => {
                cached.last_pts = pts_secs;
                if pts_secs >= source_time - 0.05 {
                    let src_w = raw_frame.width;
                    let src_h = raw_frame.height;
                    let src_fmt = raw_frame.format;
                    let rotation = cached.rotation;

                    // Use display dimensions (after rotation) for canvas layout
                    let (display_w, display_h) = if rotation == 90 || rotation == 270 {
                        (src_h as u32, src_w as u32)
                    } else {
                        (src_w as u32, src_h as u32)
                    };

                    // Compute canvas layout for this source using display dimensions
                    let mut layout = compute_canvas_layout(
                        display_w,
                        display_h,
                        canvas_w,
                        canvas_h,
                        render_w as u32,
                        render_h as u32,
                    );

                    // Apply transform offset (in canvas pixels, scaled to render pixels)
                    let canvas_scale = (render_w as f64 / canvas_w.max(1) as f64)
                        .min(render_h as f64 / canvas_h.max(1) as f64);
                    layout.clip_x += (transform.x_offset * canvas_scale) as i32 & !1;
                    layout.clip_y += (transform.y_offset * canvas_scale) as i32 & !1;

                    // For rotated video, we need to scale to pre-rotation
                    // clip dimensions (swapped), then rotate after scaling.
                    let (scale_target_w, scale_target_h) = if rotation == 90 || rotation == 270 {
                        // Scale to swapped clip dimensions (pre-rotation)
                        (layout.clip_h, layout.clip_w)
                    } else {
                        (layout.clip_w, layout.clip_h)
                    };

                    // Create/reuse SWS context: source → scale target dimensions
                    if cached.sws_ctx.is_none() {
                        cached.sws_ctx = Some(
                            SwsContext::get_context(
                                src_w,
                                src_h,
                                src_fmt,
                                scale_target_w,
                                scale_target_h,
                                ffi::AV_PIX_FMT_YUV420P,
                                sws_flags,
                                None,
                                None,
                                None,
                            )
                            .ok_or_else(|| {
                                MediaError::EncoderError(
                                    "Failed to create SWS context for source".into(),
                                )
                            })?,
                        );
                    }

                    let sws = cached.sws_ctx.as_mut().unwrap();

                    return compose_clip_onto_canvas_rotated(
                        &raw_frame,
                        &layout,
                        render_w,
                        render_h,
                        sws,
                        src_h,
                        scale_target_w,
                        scale_target_h,
                        rotation,
                    );
                }
                // Otherwise skip (seeking landed before target)
            }
            None => {
                return create_black_yuv_frame(render_w, render_h);
            }
        }
    }
}

// =============================================================================
// Audio encoding — offline clip-at-a-time rendering with sequential decode
// =============================================================================

/// Pre-render all audio clips into a contiguous sample buffer, then encode into AAC frames.
/// This avoids per-frame seeking which caused choppy audio.
fn encode_audio_offline(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    total_duration_secs: f64,
    frame_size: i32,
    audio_enc_ctx: &mut AVCodecContext,
    output_ctx: &mut AVFormatContextOutput,
    stream_index: i32,
    stream_tb: ffi::AVRational,
) -> Result<()> {
    let samples_per_frame = if frame_size > 0 { frame_size } else { 1024 };
    let total_samples =
        (total_duration_secs * OUTPUT_SAMPLE_RATE as f64).ceil() as usize;

    // Pre-allocate output buffer: interleaved f32 at 48kHz stereo, initialized to silence
    let mut output_buffer = vec![0.0f32; total_samples * OUTPUT_CHANNELS as usize];

    // Process each audio clip: decode sequentially and write into the buffer
    for track in &timeline.tracks {
        if track.track_type != TrackType::Audio {
            continue;
        }
        for clip in &track.clips {
            if let Some(asset) = source_library.get(clip.asset_id) {
                decode_audio_clip_into_buffer(
                    &asset.path,
                    clip.source_range.start.as_secs_f64(),
                    clip.timeline_range.start.as_secs_f64(),
                    clip.timeline_range.end.as_secs_f64(),
                    &mut output_buffer,
                )?;
            }
        }
    }

    // Encode the buffer into AAC frames
    let mut audio_pts: i64 = 0;
    let mut offset = 0usize;
    let frame_sample_count = samples_per_frame as usize * OUTPUT_CHANNELS as usize;

    while offset < output_buffer.len() {
        let remaining = output_buffer.len() - offset;
        let chunk_size = remaining.min(frame_sample_count);
        let chunk = &output_buffer[offset..offset + chunk_size];
        let actual_nb_samples = (chunk_size / OUTPUT_CHANNELS as usize) as i32;

        let frame = interleaved_f32_to_fltp_frame(
            chunk,
            OUTPUT_CHANNELS,
            actual_nb_samples,
            OUTPUT_SAMPLE_RATE,
            OUTPUT_CHANNELS,
            audio_pts,
        )?;

        encode_frame(
            audio_enc_ctx,
            output_ctx,
            Some(&frame),
            stream_index,
            stream_tb,
        )?;

        audio_pts += actual_nb_samples as i64;
        offset += chunk_size;
    }

    Ok(())
}

/// Decode audio from a single clip and write resampled samples into the output buffer.
/// Uses a SwrContext to convert from source format to 48kHz stereo interleaved f32.
fn decode_audio_clip_into_buffer(
    source_path: &Path,
    source_start_secs: f64,
    timeline_start_secs: f64,
    timeline_end_secs: f64,
    output_buffer: &mut [f32],
) -> Result<()> {
    let path_str = source_path.to_string_lossy().to_string();
    let c_path = CString::new(path_str.clone())
        .map_err(|_| MediaError::OpenFailed(path_str.clone()))?;

    let mut input_ctx = AVFormatContextInput::open(&c_path)
        .map_err(|e| MediaError::OpenFailed(format!("{path_str}: {e}")))?;

    // Find audio stream
    let (audio_stream_index, decoder) = {
        let streams = input_ctx.streams();
        let mut found = None;
        for (i, stream) in streams.iter().enumerate() {
            let codecpar = stream.codecpar();
            if codecpar.codec_type == ffi::AVMEDIA_TYPE_AUDIO {
                let codec_id = codecpar.codec_id;
                if let Some(dec) = AVCodec::find_decoder(codec_id) {
                    found = Some((i, dec));
                    break;
                }
            }
        }
        match found {
            Some(f) => f,
            None => return Ok(()), // No audio stream — skip silently
        }
    };

    let mut decode_ctx = AVCodecContext::new(&decoder);
    {
        let streams = input_ctx.streams();
        let audio_stream = &streams[audio_stream_index];
        decode_ctx
            .apply_codecpar(&audio_stream.codecpar())
            .map_err(|e| MediaError::DecoderError(format!("apply_codecpar: {e}")))?;
    }

    unsafe {
        use rsmpeg::UnsafeDerefMut;
        decode_ctx.deref_mut().thread_count = 0;
    }

    decode_ctx
        .open(None)
        .map_err(|e| MediaError::DecoderError(format!("open: {e}")))?;

    // Set up SwrContext: source format → 48kHz stereo interleaved f32
    let in_sample_rate = decode_ctx.sample_rate;
    let in_sample_fmt = decode_ctx.sample_fmt;
    let in_ch_layout = unsafe {
        rsmpeg::avutil::AVChannelLayoutRef::new(&decode_ctx.ch_layout)
    };

    let out_ch_layout = AVChannelLayout::from_nb_channels(OUTPUT_CHANNELS);

    let mut swr_ctx = SwrContext::new(
        &out_ch_layout,
        ffi::AV_SAMPLE_FMT_FLT, // interleaved f32
        OUTPUT_SAMPLE_RATE,
        &in_ch_layout,
        in_sample_fmt,
        in_sample_rate,
    )
    .map_err(|e| MediaError::DecoderError(format!("swr_alloc: {e}")))?;

    swr_ctx
        .init()
        .map_err(|e| MediaError::DecoderError(format!("swr_init: {e}")))?;

    // Seek to source start
    {
        let streams = input_ctx.streams();
        let audio_stream = &streams[audio_stream_index];
        let tb = audio_stream.time_base;
        let ts = (source_start_secs * tb.den as f64 / tb.num as f64) as i64;
        let _ = streams;

        // Seek may fail for short files or at start — that's OK
        let _ = input_ctx.seek(
            audio_stream_index as i32,
            ts,
            ffi::AVSEEK_FLAG_BACKWARD as i32,
        );
        decode_ctx.flush_buffers();
    }

    // Calculate output buffer positions
    let clip_start_sample =
        (timeline_start_secs * OUTPUT_SAMPLE_RATE as f64) as usize;
    let clip_end_sample =
        (timeline_end_secs * OUTPUT_SAMPLE_RATE as f64) as usize;
    let clip_duration_samples = clip_end_sample.saturating_sub(clip_start_sample);
    let max_output_floats = clip_duration_samples * OUTPUT_CHANNELS as usize;

    let mut samples_written = 0usize;
    let mut past_source_start = false;

    // Decode sequentially
    loop {
        if samples_written >= max_output_floats {
            break;
        }

        let packet = match input_ctx.read_packet() {
            Ok(Some(p)) => p,
            Ok(None) => {
                // EOF: flush decoder
                decode_ctx.send_packet(None).ok();
                loop {
                    match decode_ctx.receive_frame() {
                        Ok(frame) => {
                            let converted = convert_audio_frame(
                                &mut swr_ctx,
                                &frame,
                                &input_ctx,
                                audio_stream_index,
                            )?;
                            if let Some((samples, pts_secs)) = converted {
                                if pts_secs < source_start_secs - 0.05 {
                                    continue;
                                }
                                let written = write_samples_to_buffer(
                                    &samples,
                                    output_buffer,
                                    clip_start_sample,
                                    samples_written,
                                    max_output_floats,
                                );
                                samples_written += written;
                            }
                        }
                        Err(_) => break,
                    }
                }
                break;
            }
            Err(_) => break,
        };

        if packet.stream_index as usize != audio_stream_index {
            continue;
        }

        decode_ctx.send_packet(Some(&packet)).map_err(|e| {
            MediaError::DecoderError(format!("send_packet: {e}"))
        })?;

        loop {
            match decode_ctx.receive_frame() {
                Ok(frame) => {
                    let converted = convert_audio_frame(
                        &mut swr_ctx,
                        &frame,
                        &input_ctx,
                        audio_stream_index,
                    )?;
                    if let Some((samples, pts_secs)) = converted {
                        // Skip frames before source start
                        if !past_source_start {
                            if pts_secs < source_start_secs - 0.05 {
                                continue;
                            }
                            past_source_start = true;
                        }

                        let written = write_samples_to_buffer(
                            &samples,
                            output_buffer,
                            clip_start_sample,
                            samples_written,
                            max_output_floats,
                        );
                        samples_written += written;

                        if samples_written >= max_output_floats {
                            return Ok(());
                        }
                    }
                }
                Err(_) => break, // EAGAIN
            }
        }
    }

    Ok(())
}

/// Convert a decoded audio frame to interleaved f32 at 48kHz stereo via SwrContext.
/// Returns the samples and the PTS in seconds.
fn convert_audio_frame(
    swr_ctx: &mut SwrContext,
    frame: &AVFrame,
    input_ctx: &AVFormatContextInput,
    audio_stream_index: usize,
) -> Result<Option<(Vec<f32>, f64)>> {
    let nb_samples = frame.nb_samples;

    let mut dst_frame = AVFrame::new();
    dst_frame.set_format(ffi::AV_SAMPLE_FMT_FLT);
    dst_frame.set_sample_rate(OUTPUT_SAMPLE_RATE);

    let out_ch_layout = AVChannelLayout::from_nb_channels(OUTPUT_CHANNELS);
    unsafe {
        ffi::av_channel_layout_copy(
            &mut (*dst_frame.as_mut_ptr()).ch_layout,
            out_ch_layout.as_ptr(),
        );
    }
    // Estimate output samples for rate conversion
    let estimated_out = unsafe {
        ffi::swr_get_out_samples(swr_ctx.as_mut_ptr(), nb_samples)
    };
    dst_frame.set_nb_samples(estimated_out.max(nb_samples));
    dst_frame.alloc_buffer().map_err(|e| {
        MediaError::DecoderError(format!("alloc_buffer: {e}"))
    })?;

    swr_ctx
        .convert_frame(Some(frame), &mut dst_frame)
        .map_err(|e| MediaError::DecoderError(format!("convert_frame: {e}")))?;

    let actual_samples = dst_frame.nb_samples;
    let total_floats = actual_samples as usize * OUTPUT_CHANNELS as usize;
    let samples = unsafe {
        std::slice::from_raw_parts(
            dst_frame.data[0] as *const f32,
            total_floats,
        )
        .to_vec()
    };

    let pts_secs = {
        let streams = input_ctx.streams();
        let tb = streams[audio_stream_index].time_base;
        if frame.pts != ffi::AV_NOPTS_VALUE {
            frame.pts as f64 * tb.num as f64 / tb.den as f64
        } else {
            0.0
        }
    };

    Ok(Some((samples, pts_secs)))
}

/// Write decoded samples into the output buffer at the correct position.
/// Returns the number of f32 values written.
fn write_samples_to_buffer(
    samples: &[f32],
    output_buffer: &mut [f32],
    clip_start_sample: usize,
    samples_already_written: usize,
    max_output_floats: usize,
) -> usize {
    let buf_offset =
        clip_start_sample * OUTPUT_CHANNELS as usize + samples_already_written;
    let remaining = max_output_floats - samples_already_written;
    let to_write = samples.len().min(remaining);

    if buf_offset + to_write <= output_buffer.len() {
        output_buffer[buf_offset..buf_offset + to_write]
            .copy_from_slice(&samples[..to_write]);
    } else if buf_offset < output_buffer.len() {
        let avail = output_buffer.len() - buf_offset;
        let actual = to_write.min(avail);
        output_buffer[buf_offset..buf_offset + actual]
            .copy_from_slice(&samples[..actual]);
        return actual;
    }

    to_write
}

// =============================================================================
// Shared helpers
// =============================================================================

// =============================================================================
// Canvas composition — scale-to-fit + centered letterboxing
// =============================================================================

/// Layout describing where the clip lands on the render output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasLayout {
    /// Clip dimensions in render-space (pixels).
    pub clip_w: i32,
    pub clip_h: i32,
    /// Clip offset in render-space (pixels from top-left).
    pub clip_x: i32,
    pub clip_y: i32,
}

/// Compute where a source clip should be placed on the render output.
///
/// The pipeline is:
/// 1. Map the project canvas onto the render output (scale-to-fit, centered).
/// 2. Scale the source clip to fit within the project canvas (preserve aspect ratio).
/// 3. Map the clip position from canvas-space to render-space.
///
/// All dimensions and offsets are forced even (`& !1`) for YUV420P chroma alignment.
pub fn compute_canvas_layout(
    src_w: u32,
    src_h: u32,
    canvas_w: u32,
    canvas_h: u32,
    render_w: u32,
    render_h: u32,
) -> CanvasLayout {
    let src_w = src_w.max(1) as f64;
    let src_h = src_h.max(1) as f64;
    let canvas_w = canvas_w.max(1) as f64;
    let canvas_h = canvas_h.max(1) as f64;
    let render_w = render_w.max(1) as f64;
    let render_h = render_h.max(1) as f64;

    // Step 1: Map canvas onto render output (scale-to-fit)
    let canvas_scale = (render_w / canvas_w).min(render_h / canvas_h);
    let mapped_canvas_w = canvas_w * canvas_scale;
    let mapped_canvas_h = canvas_h * canvas_scale;
    let canvas_offset_x = (render_w - mapped_canvas_w) / 2.0;
    let canvas_offset_y = (render_h - mapped_canvas_h) / 2.0;

    // Step 2: Scale source to fit within canvas (preserve aspect ratio)
    let clip_scale = (canvas_w / src_w).min(canvas_h / src_h);
    let clip_in_canvas_w = src_w * clip_scale;
    let clip_in_canvas_h = src_h * clip_scale;
    let clip_in_canvas_x = (canvas_w - clip_in_canvas_w) / 2.0;
    let clip_in_canvas_y = (canvas_h - clip_in_canvas_h) / 2.0;

    // Step 3: Map from canvas-space to render-space
    let clip_render_w = (clip_in_canvas_w * canvas_scale) as i32 & !1;
    let clip_render_h = (clip_in_canvas_h * canvas_scale) as i32 & !1;
    let clip_render_x = (canvas_offset_x + clip_in_canvas_x * canvas_scale) as i32 & !1;
    let clip_render_y = (canvas_offset_y + clip_in_canvas_y * canvas_scale) as i32 & !1;

    CanvasLayout {
        clip_w: clip_render_w.max(2),
        clip_h: clip_render_h.max(2),
        clip_x: clip_render_x,
        clip_y: clip_render_y,
    }
}

/// Compose a decoded source frame onto a black canvas, with optional rotation.
///
/// 1. SWS-scale source to `(scale_w, scale_h)` in YUV420P (pre-rotation dimensions)
/// 2. If rotation != 0, rotate Y/U/V planes
/// 3. Create a black frame at `(render_w, render_h)`
/// 4. Blit the rotated clip onto the canvas at `(layout.clip_x, layout.clip_y)`
fn compose_clip_onto_canvas_rotated(
    raw_frame: &AVFrame,
    layout: &CanvasLayout,
    render_w: i32,
    render_h: i32,
    sws_ctx: &mut SwsContext,
    src_h: i32,
    scale_w: i32,
    scale_h: i32,
    rotation: u32,
) -> Result<AVFrame> {
    // Scale source to pre-rotation dimensions
    let mut scaled = AVFrame::new();
    scaled.set_width(scale_w);
    scaled.set_height(scale_h);
    scaled.set_format(ffi::AV_PIX_FMT_YUV420P);
    scaled.alloc_buffer().map_err(|e| {
        MediaError::EncoderError(format!("alloc scaled frame: {e}"))
    })?;

    sws_ctx
        .scale_frame(raw_frame, 0, src_h, &mut scaled)
        .map_err(|e| MediaError::EncoderError(format!("scale_frame: {e}")))?;

    // Apply rotation to the scaled frame
    let final_frame = if rotation != 0 {
        rotate_yuv420p_frame(&scaled, rotation)?
    } else {
        scaled
    };

    // Create black canvas at render dimensions
    let mut canvas = create_black_yuv_frame(render_w, render_h)?;

    // Blit rotated clip onto canvas
    blit_yuv_frame(&final_frame, &mut canvas, layout.clip_x, layout.clip_y);

    Ok(canvas)
}

/// Rotate a YUV420P frame by the given degrees (90, 180, 270).
fn rotate_yuv420p_frame(frame: &AVFrame, rotation: u32) -> Result<AVFrame> {
    let src_w = frame.width as usize;
    let src_h = frame.height as usize;

    let (dst_w, dst_h) = match rotation {
        90 | 270 => (src_h, src_w),
        180 => (src_w, src_h),
        _ => return Err(MediaError::EncoderError(format!("unsupported rotation: {rotation}"))),
    };

    let mut dst = AVFrame::new();
    dst.set_width(dst_w as i32);
    dst.set_height(dst_h as i32);
    dst.set_format(ffi::AV_PIX_FMT_YUV420P);
    dst.alloc_buffer().map_err(|e| {
        MediaError::EncoderError(format!("alloc rotated frame: {e}"))
    })?;

    unsafe {
        let src_ptr = (*frame.as_ptr()).data;
        let src_ls = (*frame.as_ptr()).linesize;
        let dst_ptr = (*dst.as_mut_ptr()).data;
        let dst_ls = (*dst.as_mut_ptr()).linesize;

        // Rotate Y plane (full resolution)
        rotate_plane(
            src_ptr[0] as *const u8, src_ls[0] as usize, src_w, src_h,
            dst_ptr[0] as *mut u8, dst_ls[0] as usize, dst_w, dst_h,
            rotation,
        );

        // Rotate U and V planes (half resolution)
        let half_src_w = src_w / 2;
        let half_src_h = src_h / 2;
        let half_dst_w = dst_w / 2;
        let half_dst_h = dst_h / 2;

        for plane in 1..=2usize {
            rotate_plane(
                src_ptr[plane] as *const u8, src_ls[plane] as usize, half_src_w, half_src_h,
                dst_ptr[plane] as *mut u8, dst_ls[plane] as usize, half_dst_w, half_dst_h,
                rotation,
            );
        }
    }

    Ok(dst)
}

/// Rotate a single plane of pixel data.
unsafe fn rotate_plane(
    src: *const u8, src_stride: usize, src_w: usize, src_h: usize,
    dst: *mut u8, dst_stride: usize, _dst_w: usize, _dst_h: usize,
    rotation: u32,
) {
    unsafe {
        match rotation {
            90 => {
                for y in 0..src_h {
                    for x in 0..src_w {
                        let dst_x = src_h - 1 - y;
                        let dst_y = x;
                        *dst.add(dst_y * dst_stride + dst_x) = *src.add(y * src_stride + x);
                    }
                }
            }
            180 => {
                for y in 0..src_h {
                    for x in 0..src_w {
                        let dst_x = src_w - 1 - x;
                        let dst_y = src_h - 1 - y;
                        *dst.add(dst_y * dst_stride + dst_x) = *src.add(y * src_stride + x);
                    }
                }
            }
            270 => {
                for y in 0..src_h {
                    for x in 0..src_w {
                        let dst_x = y;
                        let dst_y = src_w - 1 - x;
                        *dst.add(dst_y * dst_stride + dst_x) = *src.add(y * src_stride + x);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Blit a YUV420P source frame onto a YUV420P destination frame at the given offset.
/// Handles bounds clamping to prevent buffer overflows.
fn blit_yuv_frame(src: &AVFrame, dst: &mut AVFrame, offset_x: i32, offset_y: i32) {
    let src_w = src.width as usize;
    let src_h = src.height as usize;
    let dst_w = dst.width as usize;
    let dst_h = dst.height as usize;

    unsafe {
        let src_ptr = (*src.as_ptr()).data;
        let src_linesize = (*src.as_ptr()).linesize;
        let dst_ptr = (*dst.as_mut_ptr()).data;
        let dst_linesize = (*dst.as_mut_ptr()).linesize;

        // Y plane
        let y_src = src_ptr[0] as *const u8;
        let y_dst = dst_ptr[0] as *mut u8;
        let y_src_stride = src_linesize[0] as usize;
        let y_dst_stride = dst_linesize[0] as usize;

        for row in 0..src_h {
            let dst_row = offset_y as usize + row;
            if dst_row >= dst_h {
                break;
            }
            let copy_w = src_w.min(dst_w.saturating_sub(offset_x as usize));
            if copy_w == 0 {
                continue;
            }
            std::ptr::copy_nonoverlapping(
                y_src.add(row * y_src_stride),
                y_dst.add(dst_row * y_dst_stride + offset_x as usize),
                copy_w,
            );
        }

        // U and V planes (4:2:0 — half dimensions and offsets)
        let half_src_w = src_w / 2;
        let half_src_h = src_h / 2;
        let half_dst_w = dst_w / 2;
        let half_dst_h = dst_h / 2;
        let half_off_x = offset_x as usize / 2;
        let half_off_y = offset_y as usize / 2;

        for plane in 1..=2usize {
            let p_src = src_ptr[plane] as *const u8;
            let p_dst = dst_ptr[plane] as *mut u8;
            let p_src_stride = src_linesize[plane] as usize;
            let p_dst_stride = dst_linesize[plane] as usize;

            for row in 0..half_src_h {
                let dst_row = half_off_y + row;
                if dst_row >= half_dst_h {
                    break;
                }
                let copy_w = half_src_w.min(half_dst_w.saturating_sub(half_off_x));
                if copy_w == 0 {
                    continue;
                }
                std::ptr::copy_nonoverlapping(
                    p_src.add(row * p_src_stride),
                    p_dst.add(dst_row * p_dst_stride + half_off_x),
                    copy_w,
                );
            }
        }
    }
}

/// Find the video clip at a timeline position and return (source_path, source_time, effects).
fn find_video_clip_at(
    timeline: &Timeline,
    source_library: &SourceLibrary,
    pos: TimelinePosition,
) -> Option<(PathBuf, f64, Vec<EffectInstance>)> {
    for track in &timeline.tracks {
        if track.track_type == TrackType::Video {
            if let Some(clip) = track.clip_at(pos) {
                if let Some(asset) = source_library.get(clip.asset_id) {
                    let source_time = clip.source_range.start.as_secs_f64()
                        + (pos.as_secs_f64() - clip.timeline_range.start.as_secs_f64());
                    return Some((asset.path.clone(), source_time, clip.effects.clone()));
                }
            }
        }
    }
    None
}

/// Find the audio clip at a timeline position and return (source_path, source_time).
#[allow(dead_code)]
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
