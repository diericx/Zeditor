use std::path::PathBuf;

use zeditor_core::effects::{EffectInstance, EffectType};
use zeditor_core::media::{MediaAsset, SourceLibrary};
use zeditor_core::project::ProjectSettings;
use zeditor_core::timeline::{Clip, TimeRange, Timeline, TimelinePosition};
use zeditor_media::renderer::{
    compute_canvas_layout, derive_render_config, render_timeline, RenderConfig, ScalingAlgorithm,
};
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
    assert_eq!(config.canvas_width, 1920);
    assert_eq!(config.canvas_height, 1080);
    assert!((config.fps - 30.0).abs() < 0.001);
    assert_eq!(config.crf, 22);
    assert_eq!(config.preset, "superfast");
    assert_eq!(config.scaling, ScalingAlgorithm::Lanczos);
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
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

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
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

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
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

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
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

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
    let result = render_timeline(&timeline, &source_library, &config, None);
    assert!(result.is_err(), "Empty timeline should produce an error");
}

#[test]
fn test_derive_render_config_from_asset() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "derive_config", 1.0);
    let output_path = PathBuf::from("/tmp/derived_output.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let settings = ProjectSettings::default();
    let config = derive_render_config(&timeline, &source_library, &settings, output_path.clone());
    // Resolution matches project settings (default 1920x1080)
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
    assert_eq!(config.canvas_width, 1920);
    assert_eq!(config.canvas_height, 1080);
    // FPS should be derived from source (~25fps for testsrc)
    assert!(config.fps > 0.0, "FPS should be derived from source");
    assert_eq!(config.scaling, ScalingAlgorithm::Lanczos);
    assert_eq!(config.output_path, output_path);
}

#[test]
fn test_derive_render_config_empty_timeline() {
    let timeline = Timeline::new();
    let source_library = SourceLibrary::new();
    let output_path = PathBuf::from("/tmp/empty_output.mkv");

    let settings = ProjectSettings::default();
    let config = derive_render_config(&timeline, &source_library, &settings, output_path);
    // Should fall back to defaults
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
}

#[test]
fn test_render_upscale_to_1080p() {
    // Source is 320x240, render at 1920x1080 — verifies upscale works
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "render_upscale", 2.0);
    let output_path = dir.path().join("output_upscale.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    assert_eq!(asset.width, 320);
    assert_eq!(asset.height, 240);

    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 1920,
        height: 1080,
        canvas_width: 1920,
        canvas_height: 1080,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    assert_eq!(output_asset.width, 1920);
    assert_eq!(output_asset.height, 1080);
    let dur = output_asset.duration.as_secs_f64();
    assert!(
        dur >= 1.5 && dur <= 3.0,
        "Expected ~2s duration, got {dur}s"
    );
}

#[test]
fn test_render_upscale_with_audio() {
    // Source is 320x240 with audio, render at 1920x1080 — verifies upscale + audio
    let dir = fixtures::fixture_dir();
    let video_path =
        fixtures::generate_test_video_with_audio(dir.path(), "render_upscale_audio", 2.0);
    let output_path = dir.path().join("output_upscale_audio.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    assert!(asset.has_audio, "Test asset should have audio");
    assert_eq!(asset.width, 320);
    assert_eq!(asset.height, 240);

    let (timeline, source_library) = single_clip_timeline(&asset, true);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 1920,
        height: 1080,
        canvas_width: 1920,
        canvas_height: 1080,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    assert_eq!(output_asset.width, 1920);
    assert_eq!(output_asset.height, 1080);
    assert!(output_asset.has_audio, "Output should have audio");
    let dur = output_asset.duration.as_secs_f64();
    assert!(
        dur >= 1.5 && dur <= 3.0,
        "Expected ~2s duration, got {dur}s"
    );
}

#[test]
fn test_derive_render_config_preserves_1080p_with_any_source() {
    // Even with a 320x240 source, derive_render_config returns 1920x1080
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "derive_preserve", 1.0);

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    assert_eq!(asset.width, 320);
    assert_eq!(asset.height, 240);

    let (timeline, source_library) = single_clip_timeline(&asset, false);
    let output_path = PathBuf::from("/tmp/derive_preserve_output.mkv");

    let settings = ProjectSettings::default();
    let config = derive_render_config(&timeline, &source_library, &settings, output_path);
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
    assert_eq!(config.canvas_width, 1920);
    assert_eq!(config.canvas_height, 1080);
    assert_eq!(config.scaling, ScalingAlgorithm::Lanczos);
    // FPS should be derived from the source asset
    assert!(config.fps > 0.0, "FPS should be derived from source");
}

