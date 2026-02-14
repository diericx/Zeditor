use std::path::Path;

use crate::error::Result;

/// Decoded video frame with raw pixel data.
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    /// RGB pixel data, row-major, 3 bytes per pixel.
    pub data: Vec<u8>,
    /// Presentation timestamp in seconds.
    pub pts_secs: f64,
}

/// Trait for video decoders, enabling test mocking.
pub trait VideoDecoder: Send {
    fn open(path: &Path) -> Result<Self>
    where
        Self: Sized;

    fn decode_next_frame(&mut self) -> Result<Option<VideoFrame>>;

    fn seek_to(&mut self, timestamp_secs: f64) -> Result<()>;

    fn stream_info(&self) -> StreamInfo;
}

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_secs: f64,
    pub codec_name: String,
}

pub struct FfmpegDecoder {
    input_ctx: rsmpeg::avformat::AVFormatContextInput,
    decode_ctx: rsmpeg::avcodec::AVCodecContext,
    sws_ctx: Option<rsmpeg::swscale::SwsContext>,
    sws_dst_dims: (i32, i32),
    video_stream_index: usize,
    stream_info: StreamInfo,
}

impl VideoDecoder for FfmpegDecoder {
    fn open(path: &Path) -> Result<Self> {
        use std::ffi::CString;

        let path_str = path.to_string_lossy().to_string();
        let c_path = CString::new(path_str.clone())
            .map_err(|_| crate::error::MediaError::OpenFailed(path_str.clone()))?;

        let input_ctx = rsmpeg::avformat::AVFormatContextInput::open(&c_path)
            .map_err(|e| crate::error::MediaError::OpenFailed(format!("{path_str}: {e}")))?;

        let (video_stream_index, decoder) = {
            let streams = input_ctx.streams();
            let mut found = None;
            for (i, stream) in streams.iter().enumerate() {
                let codecpar = stream.codecpar();
                if codecpar.codec_type == rsmpeg::ffi::AVMEDIA_TYPE_VIDEO {
                    let codec_id = codecpar.codec_id;
                    if let Some(decoder) = rsmpeg::avcodec::AVCodec::find_decoder(codec_id) {
                        found = Some((i, decoder));
                        break;
                    }
                }
            }
            found.ok_or(crate::error::MediaError::NoVideoStream)?
        };

        let mut decode_ctx = rsmpeg::avcodec::AVCodecContext::new(&decoder);
        {
            let streams = input_ctx.streams();
            let video_stream = &streams[video_stream_index];
            decode_ctx
                .apply_codecpar(&video_stream.codecpar())
                .map_err(|e| {
                    crate::error::MediaError::DecoderError(format!("apply_codecpar: {e}"))
                })?;
        }
        // Enable multithreaded decoding (0 = auto-detect thread count).
        // Critical for 4K performance: frame/slice threading gives 4-8x speedup.
        unsafe {
            use rsmpeg::UnsafeDerefMut;
            decode_ctx.deref_mut().thread_count = 0;
        }

        decode_ctx
            .open(None)
            .map_err(|e| crate::error::MediaError::DecoderError(format!("open: {e}")))?;

        let width = decode_ctx.width as u32;
        let height = decode_ctx.height as u32;

        let stream_info = {
            let streams = input_ctx.streams();
            let video_stream = &streams[video_stream_index];
            let tb = video_stream.time_base;
            let duration = if video_stream.duration > 0 {
                video_stream.duration as f64 * tb.num as f64 / tb.den as f64
            } else {
                input_ctx.duration as f64 / rsmpeg::ffi::AV_TIME_BASE as f64
            };
            let fps = {
                let r = video_stream.r_frame_rate;
                if r.den > 0 {
                    r.num as f64 / r.den as f64
                } else {
                    30.0
                }
            };
            StreamInfo {
                width,
                height,
                fps,
                duration_secs: duration,
                codec_name: decoder.name().to_string_lossy().to_string(),
            }
        };

        Ok(Self {
            input_ctx,
            decode_ctx,
            sws_ctx: None,
            sws_dst_dims: (0, 0),
            video_stream_index,
            stream_info,
        })
    }

