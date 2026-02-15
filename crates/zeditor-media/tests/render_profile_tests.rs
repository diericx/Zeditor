use std::path::PathBuf;
use std::time::Instant;

use zeditor_media::render_profile::{
    is_profiling_enabled, profile_output_path, write_profile, FrameMetrics, ProfileCollector,
    ProfileConfig, RenderProfile,
};

#[test]
fn test_profiling_disabled_produces_no_profile() {
    let collector = ProfileCollector::new(false);
    assert!(!collector.is_enabled());
    assert!(collector.finish().is_none());
}

#[test]
fn test_profiling_enabled_produces_profile() {
    let mut collector = ProfileCollector::new(true);
    assert!(collector.is_enabled());

    collector.set_config(ProfileConfig {
        output_path: "/tmp/test.mkv".to_string(),
        width: 1920,
        height: 1080,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
    });
    collector.set_render_start(Instant::now());

    // Record 5 frames with known total_ms values: 10, 20, 30, 40, 50
    for i in 0..5u64 {
        collector.record_frame(FrameMetrics {
            frame_index: i,
            timeline_time_secs: i as f64 / 30.0,
            total_ms: (i + 1) as f64 * 10.0, // 10, 20, 30, 40, 50
            find_clips_ms: 1.0,
            decode_ms: 5.0,
            effects_ms: 0.0,
            composite_ms: 1.0,
            color_convert_ms: 1.0,
            encode_ms: 2.0,
            clip_count: 1,
            used_effects_path: false,
        });
    }

    let profile = collector.finish().unwrap();
    assert_eq!(profile.total_frames, 5);
    // avg = (10+20+30+40+50)/5 = 30
    assert!((profile.avg_frame_ms - 30.0).abs() < 0.01);
    // sorted: [10, 20, 30, 40, 50], median = index 2 = 30
    assert!((profile.median_frame_ms - 30.0).abs() < 0.01);
    // p95 index = ceil(5*0.95) = 5, min(5-1)=4 → 50
    assert!((profile.p95_frame_ms - 50.0).abs() < 0.01);
    // max = 50
    assert!((profile.max_frame_ms - 50.0).abs() < 0.01);
    // slowest frame = index 4
    assert_eq!(profile.slowest_frame_index, 4);
}

#[test]
fn test_profile_output_path_default() {
    let render_path = PathBuf::from("/tmp/output.mkv");
    // Clear env var to test default behavior
    unsafe { std::env::remove_var("ZEDITOR_PROFILE_DIR") };
    let path = profile_output_path(&render_path);
    assert_eq!(path, PathBuf::from("/tmp/output.mkv.profile.json"));
}

#[test]
fn test_profile_output_path_with_env_override() {
    unsafe { std::env::set_var("ZEDITOR_PROFILE_DIR", "/tmp/profiles") };
    let render_path = PathBuf::from("/home/user/renders/output.mkv");
    let path = profile_output_path(&render_path);
    assert_eq!(path, PathBuf::from("/tmp/profiles/output.mkv.profile.json"));
    unsafe { std::env::remove_var("ZEDITOR_PROFILE_DIR") };
}

#[test]
fn test_profile_serialization_roundtrip() {
    let profile = RenderProfile {
        config: ProfileConfig {
            output_path: "/tmp/test.mkv".to_string(),
            width: 1920,
            height: 1080,
            fps: 30.0,
            crf: 22,
            preset: "superfast".to_string(),
        },
        stages: zeditor_media::render_profile::StageTimings {
            setup_ms: 100.0,
            video_encode_ms: 5000.0,
            audio_encode_ms: 200.0,
            flush_ms: 50.0,
            write_trailer_ms: 10.0,
        },
        frames: vec![
            FrameMetrics {
                frame_index: 0,
                timeline_time_secs: 0.0,
                total_ms: 25.0,
                find_clips_ms: 1.0,
                decode_ms: 10.0,
                effects_ms: 5.0,
                composite_ms: 3.0,
                color_convert_ms: 2.0,
                encode_ms: 4.0,
                clip_count: 1,
                used_effects_path: true,
            },
        ],
        total_frames: 1,
        total_duration_secs: 0.025,
        avg_frame_ms: 25.0,
        median_frame_ms: 25.0,
        p95_frame_ms: 25.0,
        max_frame_ms: 25.0,
        slowest_frame_index: 0,
    };

    let json = serde_json::to_string_pretty(&profile).unwrap();
    let deserialized: RenderProfile = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.total_frames, 1);
    assert!((deserialized.avg_frame_ms - 25.0).abs() < 0.01);
    assert_eq!(deserialized.config.width, 1920);
    assert_eq!(deserialized.stages.setup_ms, 100.0);
    assert_eq!(deserialized.frames.len(), 1);
    assert_eq!(deserialized.frames[0].frame_index, 0);
    assert!(deserialized.frames[0].used_effects_path);
}

#[test]
fn test_write_profile_to_file() {
    let dir = tempfile::tempdir().unwrap();
    let profile_path = dir.path().join("test.profile.json");

    let profile = RenderProfile {
        config: ProfileConfig {
            output_path: "/tmp/test.mkv".to_string(),
            width: 320,
            height: 240,
            fps: 30.0,
            crf: 22,
            preset: "superfast".to_string(),
        },
        stages: zeditor_media::render_profile::StageTimings::default(),
        frames: vec![],
        total_frames: 0,
        total_duration_secs: 0.0,
        avg_frame_ms: 0.0,
        median_frame_ms: 0.0,
        p95_frame_ms: 0.0,
        max_frame_ms: 0.0,
        slowest_frame_index: 0,
    };

    write_profile(&profile, &profile_path).unwrap();
    assert!(profile_path.exists());

    let content = std::fs::read_to_string(&profile_path).unwrap();
    let deserialized: RenderProfile = serde_json::from_str(&content).unwrap();
    assert_eq!(deserialized.config.width, 320);
}

#[test]
fn test_is_profiling_enabled_default_off() {
    unsafe { std::env::remove_var("ZEDITOR_PROFILE") };
    assert!(!is_profiling_enabled());
}

#[test]
fn test_is_profiling_enabled_with_env() {
    unsafe { std::env::set_var("ZEDITOR_PROFILE", "1") };
    assert!(is_profiling_enabled());
    unsafe { std::env::set_var("ZEDITOR_PROFILE", "true") };
    assert!(is_profiling_enabled());
    unsafe { std::env::set_var("ZEDITOR_PROFILE", "TRUE") };
    assert!(is_profiling_enabled());
    unsafe { std::env::set_var("ZEDITOR_PROFILE", "0") };
    assert!(!is_profiling_enabled());
    unsafe { std::env::remove_var("ZEDITOR_PROFILE") };
}

#[test]
fn test_disabled_collector_ignores_operations() {
    let mut collector = ProfileCollector::new(false);
    collector.set_config(ProfileConfig {
        output_path: "/tmp/test.mkv".to_string(),
        width: 1920,
        height: 1080,
        fps: 30.0,
        crf: 22,
        preset: "superfast".to_string(),
    });
    collector.set_render_start(Instant::now());
    collector.record_frame(FrameMetrics {
        frame_index: 0,
        timeline_time_secs: 0.0,
        total_ms: 25.0,
        find_clips_ms: 1.0,
        decode_ms: 10.0,
        effects_ms: 5.0,
        composite_ms: 3.0,
        color_convert_ms: 2.0,
        encode_ms: 4.0,
        clip_count: 1,
        used_effects_path: true,
    });
    // Should still return None since disabled
    assert!(collector.finish().is_none());
}