// =============================================================================
// Canvas composition tests
// =============================================================================

#[test]
fn test_compute_canvas_layout() {
    // 4:3 source on 16:9 canvas at 1920x1080 render
    // Source 320x240 (4:3) fits 1440x1080 centered in 1920x1080 canvas
    let layout = compute_canvas_layout(320, 240, 1920, 1080, 1920, 1080);
    assert_eq!(layout.clip_w, 1440);
    assert_eq!(layout.clip_h, 1080);
    assert_eq!(layout.clip_x, 240);
    assert_eq!(layout.clip_y, 0);
    // All values must be even
    assert_eq!(layout.clip_w % 2, 0);
    assert_eq!(layout.clip_h % 2, 0);
    assert_eq!(layout.clip_x % 2, 0);
    assert_eq!(layout.clip_y % 2, 0);

    // Square source on widescreen canvas
    let layout2 = compute_canvas_layout(500, 500, 1920, 1080, 1920, 1080);
    assert_eq!(layout2.clip_h, 1080);
    assert_eq!(layout2.clip_w, 1080);
    // Centered horizontally: (1920 - 1080) / 2 = 420
    assert_eq!(layout2.clip_x, 420);
    assert_eq!(layout2.clip_y, 0);

    // Source matches canvas exactly
    let layout3 = compute_canvas_layout(1920, 1080, 1920, 1080, 1920, 1080);
    assert_eq!(layout3.clip_w, 1920);
    assert_eq!(layout3.clip_h, 1080);
    assert_eq!(layout3.clip_x, 0);
    assert_eq!(layout3.clip_y, 0);

    // Different render vs canvas aspect ratio (e.g. render 4:3 from 16:9 canvas)
    let layout4 = compute_canvas_layout(320, 240, 1920, 1080, 1280, 960);
    // Canvas 16:9 scaled to fit 1280x960 (4:3): width=1280, height=720, offset_y=120
    // Clip 4:3 in canvas: 1440x1080, centered at (240, 0)
    // Mapped to render: clip_w=1440*(1280/1920)=960, clip_h=1080*(1280/1920)=720
    // clip_x = 120 + 240*(1280/1920) = 120+160 = 280... let me verify with even rounding
    assert_eq!(layout4.clip_w % 2, 0);
    assert_eq!(layout4.clip_h % 2, 0);
    assert_eq!(layout4.clip_x % 2, 0);
    assert_eq!(layout4.clip_y % 2, 0);
    // Clip should not exceed render bounds
    assert!(layout4.clip_x + layout4.clip_w <= 1280);
    assert!(layout4.clip_y + layout4.clip_h <= 960);
}