    fn decode_next_frame(&mut self) -> Result<Option<VideoFrame>> {
        loop {
            match self.input_ctx.read_packet() {
                Ok(Some(packet)) => {
                    if packet.stream_index as usize != self.video_stream_index {
                        continue;
                    }
                    self.decode_ctx.send_packet(Some(&packet)).map_err(|e| {
                        crate::error::MediaError::DecoderError(format!("send_packet: {e}"))
                    })?;

                    match self.decode_ctx.receive_frame() {
                        Ok(frame) => {
                            return Ok(Some(self.frame_to_rgb(&frame)?));
                        }
                        Err(_) => continue,
                    }
                }
                Ok(None) => {
                    // EOF: flush decoder.
                    self.decode_ctx.send_packet(None).ok();
                    match self.decode_ctx.receive_frame() {
                        Ok(frame) => return Ok(Some(self.frame_to_rgb(&frame)?)),
                        Err(_) => return Ok(None),
                    }
                }
                Err(e) => {
                    return Err(crate::error::MediaError::DecoderError(format!(
                        "read_packet: {e}"
                    )));
                }
            }
        }
    }

    fn seek_to(&mut self, timestamp_secs: f64) -> Result<()> {
        let streams = self.input_ctx.streams();
        let video_stream = &streams[self.video_stream_index];
        let tb = video_stream.time_base;
        let ts = (timestamp_secs * tb.den as f64 / tb.num as f64) as i64;
        let _ = streams;

        self.input_ctx
            .seek(self.video_stream_index as i32, ts, rsmpeg::ffi::AVSEEK_FLAG_BACKWARD as i32)
            .map_err(|e| crate::error::MediaError::SeekError(format!("{e}")))?;

        self.decode_ctx.flush_buffers();

        Ok(())
    }

    fn stream_info(&self) -> StreamInfo {
        self.stream_info.clone()
    }
}

impl FfmpegDecoder {
    /// Decode the next raw video frame without pixel format conversion.
    /// Returns the raw AVFrame in the decoder's native pixel format along with PTS in seconds.
    /// Used by the renderer to avoid unnecessary RGB round-trips that lose data due to stride.
    pub(crate) fn decode_next_raw_frame(
        &mut self,
    ) -> Result<Option<(rsmpeg::avutil::AVFrame, f64)>> {
        loop {
            match self.input_ctx.read_packet() {
                Ok(Some(packet)) => {
                    if packet.stream_index as usize != self.video_stream_index {
                        continue;
                    }
                    self.decode_ctx.send_packet(Some(&packet)).map_err(|e| {
                        crate::error::MediaError::DecoderError(format!("send_packet: {e}"))
                    })?;

                    match self.decode_ctx.receive_frame() {
                        Ok(frame) => {
                            let pts_secs = self.frame_pts_secs(&frame);
                            return Ok(Some((frame, pts_secs)));
                        }
                        Err(_) => continue,
                    }
                }
                Ok(None) => {
                    self.decode_ctx.send_packet(None).ok();
                    match self.decode_ctx.receive_frame() {
                        Ok(frame) => {
                            let pts_secs = self.frame_pts_secs(&frame);
                            return Ok(Some((frame, pts_secs)));
                        }
                        Err(_) => return Ok(None),
                    }
                }
                Err(e) => {
                    return Err(crate::error::MediaError::DecoderError(format!(
                        "read_packet: {e}"
                    )));
                }
            }
        }
    }

    fn frame_pts_secs(&self, frame: &rsmpeg::avutil::AVFrame) -> f64 {
        let streams = self.input_ctx.streams();
        let tb = streams[self.video_stream_index].time_base;
        if frame.pts != rsmpeg::ffi::AV_NOPTS_VALUE {
            frame.pts as f64 * tb.num as f64 / tb.den as f64
        } else {
            0.0
        }
    }

    /// Decode the next frame, scaling to the given max dimensions for preview.
    /// Maintains aspect ratio. If max_width/max_height are 0, uses original size.
    /// Output is RGB24 format.
    pub fn decode_next_frame_scaled(
        &mut self,
        max_width: u32,
        max_height: u32,
    ) -> Result<Option<VideoFrame>> {
        self.decode_next_frame_internal(max_width, max_height, false)
    }

    /// Decode the next frame, scaling to the given max dimensions for preview.
    /// Output is RGBA32 format (4 bytes per pixel), ready for display.
    pub fn decode_next_frame_rgba_scaled(
        &mut self,
        max_width: u32,
        max_height: u32,
    ) -> Result<Option<VideoFrame>> {
        self.decode_next_frame_internal(max_width, max_height, true)
    }

