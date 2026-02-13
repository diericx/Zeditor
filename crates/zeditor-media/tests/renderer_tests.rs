use std::path::PathBuf;

use zeditor_core::media::{MediaAsset, SourceLibrary};
use zeditor_core::timeline::{Clip, TimeRange, Timeline, TimelinePosition};
use zeditor_media::renderer::{derive_render_config, render_timeline, RenderConfig};
use zeditor_test_harness::fixtures;

/// Helper: create a timeline with one video track, one audio track,
/// and a single clip spanning the full source duration.
fn single_clip_timeline(asset: &MediaAsset, has_audio: bool) -> (Timeline, SourceLibrary) {
    let mut timeline = Timeline::new();
    let video_track_idx = timeline.add_track("Video 1", zeditor_core::timeline::TrackType::Video);
    let source_range = TimeRange {
        start: TimelinePosition::zero(),
        end: TimelinePosition::from_secs_f64(asset.duration.as_secs_f64()),
    };
    let clip = Clip::new(asset.id, TimelinePosition::zero(), source_range);
    timeline
        .add_clip_trimming_overlaps(video_track_idx, clip)
        .unwrap();

    if has_audio {
        let audio_track_idx =
            timeline.add_track("Audio 1", zeditor_core::timeline::TrackType::Audio);
        let audio_clip = Clip::new(asset.id, TimelinePosition::zero(), source_range);
        timeline
            .add_clip_trimming_overlaps(audio_track_idx, audio_clip)
            .unwrap();
    }

    let mut source_library = SourceLibrary::new();
    source_library.import(asset.clone());

    (timeline, source_library)
}

#[test]
fn test_render_config_defaults() {
    let config = RenderConfig::default_with_path(PathBuf::from("/tmp/test.mkv"));
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
    assert!((config.fps - 30.0).abs() < 0.001);
    assert_eq!(config.crf, 22);
    assert_eq!(config.preset, "superfast");
    assert_eq!(config.output_path, PathBuf::from("/tmp/test.mkv"));
}

#[test]
fn test_render_single_clip() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "render_single", 2.0);
    let output_path = dir.path().join("output_single.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
    };

    render_timeline(&timeline, &source_library, &config).unwrap();

    // Verify output exists and is non-empty
    assert!(output_path.exists(), "Output file should exist");
    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.len() > 0, "Output file should be non-empty");

    // Probe output to verify it has a video stream with expected dimensions
    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    assert_eq!(output_asset.width, 320);
    assert_eq!(output_asset.height, 240);
    // Duration should be approximately 2 seconds
    let dur = output_asset.duration.as_secs_f64();
    assert!(
        dur >= 1.5 && dur <= 3.0,
        "Expected ~2s duration, got {dur}s"
    );
}

#[test]
fn test_render_with_gap() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "render_gap", 1.0);
    let output_path = dir.path().join("output_gap.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();

    // Place clip at t=1s, so 0-1s is a gap (black frames)
    let mut timeline = Timeline::new();
    let video_track_idx = timeline.add_track("Video 1", zeditor_core::timeline::TrackType::Video);
    let source_range = TimeRange {
        start: TimelinePosition::zero(),
        end: TimelinePosition::from_secs_f64(asset.duration.as_secs_f64()),
    };
    let clip = Clip::new(
        asset.id,
        TimelinePosition::from_secs_f64(1.0),
        source_range,
    );
    timeline
        .add_clip_trimming_overlaps(video_track_idx, clip)
        .unwrap();

    let mut source_library = SourceLibrary::new();
    source_library.import(asset.clone());

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
    };

    render_timeline(&timeline, &source_library, &config).unwrap();

    assert!(output_path.exists());
    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    // Duration should be approximately 2s (1s gap + 1s clip)
    let dur = output_asset.duration.as_secs_f64();
    assert!(
        dur >= 1.5 && dur <= 3.0,
        "Expected ~2s duration, got {dur}s"
    );
}

#[test]
fn test_render_multiple_clips() {
    let dir = fixtures::fixture_dir();
    let video_path1 = fixtures::generate_test_video(dir.path(), "render_multi1", 1.0);
    let video_path2 = fixtures::generate_test_video(dir.path(), "render_multi2", 1.0);
    let output_path = dir.path().join("output_multi.mkv");

    let asset1 = zeditor_media::probe::probe(&video_path1).unwrap();
    let asset2 = zeditor_media::probe::probe(&video_path2).unwrap();

    let mut timeline = Timeline::new();
    let video_track_idx = timeline.add_track("Video 1", zeditor_core::timeline::TrackType::Video);

    // Clip 1 at t=0
    let source_range1 = TimeRange {
        start: TimelinePosition::zero(),
        end: TimelinePosition::from_secs_f64(asset1.duration.as_secs_f64()),
    };
    let clip1 = Clip::new(asset1.id, TimelinePosition::zero(), source_range1);
    timeline
        .add_clip_trimming_overlaps(video_track_idx, clip1)
        .unwrap();

    // Clip 2 right after clip 1
    let clip2_start = asset1.duration.as_secs_f64();
    let source_range2 = TimeRange {
        start: TimelinePosition::zero(),
        end: TimelinePosition::from_secs_f64(asset2.duration.as_secs_f64()),
    };
    let clip2 = Clip::new(
        asset2.id,
        TimelinePosition::from_secs_f64(clip2_start),
        source_range2,
    );
    timeline
        .add_clip_trimming_overlaps(video_track_idx, clip2)
        .unwrap();

    let mut source_library = SourceLibrary::new();
    source_library.import(asset1.clone());
    source_library.import(asset2.clone());

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
    };

    render_timeline(&timeline, &source_library, &config).unwrap();

    assert!(output_path.exists());
    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    // Duration should be approximately 2s (1s + 1s)
    let dur = output_asset.duration.as_secs_f64();
    assert!(
        dur >= 1.5 && dur <= 3.0,
        "Expected ~2s duration, got {dur}s"
    );
}

#[test]
fn test_render_with_audio() {
    let dir = fixtures::fixture_dir();
    let video_path =
        fixtures::generate_test_video_with_audio(dir.path(), "render_audio", 2.0);
    let output_path = dir.path().join("output_audio.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    assert!(asset.has_audio, "Test asset should have audio");

    let (timeline, source_library) = single_clip_timeline(&asset, true);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
    };

    render_timeline(&timeline, &source_library, &config).unwrap();

    assert!(output_path.exists());
    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    assert!(output_asset.has_audio, "Output should have audio");
    assert!(output_asset.width > 0, "Output should have video");
}

#[test]
fn test_render_empty_timeline() {
    let dir = fixtures::fixture_dir();
    let output_path = dir.path().join("output_empty.mkv");

    let timeline = Timeline::new();
    let source_library = SourceLibrary::new();

    let config = RenderConfig::default_with_path(output_path);
    let result = render_timeline(&timeline, &source_library, &config);
    assert!(result.is_err(), "Empty timeline should produce an error");
}

#[test]
fn test_derive_render_config_from_asset() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "derive_config", 1.0);
    let output_path = PathBuf::from("/tmp/derived_output.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = derive_render_config(&timeline, &source_library, output_path.clone());
    // Test video is 320x240
    assert_eq!(config.width, 320);
    assert_eq!(config.height, 240);
    assert_eq!(config.output_path, output_path);
}

#[test]
fn test_derive_render_config_empty_timeline() {
    let timeline = Timeline::new();
    let source_library = SourceLibrary::new();
    let output_path = PathBuf::from("/tmp/empty_output.mkv");

    let config = derive_render_config(&timeline, &source_library, output_path);
    // Should fall back to defaults
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
}