#[test]
fn test_render_canvas_composition() {
    // 500x500 source on 1920x1080 canvas, rendered at 1920x1080
    // The clip should be letterboxed (pillarboxed) within the output
    let dir = fixtures::fixture_dir();
    let video_path =
        fixtures::generate_test_video_with_size(dir.path(), "canvas_compose", 2.0, 500, 500);
    let output_path = dir.path().join("output_canvas_compose.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    assert_eq!(asset.width, 500);
    assert_eq!(asset.height, 500);

    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 1920,
        height: 1080,
        canvas_width: 1920,
        canvas_height: 1080,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    assert_eq!(output_asset.width, 1920);
    assert_eq!(output_asset.height, 1080);
    let dur = output_asset.duration.as_secs_f64();
    assert!(
        dur >= 1.5 && dur <= 3.0,
        "Expected ~2s duration, got {dur}s"
    );
}

#[test]
fn test_render_canvas_downscale() {
    // 320x240 source on 1920x1080 canvas, rendered at 1280x720
    // Output should be 1280x720 with the clip properly scaled
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "canvas_downscale", 2.0);
    let output_path = dir.path().join("output_canvas_downscale.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    assert_eq!(asset.width, 320);
    assert_eq!(asset.height, 240);

    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 1280,
        height: 720,
        canvas_width: 1920,
        canvas_height: 1080,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    assert_eq!(output_asset.width, 1280);
    assert_eq!(output_asset.height, 720);
    let dur = output_asset.duration.as_secs_f64();
    assert!(
        dur >= 1.5 && dur <= 3.0,
        "Expected ~2s duration, got {dur}s"
    );
}

#[test]
fn test_render_source_matches_canvas() {
    // 320x240 source on 320x240 canvas, rendered at 320x240
    // Clip should fill the entire frame (no borders)
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "canvas_match", 2.0);
    let output_path = dir.path().join("output_canvas_match.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    assert_eq!(asset.width, 320);
    assert_eq!(asset.height, 240);

    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    assert_eq!(output_asset.width, 320);
    assert_eq!(output_asset.height, 240);

    // Verify layout: clip should fill entire frame
    let layout = compute_canvas_layout(320, 240, 320, 240, 320, 240);
    assert_eq!(layout.clip_x, 0);
    assert_eq!(layout.clip_y, 0);
    assert_eq!(layout.clip_w, 320);
    assert_eq!(layout.clip_h, 240);
}

// =============================================================================
// Brief 15: Track layering tests
// =============================================================================

/// Test rendering with overlapping video clips on two tracks.
/// V1 (bottom) and V2 (top) both have clips at the same timeline position.
#[test]
fn test_render_overlapping_video_two_tracks() {
    let dir = fixtures::fixture_dir();
    let video_path1 = fixtures::generate_test_video(dir.path(), "overlap_v1", 2.0);
    let video_path2 = fixtures::generate_test_video(dir.path(), "overlap_v2", 2.0);
    let output_path = dir.path().join("output_overlap_video.mkv");

    let asset1 = zeditor_media::probe::probe(&video_path1).unwrap();
    let asset2 = zeditor_media::probe::probe(&video_path2).unwrap();

    let mut timeline = Timeline::new();
    // Add two video tracks: V2 (top, index 0) then V1 (bottom, index 1)
    let v2_idx = timeline.add_track("V2", zeditor_core::timeline::TrackType::Video);
    let v1_idx = timeline.add_track("V1", zeditor_core::timeline::TrackType::Video);

    let source_range1 = TimeRange {
        start: TimelinePosition::zero(),
        end: TimelinePosition::from_secs_f64(asset1.duration.as_secs_f64()),
    };
    let source_range2 = TimeRange {
        start: TimelinePosition::zero(),
        end: TimelinePosition::from_secs_f64(asset2.duration.as_secs_f64()),
    };

    // V1 (bottom) clip
    let clip1 = Clip::new(asset1.id, TimelinePosition::zero(), source_range1);
    timeline.add_clip_trimming_overlaps(v1_idx, clip1).unwrap();

    // V2 (top) clip — same position, overlapping
    let clip2 = Clip::new(asset2.id, TimelinePosition::zero(), source_range2);
    timeline.add_clip_trimming_overlaps(v2_idx, clip2).unwrap();

    let mut source_library = SourceLibrary::new();
    source_library.import(asset1.clone());
    source_library.import(asset2.clone());

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    assert!(output_path.exists(), "Output file should exist");
    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.len() > 0, "Output file should be non-empty");

    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    assert_eq!(output_asset.width, 320);
    assert_eq!(output_asset.height, 240);
    let dur = output_asset.duration.as_secs_f64();
    assert!(
        dur >= 1.5 && dur <= 3.0,
        "Expected ~2s duration, got {dur}s"
    );
}

/// Test rendering with overlapping audio clips on two tracks.
/// Both A1 and A2 have clips at the same timeline position — they should mix.
#[test]
fn test_render_overlapping_audio_two_tracks() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "overlap_audio", 2.0);
    let output_path = dir.path().join("output_overlap_audio.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();

    let mut timeline = Timeline::new();
    let v1_idx = timeline.add_track("V1", zeditor_core::timeline::TrackType::Video);
    let a1_idx = timeline.add_track("A1", zeditor_core::timeline::TrackType::Audio);
    let a2_idx = timeline.add_track("A2", zeditor_core::timeline::TrackType::Audio);

    let source_range = TimeRange {
        start: TimelinePosition::zero(),
        end: TimelinePosition::from_secs_f64(asset.duration.as_secs_f64()),
    };

    // Video on V1
    let video_clip = Clip::new(asset.id, TimelinePosition::zero(), source_range);
    timeline.add_clip_trimming_overlaps(v1_idx, video_clip).unwrap();

    // Audio on both A1 and A2 (same asset, same time — should mix)
    let audio_clip1 = Clip::new(asset.id, TimelinePosition::zero(), source_range);
    timeline.add_clip_trimming_overlaps(a1_idx, audio_clip1).unwrap();

    let audio_clip2 = Clip::new(asset.id, TimelinePosition::zero(), source_range);
    timeline.add_clip_trimming_overlaps(a2_idx, audio_clip2).unwrap();

    let mut source_library = SourceLibrary::new();
    source_library.import(asset.clone());

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::Lanczos,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    assert!(output_path.exists(), "Output file should exist");
    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.len() > 0, "Output file should be non-empty");
}

