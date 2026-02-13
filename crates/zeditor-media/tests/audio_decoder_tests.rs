use zeditor_media::audio_decoder::FfmpegAudioDecoder;
use zeditor_test_harness::fixtures;

#[test]
fn test_open_audio_stream() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video_with_audio(dir.path(), "audio_open", 2.0);

    let decoder = FfmpegAudioDecoder::open(&path).unwrap();
    assert!(decoder.sample_rate() > 0);
    assert!(decoder.channels() > 0);
}

#[test]
fn test_decode_audio_frames() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video_with_audio(dir.path(), "audio_decode", 2.0);

    let mut decoder = FfmpegAudioDecoder::open(&path).unwrap();

    let frame = decoder.decode_next_audio_frame().unwrap();
    assert!(frame.is_some(), "should decode at least one frame");

    let frame = frame.unwrap();
    assert!(!frame.samples.is_empty(), "samples should not be empty");
    assert!(frame.sample_rate > 0);
    assert!(frame.channels > 0);
}

#[test]
fn test_decode_multiple_audio_frames() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video_with_audio(dir.path(), "audio_multi", 2.0);

    let mut decoder = FfmpegAudioDecoder::open(&path).unwrap();

    let mut count = 0;
    while let Ok(Some(_frame)) = decoder.decode_next_audio_frame() {
        count += 1;
        if count > 10 {
            break;
        }
    }
    assert!(count > 1, "should decode multiple frames, got {count}");
}

#[test]
fn test_seek_audio() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video_with_audio(dir.path(), "audio_seek", 5.0);

    let mut decoder = FfmpegAudioDecoder::open(&path).unwrap();
    decoder.seek_to(2.0).unwrap();

    let frame = decoder.decode_next_audio_frame().unwrap();
    assert!(frame.is_some(), "should decode frame after seek");

    let frame = frame.unwrap();
    // PTS should be near the seek target (within ~1s due to keyframe seeking)
    assert!(
        frame.pts_secs >= 1.0,
        "PTS after seek to 2.0 should be >= 1.0, got {}",
        frame.pts_secs
    );
}

#[test]
fn test_no_audio_stream_returns_error() {
    let dir = fixtures::fixture_dir();
    // Generate video-only file (no audio)
    let path = fixtures::generate_test_video(dir.path(), "video_only", 1.0);

    let result = FfmpegAudioDecoder::open(&path);
    assert!(result.is_err(), "should fail for video-only file");
}

#[test]
fn test_audio_samples_are_valid_f32() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video_with_audio(dir.path(), "audio_valid", 2.0);

    let mut decoder = FfmpegAudioDecoder::open(&path).unwrap();
    let frame = decoder.decode_next_audio_frame().unwrap().unwrap();

    // All samples should be finite f32 values
    for sample in &frame.samples {
        assert!(sample.is_finite(), "sample should be finite, got {sample}");
    }

    // Samples should have reasonable amplitude (sine wave at 440Hz)
    let max_amp = frame.samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    assert!(max_amp > 0.0, "audio should have non-zero amplitude");
    assert!(max_amp <= 1.5, "audio amplitude should be reasonable, got {max_amp}");
}
