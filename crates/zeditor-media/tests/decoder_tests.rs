use zeditor_media::decoder::{self, FfmpegDecoder, VideoDecoder};
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

#[test]
fn test_open_rotated_video_has_rotation() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video_rotated(dir.path(), "rot_open", 1.0, 320, 240, 90);

    let decoder = FfmpegDecoder::open(&path).unwrap();
    let info = decoder.stream_info();
    assert_eq!(info.rotation, 90, "stream info should report 90° rotation");
    // Raw stream dimensions are still the encoded dimensions
    assert_eq!(info.width, 320);
    assert_eq!(info.height, 240);
}

#[test]
fn test_decode_rotated_dimensions() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video_rotated(dir.path(), "rot_decode", 1.0, 320, 240, 90);

    let mut decoder = FfmpegDecoder::open(&path).unwrap();
    let frame = decoder
        .decode_next_frame_rgba_scaled(320, 320)
        .unwrap()
        .expect("should decode a frame");

    // After 90° rotation, a 320x240 source displayed as 240x320.
    // The frame_to_scaled method scales to fit within max bounds considering post-rotation size,
    // then rotates. So the output should be taller than wide.
    assert!(
        frame.width < frame.height,
        "rotated frame should be taller than wide: {}x{}",
        frame.width,
        frame.height
    );
}

/// Test pure RGBA 90° rotation on a small 2x3 pixel image.
#[test]
fn test_rotate_rgba_90() {
    // 2x3 image (w=2, h=3), RGBA = 4 bytes per pixel
    // Layout (row-major):
    //   row0: [R,G,B,A] [R,G,B,A]  = pixels (0,0) (1,0)
    //   row1: [R,G,B,A] [R,G,B,A]  = pixels (0,1) (1,1)
    //   row2: [R,G,B,A] [R,G,B,A]  = pixels (0,2) (1,2)
    let data: Vec<u8> = vec![
        1, 0, 0, 255, 2, 0, 0, 255, // row 0
        3, 0, 0, 255, 4, 0, 0, 255, // row 1
        5, 0, 0, 255, 6, 0, 0, 255, // row 2
    ];

    let (rotated, new_w, new_h) = decoder::rotate_rgba_90(&data, 2, 3);
    // 90° CW rotation of 2x3 → 3x2
    assert_eq!(new_w, 3);
    assert_eq!(new_h, 2);
    assert_eq!(rotated.len(), (3 * 2 * 4) as usize);

    // After 90° CW rotation:
    //   Original:      Rotated (3x2):
    //   1 2            5 3 1
    //   3 4            6 4 2
    //   5 6
    // row0: pixel(0,0)=5, pixel(1,0)=3, pixel(2,0)=1
    // row1: pixel(0,1)=6, pixel(1,1)=4, pixel(2,1)=2
    assert_eq!(rotated[0], 5); // (0,0)
    assert_eq!(rotated[4], 3); // (1,0)
    assert_eq!(rotated[8], 1); // (2,0)
    assert_eq!(rotated[12], 6); // (0,1)
    assert_eq!(rotated[16], 4); // (1,1)
    assert_eq!(rotated[20], 2); // (2,1)
}

/// Test pure RGBA 180° rotation.
#[test]
fn test_rotate_rgba_180() {
    let data: Vec<u8> = vec![
        1, 0, 0, 255, 2, 0, 0, 255, // row 0
        3, 0, 0, 255, 4, 0, 0, 255, // row 1
    ];

    let (rotated, new_w, new_h) = decoder::rotate_rgba_180(&data, 2, 2);
    assert_eq!(new_w, 2);
    assert_eq!(new_h, 2);
    // 180° rotation: reverse all pixels
    // Original: 1 2 / 3 4  →  4 3 / 2 1
    assert_eq!(rotated[0], 4);
    assert_eq!(rotated[4], 3);
    assert_eq!(rotated[8], 2);
    assert_eq!(rotated[12], 1);
}

/// Test pure RGBA 270° rotation.
#[test]
fn test_rotate_rgba_270() {
    let data: Vec<u8> = vec![
        1, 0, 0, 255, 2, 0, 0, 255, // row 0
        3, 0, 0, 255, 4, 0, 0, 255, // row 1
        5, 0, 0, 255, 6, 0, 0, 255, // row 2
    ];

    let (rotated, new_w, new_h) = decoder::rotate_rgba_270(&data, 2, 3);
    // 270° CW rotation of 2x3 → 3x2
    assert_eq!(new_w, 3);
    assert_eq!(new_h, 2);
    // 270° CW = 90° CCW:
    //   Original:      Rotated (3x2):
    //   1 2            2 4 6
    //   3 4            1 3 5
    //   5 6
    assert_eq!(rotated[0], 2);
    assert_eq!(rotated[4], 4);
    assert_eq!(rotated[8], 6);
    assert_eq!(rotated[12], 1);
    assert_eq!(rotated[16], 3);
    assert_eq!(rotated[20], 5);
}
