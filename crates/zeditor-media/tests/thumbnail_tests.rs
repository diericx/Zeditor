use zeditor_media::thumbnail;
use zeditor_test_harness::fixtures;

#[test]
fn test_generate_thumbnail() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video(dir.path(), "thumb_test", 1.0);

    let frame = thumbnail::generate_thumbnail(&path).unwrap();
    assert_eq!(frame.width, 320);
    assert_eq!(frame.height, 240);
    assert_eq!(frame.data.len(), (320 * 240 * 3) as usize);
    // First frame should have pts near 0.
    assert!(
        frame.pts_secs < 0.1,
        "first frame pts: {}",
        frame.pts_secs
    );
}

#[test]
fn test_generate_thumbnail_at_timestamp() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video(dir.path(), "thumb_at_test", 3.0);

    let frame = thumbnail::generate_thumbnail_at(&path, 1.5).unwrap();
    assert_eq!(frame.width, 320);
    assert_eq!(frame.height, 240);
    // Verify we got a valid frame (the exact PTS depends on keyframe placement).
    assert!(frame.data.len() > 0, "frame should have pixel data");
}
