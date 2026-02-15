use std::path::Path;

use crate::error::{MediaError, Result};

/// Decoded audio frame with interleaved f32 PCM samples.
pub struct AudioFrame {
    /// Interleaved f32 PCM samples (L, R, L, R, ... for stereo).
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Presentation timestamp in seconds.
    pub pts_secs: f64,
}

pub struct FfmpegAudioDecoder {
    input_ctx: rsmpeg::avformat::AVFormatContextInput,
    decode_ctx: rsmpeg::avcodec::AVCodecContext,
    swr_ctx: rsmpeg::swresample::SwrContext,
    audio_stream_index: usize,
    sample_rate: u32,
    channels: u16,
}

impl FfmpegAudioDecoder {
    pub fn open(path: &Path) -> Result<Self> {
        use std::ffi::CString;

        let path_str = path.to_string_lossy().to_string();
        let c_path = CString::new(path_str.clone())
            .map_err(|_| MediaError::OpenFailed(path_str.clone()))?;

        let input_ctx = rsmpeg::avformat::AVFormatContextInput::open(&c_path)
            .map_err(|e| MediaError::OpenFailed(format!("{path_str}: {e}")))?;

        let (audio_stream_index, decoder) = {
            let streams = input_ctx.streams();
            let mut found = None;
            for (i, stream) in streams.iter().enumerate() {
                let codecpar = stream.codecpar();
                if codecpar.codec_type == rsmpeg::ffi::AVMEDIA_TYPE_AUDIO {
                    let codec_id = codecpar.codec_id;
                    if let Some(decoder) = rsmpeg::avcodec::AVCodec::find_decoder(codec_id) {
                        found = Some((i, decoder));
                        break;
                    }
                }
            }
            found.ok_or(MediaError::NoAudioStream)?
        };

        // Tell the demuxer to discard non-audio streams so read_packet() skips
        // video packets entirely. Without this, decoding audio from a video file
        // reads and discards many video packets per audio frame, making multi-clip
        // audio decode too slow for real-time playback.
        {
            let nb_streams = input_ctx.streams().len();
            for i in 0..nb_streams {
                if i != audio_stream_index {
                    unsafe {
                        let streams = input_ctx.streams();
                        let stream_ptr = streams[i].as_ptr() as *mut rsmpeg::ffi::AVStream;
                        (*stream_ptr).discard = rsmpeg::ffi::AVDISCARD_ALL;
                    }
                }
            }
        }

        let mut decode_ctx = rsmpeg::avcodec::AVCodecContext::new(&decoder);
        {
            let streams = input_ctx.streams();
            let audio_stream = &streams[audio_stream_index];
            decode_ctx
                .apply_codecpar(&audio_stream.codecpar())
                .map_err(|e| MediaError::DecoderError(format!("apply_codecpar: {e}")))?;
        }

        // Enable multithreaded decoding
        unsafe {
            use rsmpeg::UnsafeDerefMut;
            decode_ctx.deref_mut().thread_count = 0;
        }

        decode_ctx
            .open(None)
            .map_err(|e| MediaError::DecoderError(format!("open: {e}")))?;

        let in_sample_rate = decode_ctx.sample_rate;
        let in_sample_fmt = decode_ctx.sample_fmt;
        let in_ch_layout = unsafe {
            rsmpeg::avutil::AVChannelLayoutRef::new(&decode_ctx.ch_layout)
        };
        let channels = in_ch_layout.nb_channels as u16;

        // Output: same channel count as input, f32 interleaved, same sample rate
        let out_ch_layout = rsmpeg::avutil::AVChannelLayout::from_nb_channels(channels as i32);

        let mut swr_ctx = rsmpeg::swresample::SwrContext::new(
            &out_ch_layout,
            rsmpeg::ffi::AV_SAMPLE_FMT_FLT,
            in_sample_rate,
            &in_ch_layout,
            in_sample_fmt,
            in_sample_rate,
        )
        .map_err(|e| MediaError::DecoderError(format!("swr_alloc_set_opts2: {e}")))?;

        swr_ctx
            .init()
            .map_err(|e| MediaError::DecoderError(format!("swr_init: {e}")))?;

        Ok(Self {
            input_ctx,
            decode_ctx,
            swr_ctx,
            audio_stream_index,
            sample_rate: in_sample_rate as u32,
            channels,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Decode the next audio frame, returning interleaved f32 PCM samples.
    pub fn decode_next_audio_frame(&mut self) -> Result<Option<AudioFrame>> {
        loop {
            match self.input_ctx.read_packet() {
                Ok(Some(packet)) => {
                    if packet.stream_index as usize != self.audio_stream_index {
                        continue;
                    }
                    self.decode_ctx.send_packet(Some(&packet)).map_err(|e| {
                        MediaError::DecoderError(format!("send_packet: {e}"))
                    })?;

                    match self.decode_ctx.receive_frame() {
                        Ok(frame) => {
                            return Ok(Some(self.convert_frame(&frame)?));
                        }
                        Err(_) => continue,
                    }
                }
                Ok(None) => {
                    // EOF: flush decoder
                    self.decode_ctx.send_packet(None).ok();
                    match self.decode_ctx.receive_frame() {
                        Ok(frame) => return Ok(Some(self.convert_frame(&frame)?)),
                        Err(_) => return Ok(None),
                    }
                }
                Err(e) => {
                    return Err(MediaError::DecoderError(format!("read_packet: {e}")));
                }
            }
        }
    }

    /// Seek to a timestamp in the audio stream.
    pub fn seek_to(&mut self, timestamp_secs: f64) -> Result<()> {
        let streams = self.input_ctx.streams();
        let audio_stream = &streams[self.audio_stream_index];
        let tb = audio_stream.time_base;
        let ts = (timestamp_secs * tb.den as f64 / tb.num as f64) as i64;
        let _ = streams;

        self.input_ctx
            .seek(
                self.audio_stream_index as i32,
                ts,
                rsmpeg::ffi::AVSEEK_FLAG_BACKWARD as i32,
            )
            .map_err(|e| MediaError::SeekError(format!("{e}")))?;

        self.decode_ctx.flush_buffers();

        Ok(())
    }

    fn convert_frame(&mut self, frame: &rsmpeg::avutil::AVFrame) -> Result<AudioFrame> {
        let nb_samples = frame.nb_samples;

        // Create output frame for conversion
        let mut dst_frame = rsmpeg::avutil::AVFrame::new();
        dst_frame.set_format(rsmpeg::ffi::AV_SAMPLE_FMT_FLT);
        dst_frame.set_sample_rate(self.sample_rate as i32);

        // Set output channel layout
        let out_ch_layout = rsmpeg::avutil::AVChannelLayout::from_nb_channels(self.channels as i32);
        unsafe {
            rsmpeg::ffi::av_channel_layout_copy(
                &mut (*dst_frame.as_mut_ptr()).ch_layout,
                out_ch_layout.as_ptr(),
            );
        }
        dst_frame.set_nb_samples(nb_samples);
        dst_frame.alloc_buffer().map_err(|e| {
            MediaError::DecoderError(format!("alloc_buffer: {e}"))
        })?;

        self.swr_ctx
            .convert_frame(Some(frame), &mut dst_frame)
            .map_err(|e| MediaError::DecoderError(format!("convert_frame: {e}")))?;

        let actual_samples = dst_frame.nb_samples;
        let total_floats = actual_samples as usize * self.channels as usize;
        let samples = unsafe {
            std::slice::from_raw_parts(
                dst_frame.data[0] as *const f32,
                total_floats,
            )
            .to_vec()
        };

        let pts_secs = {
            let streams = self.input_ctx.streams();
            let tb = streams[self.audio_stream_index].time_base;
            if frame.pts != rsmpeg::ffi::AV_NOPTS_VALUE {
                frame.pts as f64 * tb.num as f64 / tb.den as f64
            } else {
                0.0
            }
        };

        Ok(AudioFrame {
            samples,
            sample_rate: self.sample_rate,
            channels: self.channels,
            pts_secs,
        })
    }
}
