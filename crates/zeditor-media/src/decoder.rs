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
    fn frame_to_rgb(&mut self, frame: &rsmpeg::avutil::AVFrame) -> Result<VideoFrame> {
        let width = frame.width as u32;
        let height = frame.height as u32;

        let sws = self.sws_ctx.get_or_insert_with(|| {
            rsmpeg::swscale::SwsContext::get_context(
                frame.width,
                frame.height,
                frame.format,
                frame.width,
                frame.height,
                rsmpeg::ffi::AV_PIX_FMT_RGB24,
                rsmpeg::ffi::SWS_FAST_BILINEAR,
                None,
                None,
                None,
            )
            .expect("failed to create sws context")
        });

        let mut dst_frame = rsmpeg::avutil::AVFrame::new();
        dst_frame.set_width(frame.width);
        dst_frame.set_height(frame.height);
        dst_frame.set_format(rsmpeg::ffi::AV_PIX_FMT_RGB24);
        dst_frame
            .alloc_buffer()
            .map_err(|e| crate::error::MediaError::DecoderError(format!("alloc_buffer: {e}")))?;

        sws.scale_frame(frame, 0, frame.height, &mut dst_frame)
            .map_err(|e| crate::error::MediaError::DecoderError(format!("scale_frame: {e}")))?;

        let data_size = (width * height * 3) as usize;
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
