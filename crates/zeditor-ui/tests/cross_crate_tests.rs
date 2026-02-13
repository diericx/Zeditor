#![allow(unused_must_use)]
//! Cross-crate integration tests verifying the full pipeline:
//! test-harness → media → core → ui

use zeditor_core::timeline::TimelinePosition;
use zeditor_media::{decoder::VideoDecoder, probe};
use zeditor_test_harness::{builders::MediaAssetBuilder, fixtures};
use zeditor_ui::app::App;
use zeditor_ui::message::Message;

/// Test the full pipeline: generate fixture → probe → import → add to timeline.
#[test]
fn test_fixture_to_timeline_pipeline() {
    // Generate a test video.
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "pipeline_test", 2.0);

    // Probe the video to get metadata.
    let asset = probe::probe(&video_path).unwrap();
    assert_eq!(asset.width, 320);
    assert_eq!(asset.height, 240);

    // Import into the UI app.
    let mut app = App::new();
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    assert_eq!(app.project.source_library.len(), 1);

    // Add to timeline.
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
}

/// Test decoding frames from a fixture video.
#[test]
fn test_decode_fixture_frames() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "decode_integration", 1.0);

    let mut decoder =
        zeditor_media::decoder::FfmpegDecoder::open(&video_path).unwrap();
    let info = decoder.stream_info();
    assert_eq!(info.width, 320);
    assert_eq!(info.height, 240);

    let frame = decoder.decode_next_frame().unwrap().unwrap();
    assert_eq!(frame.width, 320);
    assert_eq!(frame.height, 240);
    assert_eq!(frame.data.len(), (320 * 240 * 3) as usize);
}

/// Test that builder-created assets work with the UI.
#[test]
fn test_builder_to_ui() {
    let asset = MediaAssetBuilder::new("test_clip")
        .duration_secs(8.0)
        .resolution(1280, 720)
        .fps(24.0)
        .build();

    let mut app = App::new();
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    let clip = &app.project.timeline.tracks[0].clips[0];
    let clip_duration = clip.duration().as_secs_f64();
    assert!(
        (clip_duration - 8.0).abs() < 0.01,
        "clip duration should be ~8s, got {clip_duration}"
    );
}
