use zeditor_media::probe;
use zeditor_test_harness::fixtures;

#[test]
fn test_probe_video_only() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video(dir.path(), "probe_video", 2.0);

    let asset = probe::probe(&path).unwrap();
    assert_eq!(asset.name, "probe_video.mp4");
    assert_eq!(asset.width, 320);
    assert_eq!(asset.height, 240);
    assert!(asset.fps > 29.0 && asset.fps < 31.0, "fps: {}", asset.fps);
    assert!(
        asset.duration.as_secs_f64() > 1.5 && asset.duration.as_secs_f64() < 2.5,
        "duration: {:?}",
        asset.duration
    );
    assert!(!asset.has_audio);
}

#[test]
fn test_probe_video_with_audio() {
    let dir = fixtures::fixture_dir();
    let path = fixtures::generate_test_video_with_audio(dir.path(), "probe_av", 1.0);

    let asset = probe::probe(&path).unwrap();
    assert_eq!(asset.name, "probe_av.mp4");
    assert!(asset.has_audio);
    assert_eq!(asset.width, 320);
    assert_eq!(asset.height, 240);
}