/// Test that write_samples_to_buffer uses additive mixing with clamping.
#[test]
fn test_audio_mixing_additive() {
    use zeditor_media::renderer::write_samples_to_buffer;

    // Pre-fill buffer with some values
    let mut buffer = vec![0.5f32, 0.3, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0];
    let samples = vec![0.3f32, 0.8, -0.6, 0.1];

    let written = write_samples_to_buffer(&samples, &mut buffer, 0, 0, 8);
    assert_eq!(written, 4);

    // Check additive mixing: 0.5+0.3=0.8, 0.3+0.8=1.0(clamped), -0.5+-0.6=-1.0(clamped), 0.0+0.1=0.1
    assert!((buffer[0] - 0.8).abs() < 0.001, "Expected 0.8, got {}", buffer[0]);
    assert!((buffer[1] - 1.0).abs() < 0.001, "Expected 1.0 (clamped), got {}", buffer[1]);
    assert!((buffer[2] - (-1.0)).abs() < 0.001, "Expected -1.0 (clamped), got {}", buffer[2]);
    assert!((buffer[3] - 0.1).abs() < 0.001, "Expected 0.1, got {}", buffer[3]);

    // Remaining buffer should be untouched
    assert_eq!(buffer[4], 0.0);
}

// =============================================================================
// Brief 16: Pixel effect pipeline integration tests
// =============================================================================

/// Helper: create a timeline with one clip that has effects applied.
fn single_clip_timeline_with_effects(
    asset: &MediaAsset,
    effects: Vec<EffectInstance>,
) -> (Timeline, SourceLibrary) {
    let mut timeline = Timeline::new();
    let video_track_idx = timeline.add_track("Video 1", zeditor_core::timeline::TrackType::Video);
    let source_range = TimeRange {
        start: TimelinePosition::zero(),
        end: TimelinePosition::from_secs_f64(asset.duration.as_secs_f64()),
    };
    let mut clip = Clip::new(asset.id, TimelinePosition::zero(), source_range);
    clip.effects = effects;
    timeline
        .add_clip_trimming_overlaps(video_track_idx, clip)
        .unwrap();

    let mut source_library = SourceLibrary::new();
    source_library.import(asset.clone());

    (timeline, source_library)
}

#[test]
fn test_render_with_grayscale_effect() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "grayscale_fx", 1.0);
    let output_path = dir.path().join("output_grayscale.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();

    let effects = vec![EffectInstance::new(EffectType::Grayscale)];
    let (timeline, source_library) = single_clip_timeline_with_effects(&asset, effects);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::FastBilinear,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    // Verify output exists
    assert!(output_path.exists(), "Grayscale render output should exist");
    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.len() > 0, "Output file should be non-empty");

    // Decode the output and verify pixels are grayscale (R ≈ G ≈ B)
    use zeditor_media::decoder::{FfmpegDecoder, VideoDecoder};
    let mut decoder = FfmpegDecoder::open(&output_path).unwrap();
    let frame = decoder.decode_next_frame_rgba_scaled(320, 240).unwrap().unwrap();

    // Check a sample of pixels - they should be grayscale (R ≈ G ≈ B within tolerance)
    // YUV→RGB conversion can introduce slight rounding, so allow tolerance
    let mut grayscale_count = 0;
    let total_pixels = (frame.width * frame.height) as usize;
    for i in 0..total_pixels {
        let r = frame.data[i * 4] as i32;
        let g = frame.data[i * 4 + 1] as i32;
        let b = frame.data[i * 4 + 2] as i32;
        if (r - g).unsigned_abs() <= 10 && (g - b).unsigned_abs() <= 10 && (r - b).unsigned_abs() <= 10 {
            grayscale_count += 1;
        }
    }
    let grayscale_ratio = grayscale_count as f64 / total_pixels as f64;
    assert!(
        grayscale_ratio > 0.8,
        "Expected mostly grayscale pixels, got {:.1}% grayscale",
        grayscale_ratio * 100.0
    );
}

