use std::time::Duration;

use zeditor_core::media::MediaAsset;
use zeditor_core::project::Project;
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
    let path = dir.path().join("test_project.json");
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
