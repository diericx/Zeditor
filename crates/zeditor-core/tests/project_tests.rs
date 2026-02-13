use std::time::Duration;

use zeditor_core::error::CoreError;
use zeditor_core::media::MediaAsset;
use zeditor_core::project::{Project, CURRENT_PROJECT_VERSION};
use zeditor_core::timeline::*;

#[test]
fn test_new_project_has_default_tracks() {
    let project = Project::new("Test");
    assert_eq!(project.timeline.tracks.len(), 2);
    assert_eq!(project.timeline.tracks[0].name, "Video 1");
    assert_eq!(project.timeline.tracks[1].name, "Audio 1");
}

#[test]
fn test_project_save_load() {
    let mut project = Project::new("Test Project");

    let asset = MediaAsset::new(
        "test.mp4".into(),
        "/tmp/test.mp4".into(),
        Duration::from_secs(10),
        1920,
        1080,
        30.0,
        true,
    );
    let asset_id = asset.id;
    project.source_library.import(asset);

    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    )
    .unwrap();
    let clip = Clip::new(asset_id, TimelinePosition::zero(), source_range);
    project.timeline.add_clip(0, clip).unwrap();

    // Save.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_project.zpf");
    project.save(&path).unwrap();

    // Load.
    let loaded = Project::load(&path).unwrap();
    assert_eq!(loaded.name, "Test Project");
    assert_eq!(loaded.source_library.len(), 1);
    assert_eq!(loaded.timeline.tracks[0].clips.len(), 1);

    // Verify asset roundtrips.
    let loaded_asset = loaded.source_library.get(asset_id).unwrap();
    assert_eq!(loaded_asset.name, "test.mp4");
    assert_eq!(loaded_asset.width, 1920);
}

#[test]
fn test_source_library_operations() {
    let mut project = Project::new("Test");

    assert!(project.source_library.is_empty());

    let asset = MediaAsset::new(
        "clip1.mp4".into(),
        "/tmp/clip1.mp4".into(),
        Duration::from_secs(5),
        1280,
        720,
        24.0,
        false,
    );
    let id = asset.id;
    project.source_library.import(asset);

    assert_eq!(project.source_library.len(), 1);
    assert!(!project.source_library.is_empty());
    assert!(project.source_library.get(id).is_some());

    let removed = project.source_library.remove(id).unwrap();
    assert_eq!(removed.name, "clip1.mp4");
    assert!(project.source_library.is_empty());
}

#[test]
fn test_project_file_contains_version() {
    let project = Project::new("Versioned");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("versioned.zpf");
    project.save(&path).unwrap();

    // Read raw JSON and check version field
    let raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["version"].as_str().unwrap(), CURRENT_PROJECT_VERSION);
    assert_eq!(raw["version"].as_str().unwrap(), "1.0.0");
}

#[test]
fn test_project_file_version_too_new() {
    // Write a file with version "99.0.0"
    let project = Project::new("Future");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("future.zpf");
    project.save(&path).unwrap();

    // Patch the version in the JSON
    let mut raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    raw["version"] = serde_json::Value::String("99.0.0".into());
    std::fs::write(&path, serde_json::to_string(&raw).unwrap()).unwrap();

    let err = Project::load(&path).unwrap_err();
    match &err {
        CoreError::VersionTooNew { got, max } => {
            assert_eq!(got, "99.0.0");
            assert_eq!(max, CURRENT_PROJECT_VERSION);
        }
        other => panic!("expected VersionTooNew, got: {other}"),
    }
}

#[test]
fn test_project_file_version_too_old() {
    let project = Project::new("Old");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.zpf");
    project.save(&path).unwrap();

    // Patch the version to something below MIN
    let mut raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    raw["version"] = serde_json::Value::String("0.1.0".into());
    std::fs::write(&path, serde_json::to_string(&raw).unwrap()).unwrap();

    let err = Project::load(&path).unwrap_err();
    match &err {
        CoreError::VersionTooOld { got, min } => {
            assert_eq!(got, "0.1.0");
            assert_eq!(min, "1.0.0");
        }
        other => panic!("expected VersionTooOld, got: {other}"),
    }
}

#[test]
fn test_project_file_missing_version() {
    // Write a bare Project JSON without the ProjectFile wrapper
    let project = Project::new("Bare");
    let json = serde_json::to_string_pretty(&project).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bare.zpf");
    std::fs::write(&path, json).unwrap();

    let err = Project::load(&path).unwrap_err();
    match &err {
        CoreError::InvalidProjectFile(msg) => {
            assert!(msg.contains("version"), "error should mention version: {msg}");
        }
        other => panic!("expected InvalidProjectFile, got: {other}"),
    }
}

#[test]
fn test_project_file_roundtrip_full_data() {
    // Create a fully populated project
    let mut project = Project::new("Roundtrip Test");

    let asset1 = MediaAsset::new(
        "intro.mp4".into(),
        "/media/intro.mp4".into(),
        Duration::from_secs(10),
        1920,
        1080,
        30.0,
        true,
    );
    let asset1_id = asset1.id;
    project.source_library.import(asset1);

    let asset2 = MediaAsset::new(
        "main.mp4".into(),
        "/media/main.mp4".into(),
        Duration::from_secs(60),
        3840,
        2160,
        60.0,
        false,
    );
    let asset2_id = asset2.id;
    project.source_library.import(asset2);

    // Add clips to video track
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    )
    .unwrap();
    let clip1 = Clip::new(asset1_id, TimelinePosition::zero(), source_range);
    project.timeline.add_clip(0, clip1).unwrap();

    let source_range2 = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(10.0),
    )
    .unwrap();
    let clip2 = Clip::new(asset2_id, TimelinePosition::from_secs_f64(5.0), source_range2);
    project.timeline.add_clip(0, clip2).unwrap();

    // Add a clip to audio track
    let audio_source = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(5.0),
    )
    .unwrap();
    let audio_clip = Clip::new(asset1_id, TimelinePosition::zero(), audio_source);
    project.timeline.add_clip(1, audio_clip).unwrap();

    // Save and reload
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("roundtrip.zpf");
    project.save(&path).unwrap();
    let loaded = Project::load(&path).unwrap();

    // Full equality check (catches any future #[serde(skip)] additions)
    assert_eq!(loaded, project, "loaded project should equal original");
}