#[test]
fn test_render_with_brightness_effect() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "brightness_fx", 1.0);

    // Render without effects
    let no_fx_output = dir.path().join("output_no_fx.mkv");
    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    let (timeline_no_fx, source_library_no_fx) = single_clip_timeline(&asset, false);

    let config_no_fx = RenderConfig {
        output_path: no_fx_output.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::FastBilinear,
    };
    render_timeline(&timeline_no_fx, &source_library_no_fx, &config_no_fx, None).unwrap();

    // Render with brightness = 0.5
    let fx_output = dir.path().join("output_bright.mkv");
    let mut brightness = EffectInstance::new(EffectType::Brightness);
    brightness.set_float("brightness", 0.5);
    let (timeline_fx, source_library_fx) = single_clip_timeline_with_effects(&asset, vec![brightness]);

    let config_fx = RenderConfig {
        output_path: fx_output.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::FastBilinear,
    };
    render_timeline(&timeline_fx, &source_library_fx, &config_fx, None).unwrap();

    // Decode both and compare - bright version should have higher average luminance
    use zeditor_media::decoder::{FfmpegDecoder, VideoDecoder};

    let mut dec_no_fx = FfmpegDecoder::open(&no_fx_output).unwrap();
    let frame_no_fx = dec_no_fx.decode_next_frame_rgba_scaled(320, 240).unwrap().unwrap();

    let mut dec_fx = FfmpegDecoder::open(&fx_output).unwrap();
    let frame_fx = dec_fx.decode_next_frame_rgba_scaled(320, 240).unwrap().unwrap();

    // Compute average luminance for both
    let avg_lum = |data: &[u8], total: usize| -> f64 {
        let mut sum = 0u64;
        for i in 0..total {
            sum += data[i * 4] as u64 + data[i * 4 + 1] as u64 + data[i * 4 + 2] as u64;
        }
        sum as f64 / (total as f64 * 3.0)
    };

    let total = (frame_no_fx.width * frame_no_fx.height) as usize;
    let lum_no_fx = avg_lum(&frame_no_fx.data, total);
    let lum_fx = avg_lum(&frame_fx.data, total);

    assert!(
        lum_fx > lum_no_fx + 10.0,
        "Brightness effect should make frame brighter: no_fx={lum_no_fx:.1}, fx={lum_fx:.1}"
    );
}

#[test]
fn test_render_with_transform_effect() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "transform_fx", 1.0);
    let output_path = dir.path().join("output_transform.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();

    // Apply a large X offset to shift the clip right
    let mut transform = EffectInstance::new(EffectType::Transform);
    transform.set_float("x_offset", 100.0);
    let (timeline, source_library) = single_clip_timeline_with_effects(&asset, vec![transform]);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::FastBilinear,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    assert!(output_path.exists(), "Transform render output should exist");
    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.len() > 0, "Output file should be non-empty");

    // Decode and verify left side is darker (shifted clip leaves black area)
    use zeditor_media::decoder::{FfmpegDecoder, VideoDecoder};
    let mut decoder = FfmpegDecoder::open(&output_path).unwrap();
    let frame = decoder.decode_next_frame_rgba_scaled(320, 240).unwrap().unwrap();

    // Left 50 pixels should be mostly black (transparent area from transform)
    let mut left_sum = 0u64;
    let mut right_sum = 0u64;
    for y in 0..frame.height {
        for x in 0..50u32 {
            let idx = ((y * frame.width + x) * 4) as usize;
            left_sum += frame.data[idx] as u64 + frame.data[idx + 1] as u64 + frame.data[idx + 2] as u64;
        }
        for x in 150..frame.width {
            let idx = ((y * frame.width + x) * 4) as usize;
            right_sum += frame.data[idx] as u64 + frame.data[idx + 1] as u64 + frame.data[idx + 2] as u64;
        }
    }
    let left_avg = left_sum as f64 / (50.0 * frame.height as f64 * 3.0);
    let right_avg = right_sum as f64 / ((frame.width as f64 - 150.0) * frame.height as f64 * 3.0);

    assert!(
        left_avg < right_avg,
        "Left side (offset area) should be darker than right: left={left_avg:.1}, right={right_avg:.1}"
    );
}

