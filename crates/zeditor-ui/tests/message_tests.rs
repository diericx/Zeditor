#![allow(unused_must_use)]

use std::path::PathBuf;
use std::time::Duration;

use zeditor_core::media::MediaAsset;
use zeditor_core::timeline::TimelinePosition;
use zeditor_ui::app::App;
use zeditor_ui::message::Message;

fn make_test_asset(name: &str, duration_secs: f64) -> MediaAsset {
    MediaAsset::new(
        name.into(),
        PathBuf::from(format!("/test/{name}.mp4")),
        Duration::from_secs_f64(duration_secs),
        1920,
        1080,
        30.0,
        true,
    )
}

#[test]
fn test_import_media_flow() {
    let mut app = App::new();
    assert_eq!(app.project.source_library.len(), 0);

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;

    app.update(Message::MediaImported(Ok(asset)));

    assert_eq!(app.project.source_library.len(), 1);
    assert!(app.project.source_library.get(asset_id).is_some());
    assert!(app.status_message.contains("Imported"));
}

#[test]
fn test_import_media_error() {
    let mut app = App::new();

    app.update(Message::MediaImported(Err("file not found".into())));

    assert_eq!(app.project.source_library.len(), 0);
    assert!(app.status_message.contains("Import failed"));
}

#[test]
fn test_add_clip_to_timeline() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    assert_eq!(app.status_message, "Clip added");
}

#[test]
fn test_add_clip_nonexistent_asset() {
    let mut app = App::new();

    app.update(Message::AddClipToTimeline {
        asset_id: uuid::Uuid::new_v4(),
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 0);
    assert_eq!(app.status_message, "Asset not found");
}

#[test]
fn test_cut_clip_flow() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 10.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    app.update(Message::CutClip {
        track_index: 0,
        position: TimelinePosition::from_secs_f64(4.0),
    });

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 2);
    assert_eq!(app.status_message, "Clip cut");
}

#[test]
fn test_undo_redo_flow() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);

    app.update(Message::Undo);
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 0);
    assert_eq!(app.status_message, "Undone");

    app.update(Message::Redo);
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    assert_eq!(app.status_message, "Redone");
}

#[test]
fn test_play_pause() {
    let mut app = App::new();
    assert!(!app.is_playing);

    app.update(Message::Play);
    assert!(app.is_playing);

    app.update(Message::Pause);
    assert!(!app.is_playing);
}

#[test]
fn test_seek() {
    let mut app = App::new();
    assert_eq!(app.playback_position, TimelinePosition::zero());

    app.update(Message::SeekTo(TimelinePosition::from_secs_f64(5.0)));
    assert_eq!(
        app.playback_position,
        TimelinePosition::from_secs_f64(5.0)
    );
}

#[test]
fn test_remove_asset() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    assert_eq!(app.project.source_library.len(), 1);

    app.update(Message::RemoveAsset(asset_id));
    assert_eq!(app.project.source_library.len(), 0);
    assert!(app.status_message.contains("Removed"));
}

#[test]
fn test_move_clip_flow() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    let clip_id = app.project.timeline.tracks[0].clips[0].id;

    app.update(Message::MoveClip {
        source_track: 0,
        clip_id,
        dest_track: 1,
        position: TimelinePosition::from_secs_f64(2.0),
    });

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 0);
    assert_eq!(app.project.timeline.tracks[1].clips.len(), 1);
}

#[test]
fn test_full_editing_workflow() {
    let mut app = App::new();

    // Import two assets.
    let asset1 = make_test_asset("intro", 5.0);
    let asset1_id = asset1.id;
    let asset2 = make_test_asset("main", 10.0);
    let asset2_id = asset2.id;

    app.update(Message::MediaImported(Ok(asset1)));
    app.update(Message::MediaImported(Ok(asset2)));
    assert_eq!(app.project.source_library.len(), 2);

    // Add both to timeline.
    app.update(Message::AddClipToTimeline {
        asset_id: asset1_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });
    app.update(Message::AddClipToTimeline {
        asset_id: asset2_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(5.0),
    });
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 2);

    // Cut the second clip at 8s.
    app.update(Message::CutClip {
        track_index: 0,
        position: TimelinePosition::from_secs_f64(8.0),
    });
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 3);

    // Undo the cut.
    app.update(Message::Undo);
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 2);

    // Redo the cut.
    app.update(Message::Redo);
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 3);
}
