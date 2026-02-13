use zeditor_media::decoder::{FfmpegDecoder, VideoDecoder};
use zeditor_test_harness::fixtures;

#[test]
fn test_open_video() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video(dir.path(), "open_test", 1.0);

    let decoder = FfmpegDecoder::open(&path).unwrap();
    let info = decoder.stream_info();
    assert_eq!(info.width, 320);
    assert_eq!(info.height, 240);
    assert!(info.fps > 29.0 && info.fps < 31.0, "fps: {}", info.fps);
    assert!(
        info.duration_secs > 0.8 && info.duration_secs < 1.5,
        "duration: {}",
        info.duration_secs
    );
}

#[test]
fn test_decode_frames() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video(dir.path(), "decode_test", 1.0);

    let mut decoder = FfmpegDecoder::open(&path).unwrap();
    let mut frame_count = 0;

    while let Ok(Some(frame)) = decoder.decode_next_frame() {
        assert_eq!(frame.width, 320);
        assert_eq!(frame.height, 240);
        assert_eq!(frame.data.len(), (320 * 240 * 3) as usize);
        frame_count += 1;
    }

    // 1 second at 30fps should yield ~30 frames.
    assert!(
        frame_count >= 25 && frame_count <= 35,
        "expected ~30 frames, got {frame_count}"
    );
}

#[test]
fn test_seek_and_decode() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video(dir.path(), "seek_test", 3.0);

    let mut decoder = FfmpegDecoder::open(&path).unwrap();

    // Seek to 2 seconds.
    decoder.seek_to(2.0).unwrap();

    // Should still be able to decode frames after seeking.
    let frame = decoder.decode_next_frame().unwrap();
    assert!(frame.is_some(), "should decode a frame after seeking");

    let frame = frame.unwrap();
    assert_eq!(frame.width, 320);
    assert_eq!(frame.height, 240);
}