    fn decode_next_frame_internal(
        &mut self,
        max_width: u32,
        max_height: u32,
        rgba: bool,
    ) -> Result<Option<VideoFrame>> {
        loop {
            match self.input_ctx.read_packet() {
                Ok(Some(packet)) => {
                    if packet.stream_index as usize != self.video_stream_index {
                        continue;
                    }
                    self.decode_ctx.send_packet(Some(&packet)).map_err(|e| {
                        crate::error::MediaError::DecoderError(format!("send_packet: {e}"))
                    })?;

                    match self.decode_ctx.receive_frame() {
                        Ok(frame) => {
                            return Ok(Some(self.frame_to_scaled(&frame, max_width, max_height, rgba)?));
                        }
                        Err(_) => continue,
                    }
                }
                Ok(None) => {
                    self.decode_ctx.send_packet(None).ok();
                    match self.decode_ctx.receive_frame() {
                        Ok(frame) => {
                            return Ok(Some(self.frame_to_scaled(&frame, max_width, max_height, rgba)?))
                        }
                        Err(_) => return Ok(None),
                    }
                }
                Err(e) => {
                    return Err(crate::error::MediaError::DecoderError(format!(
                        "read_packet: {e}"
                    )));
                }
            }
        }
    }

    fn frame_to_rgb(&mut self, frame: &rsmpeg::avutil::AVFrame) -> Result<VideoFrame> {
        self.frame_to_scaled(frame, 0, 0, false)
    }

    fn frame_to_scaled(
        &mut self,
        frame: &rsmpeg::avutil::AVFrame,
        max_width: u32,
        max_height: u32,
        rgba: bool,
    ) -> Result<VideoFrame> {
        let src_w = frame.width;
        let src_h = frame.height;

        let dst_fmt = if rgba {
            rsmpeg::ffi::AV_PIX_FMT_RGBA
        } else {
            rsmpeg::ffi::AV_PIX_FMT_RGB24
        };
        let bytes_per_pixel: u32 = if rgba { 4 } else { 3 };

        // Calculate output dimensions
        let (dst_w, dst_h) = if max_width > 0 && max_height > 0
            && (src_w as u32 > max_width || src_h as u32 > max_height)
        {
            let scale_w = max_width as f64 / src_w as f64;
            let scale_h = max_height as f64 / src_h as f64;
            let scale = scale_w.min(scale_h);
            let w = ((src_w as f64 * scale) as i32).max(2) & !1; // ensure even
            let h = ((src_h as f64 * scale) as i32).max(2) & !1;
            (w, h)
        } else {
            (src_w, src_h)
        };

        // Recreate SWS context if output dimensions or format changed
        let need_new_sws = self.sws_ctx.is_none()
            || self.sws_dst_dims != (dst_w, dst_h);

        if need_new_sws {
            self.sws_ctx = Some(
                rsmpeg::swscale::SwsContext::get_context(
                    src_w,
                    src_h,
                    frame.format,
                    dst_w,
                    dst_h,
                    dst_fmt,
                    rsmpeg::ffi::SWS_FAST_BILINEAR,
                    None,
                    None,
                    None,
                )
                .ok_or_else(|| {
                    crate::error::MediaError::DecoderError("failed to create sws context".into())
                })?,
            );
            self.sws_dst_dims = (dst_w, dst_h);
        }

        let sws = self.sws_ctx.as_mut().unwrap();

        let mut dst_frame = rsmpeg::avutil::AVFrame::new();
        dst_frame.set_width(dst_w);
        dst_frame.set_height(dst_h);
        dst_frame.set_format(dst_fmt);
        dst_frame
            .alloc_buffer()
            .map_err(|e| crate::error::MediaError::DecoderError(format!("alloc_buffer: {e}")))?;

        sws.scale_frame(frame, 0, src_h, &mut dst_frame)
            .map_err(|e| crate::error::MediaError::DecoderError(format!("scale_frame: {e}")))?;

        let width = dst_w as u32;
        let height = dst_h as u32;
        let data_size = (width * height * bytes_per_pixel) as usize;
        let data = unsafe {
            std::slice::from_raw_parts(dst_frame.data[0] as *const u8, data_size).to_vec()
        };

        let pts_secs = {
            let streams = self.input_ctx.streams();
            let tb = streams[self.video_stream_index].time_base;
            if frame.pts != rsmpeg::ffi::AV_NOPTS_VALUE {
                frame.pts as f64 * tb.num as f64 / tb.den as f64
            } else {
                0.0
            }
        };

        Ok(VideoFrame {
            width,
            height,
            data,
            pts_secs,
        })
    }
}