#[test]
fn test_render_no_effects_fast_path() {
    // Verify that rendering without effects still works (fast path)
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "no_fx_fast", 1.0);
    let output_path = dir.path().join("output_no_fx_fast.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::FastBilinear,
    };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    assert!(output_path.exists());
    let output_asset = zeditor_media::probe::probe(&output_path).unwrap();
    assert_eq!(output_asset.width, 320);
    assert_eq!(output_asset.height, 240);
}

// =============================================================================
// Brief 17: Render profiling integration tests
// =============================================================================

#[test]
fn test_render_with_profiling_creates_json() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "profile_json", 1.0);
    let output_path = dir.path().join("output_profile.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::FastBilinear,
    };

    // Enable profiling, clear custom dir
    unsafe { std::env::set_var("ZEDITOR_PROFILE", "1") };
    unsafe { std::env::remove_var("ZEDITOR_PROFILE_DIR") };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    unsafe { std::env::remove_var("ZEDITOR_PROFILE") };

    // Verify profile JSON exists
    let profile_path = output_path.with_file_name("output_profile.mkv.profile.json");
    assert!(
        profile_path.exists(),
        "Profile JSON should exist at {}",
        profile_path.display()
    );

    // Verify it deserializes correctly
    let content = std::fs::read_to_string(&profile_path).unwrap();
    let profile: zeditor_media::render_profile::RenderProfile =
        serde_json::from_str(&content).unwrap();

    assert!(profile.total_frames > 0, "Should have recorded frames");
    assert!(profile.avg_frame_ms > 0.0, "Avg frame time should be > 0");
    assert!(
        profile.stages.video_encode_ms > 0.0,
        "Video encode stage should have time"
    );
    assert_eq!(profile.config.width, 320);
    assert_eq!(profile.config.height, 240);
    // Each frame should have metrics
    assert_eq!(profile.frames.len(), profile.total_frames as usize);
}

#[test]
fn test_render_without_profiling_no_json() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "no_profile", 1.0);
    let output_path = dir.path().join("output_no_profile.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::FastBilinear,
    };

    // Ensure profiling is disabled
    unsafe { std::env::remove_var("ZEDITOR_PROFILE") };

    render_timeline(&timeline, &source_library, &config, None).unwrap();

    // No profile JSON should be created
    let profile_path = output_path.with_file_name("output_no_profile.mkv.profile.json");
    assert!(
        !profile_path.exists(),
        "Profile JSON should NOT exist when profiling is disabled"
    );
}

#[test]
fn test_render_progress_channel_receives_updates() {
    let dir = fixtures::fixture_dir();
    let video_path = fixtures::generate_test_video(dir.path(), "progress_chan", 1.0);
    let output_path = dir.path().join("output_progress.mkv");

    let asset = zeditor_media::probe::probe(&video_path).unwrap();
    let (timeline, source_library) = single_clip_timeline(&asset, false);

    let config = RenderConfig {
        output_path: output_path.clone(),
        width: 320,
        height: 240,
        canvas_width: 320,
        canvas_height: 240,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
        scaling: ScalingAlgorithm::FastBilinear,
    };

    // Ensure profiling is off for this test
    unsafe { std::env::remove_var("ZEDITOR_PROFILE") };

    let (tx, rx) = std::sync::mpsc::channel();
    render_timeline(&timeline, &source_library, &config, Some(tx)).unwrap();

    // Collect all progress messages
    let mut messages: Vec<zeditor_media::render_profile::RenderProgress> = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        messages.push(msg);
    }

    // Should have received at least a few progress updates
    assert!(
        messages.len() >= 2,
        "Expected at least 2 progress messages, got {}",
        messages.len()
    );

    // Frame counts should be monotonically non-decreasing
    let mut prev_frame = 0u64;
    for msg in &messages {
        assert!(
            msg.current_frame >= prev_frame,
            "Frames should be monotonically non-decreasing: {} < {}",
            msg.current_frame,
            prev_frame
        );
        prev_frame = msg.current_frame;
    }

    // Last message should be Complete stage
    let last = messages.last().unwrap();
    assert_eq!(
        last.stage,
        zeditor_media::render_profile::RenderStage::Complete,
        "Last progress message should be Complete"
    );
}
