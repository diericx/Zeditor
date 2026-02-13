#![allow(unused_must_use)]

use std::path::PathBuf;
use std::time::Duration;

use zeditor_core::media::MediaAsset;
use zeditor_core::timeline::TimelinePosition;
use zeditor_ui::app::App;
use zeditor_ui::message::{Message, ToolMode};

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
    assert!(app.playback_start_wall.is_some());

    app.update(Message::Pause);
    assert!(!app.is_playing);
    assert!(app.playback_start_wall.is_none());
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

    // Use no_audio asset to test simple move without linked audio clip
    let asset = MediaAsset::new(
        "clip1".into(),
        PathBuf::from("/test/clip1.mp4"),
        Duration::from_secs_f64(5.0),
        1920, 1080, 30.0, false,
    );
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
fn test_select_source_asset() {
    let mut app = App::new();
    assert!(app.selected_asset_id.is_none());

    let id = uuid::Uuid::new_v4();
    app.update(Message::SelectSourceAsset(Some(id)));
    assert_eq!(app.selected_asset_id, Some(id));

    app.update(Message::SelectSourceAsset(None));
    assert!(app.selected_asset_id.is_none());
}

#[test]
fn test_open_file_dialog_returns_task() {
    let mut app = App::new();
    let task = app.update(Message::OpenFileDialog);
    // OpenFileDialog returns a Task::perform, which is non-trivial
    // We can't easily inspect it, but verify the status was updated
    assert_eq!(app.status_message, "Opening file dialog...");
    let _ = task; // just ensure it compiles and doesn't panic
}

#[test]
fn test_toggle_playback() {
    let mut app = App::new();
    assert!(!app.is_playing);

    app.update(Message::TogglePlayback);
    assert!(app.is_playing);

    app.update(Message::TogglePlayback);
    assert!(!app.is_playing);
}

#[test]
fn test_playback_tick_advances_position() {
    let mut app = App::new();

    // Add content so playback doesn't immediately stop
    let asset = make_test_asset("clip1", 30.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    app.update(Message::Play);
    assert!(app.is_playing);

    // Simulate a small passage of time by adjusting the start wall
    let start = app.playback_start_wall.unwrap();
    app.playback_start_wall = Some(start - Duration::from_millis(100));

    app.update(Message::PlaybackTick);
    assert!(app.playback_position.as_secs_f64() > 0.0);
}

#[test]
fn test_playback_continues_past_end() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 1.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    app.update(Message::Play);

    // Set start wall far in the past so elapsed > timeline duration
    let start = app.playback_start_wall.unwrap();
    app.playback_start_wall = Some(start - Duration::from_secs(5));

    app.update(Message::PlaybackTick);
    // Playback should continue past the end — user stops manually
    assert!(app.is_playing);
    assert!(app.playback_position.as_secs_f64() > 1.0);
}

#[test]
fn test_spacebar_toggles() {
    let mut app = App::new();
    assert!(!app.is_playing);

    app.update(Message::KeyboardEvent(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Space),
        modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Space),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    }));
    assert!(app.is_playing);
}

#[test]
fn test_timeline_click_empty_moves_cursor() {
    let mut app = App::new();
    assert_eq!(app.playback_position, TimelinePosition::zero());

    app.update(Message::TimelineClickEmpty(
        TimelinePosition::from_secs_f64(3.5),
    ));
    assert_eq!(
        app.playback_position,
        TimelinePosition::from_secs_f64(3.5)
    );
}

#[test]
fn test_place_selected_clip() {
    let mut app = App::new();
    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.selected_asset_id = Some(asset_id);

    app.update(Message::PlaceSelectedClip {
        asset_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(2.0),
    });

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    assert!(app.selected_asset_id.is_none());
    assert_eq!(app.status_message, "Clip placed");
}

#[test]
fn test_place_clears_selection() {
    let mut app = App::new();
    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.selected_asset_id = Some(asset_id);

    app.update(Message::PlaceSelectedClip {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    assert!(app.selected_asset_id.is_none());
}

#[test]
fn test_click_without_selection_moves_cursor() {
    let mut app = App::new();
    assert!(app.selected_asset_id.is_none());

    app.update(Message::TimelineClickEmpty(
        TimelinePosition::from_secs_f64(5.0),
    ));
    assert_eq!(
        app.playback_position,
        TimelinePosition::from_secs_f64(5.0)
    );
}

#[test]
fn test_rgb24_to_rgba32() {
    let rgb = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 128, 128, 128];
    let rgba = zeditor_ui::app::rgb24_to_rgba32(&rgb, 2, 2);
    assert_eq!(rgba.len(), 16);
    assert_eq!(&rgba[0..4], &[255, 0, 0, 255]); // red pixel
    assert_eq!(&rgba[4..8], &[0, 255, 0, 255]); // green pixel
    assert_eq!(&rgba[8..12], &[0, 0, 255, 255]); // blue pixel
    assert_eq!(&rgba[12..16], &[128, 128, 128, 255]); // gray pixel
}

#[test]
fn test_clip_at_position() {
    let mut app = App::new();
    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    // Position inside clip
    let result = app.clip_at_position(TimelinePosition::from_secs_f64(2.0));
    assert!(result.is_some());
    let (track_idx, clip) = result.unwrap();
    assert_eq!(track_idx, 0);
    assert_eq!(clip.asset_id, asset_id);

    // Position outside clip
    let result = app.clip_at_position(TimelinePosition::from_secs_f64(10.0));
    assert!(result.is_none());
}

#[test]
fn test_move_clip_with_snap() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    // Add first clip at 0s
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    // Add second clip at 10s
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(10.0),
    });

    let clip2_id = app.project.timeline.tracks[0].clips[1].id;

    // Move clip2 close to end of clip1 (5.1s, within snap threshold of 200ms)
    app.update(Message::MoveClip {
        source_track: 0,
        clip_id: clip2_id,
        dest_track: 0,
        position: TimelinePosition::from_secs_f64(5.1),
    });

    // Should snap to 5.0s (end of first clip)
    let clip2 = app.project.timeline.tracks[0]
        .get_clip(clip2_id)
        .unwrap();
    let start = clip2.timeline_range.start.as_secs_f64();
    assert!(
        (start - 5.0).abs() < 0.01,
        "Expected snap to 5.0s, got {start}"
    );
}

#[test]
fn test_click_during_playback_pauses_and_moves_cursor() {
    let mut app = App::new();

    // Add content so playback doesn't immediately stop
    let asset = make_test_asset("clip1", 30.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    // Start playing
    app.update(Message::Play);
    assert!(app.is_playing);
    assert!(app.playback_start_wall.is_some());

    // Click timeline at 15s while playing
    app.update(Message::TimelineClickEmpty(
        TimelinePosition::from_secs_f64(15.0),
    ));

    // Should be paused at 15s
    assert!(!app.is_playing, "clicking timeline during playback should pause");
    assert!(
        app.playback_start_wall.is_none(),
        "playback_start_wall should be cleared on pause"
    );
    assert_eq!(
        app.playback_position,
        TimelinePosition::from_secs_f64(15.0),
        "cursor should move to clicked position"
    );
}

#[test]
fn test_place_overlapping_clip_trims_previous() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    // Add first clip at 0s (5s long → [0, 5))
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);

    // Place second clip at 3s — overlaps [3, 8), should trim first to [0, 3)
    app.update(Message::PlaceSelectedClip {
        asset_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(3.0),
    });

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 2);
    let first = &app.project.timeline.tracks[0].clips[0];
    assert_eq!(first.timeline_range.start, TimelinePosition::zero());
    assert_eq!(
        first.timeline_range.end,
        TimelinePosition::from_secs_f64(3.0)
    );
    let second = &app.project.timeline.tracks[0].clips[1];
    assert_eq!(
        second.timeline_range.start,
        TimelinePosition::from_secs_f64(3.0)
    );
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

#[test]
fn test_tool_mode_defaults_to_arrow() {
    let app = App::new();
    assert_eq!(app.tool_mode, ToolMode::Arrow);
}

#[test]
fn test_a_key_sets_arrow_mode() {
    let mut app = App::new();
    // First switch to blade
    app.update(Message::KeyboardEvent(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Character("b".into()),
        modified_key: iced::keyboard::Key::Character("b".into()),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    }));
    assert_eq!(app.tool_mode, ToolMode::Blade);

    // Now press A to go back to arrow
    app.update(Message::KeyboardEvent(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Character("a".into()),
        modified_key: iced::keyboard::Key::Character("a".into()),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    }));
    assert_eq!(app.tool_mode, ToolMode::Arrow);
}

#[test]
fn test_b_key_sets_blade_mode() {
    let mut app = App::new();
    assert_eq!(app.tool_mode, ToolMode::Arrow);

    app.update(Message::KeyboardEvent(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Character("b".into()),
        modified_key: iced::keyboard::Key::Character("b".into()),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    }));
    assert_eq!(app.tool_mode, ToolMode::Blade);
}

// ===== Grouped operation tests =====

#[test]
fn test_add_clip_with_audio_creates_both() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0); // has_audio: true
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    // Video track should have 1 clip
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    // Audio track should also have 1 clip
    assert_eq!(app.project.timeline.tracks[1].clips.len(), 1);

    // They should share a link_id
    let vid_link = app.project.timeline.tracks[0].clips[0].link_id;
    let aud_link = app.project.timeline.tracks[1].clips[0].link_id;
    assert!(vid_link.is_some());
    assert_eq!(vid_link, aud_link);
}

#[test]
fn test_add_clip_without_audio_creates_only_video() {
    let mut app = App::new();

    let asset = MediaAsset::new(
        "clip1".into(),
        PathBuf::from("/test/clip1.mp4"),
        Duration::from_secs_f64(5.0),
        1920, 1080, 30.0, false,
    );
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    assert_eq!(app.project.timeline.tracks[1].clips.len(), 0);
    assert!(app.project.timeline.tracks[0].clips[0].link_id.is_none());
}

#[test]
fn test_cut_linked_pair_creates_four_clips() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 10.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    // Both tracks have 1 clip
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    assert_eq!(app.project.timeline.tracks[1].clips.len(), 1);

    // Cut at 4s
    app.update(Message::CutClip {
        track_index: 0,
        position: TimelinePosition::from_secs_f64(4.0),
    });

    // Both tracks should have 2 clips each (total 4)
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 2);
    assert_eq!(app.project.timeline.tracks[1].clips.len(), 2);
    assert_eq!(app.status_message, "Clip cut");
}

#[test]
fn test_move_linked_pair() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    let vid_id = app.project.timeline.tracks[0].clips[0].id;

    // Move video clip to 3s → audio should also move
    app.update(Message::MoveClip {
        source_track: 0,
        clip_id: vid_id,
        dest_track: 0,
        position: TimelinePosition::from_secs_f64(3.0),
    });

    let vid = app.project.timeline.tracks[0].get_clip(vid_id).unwrap();
    assert_eq!(vid.timeline_range.start, TimelinePosition::from_secs_f64(3.0));

    let aud = &app.project.timeline.tracks[1].clips[0];
    assert_eq!(aud.timeline_range.start, TimelinePosition::from_secs_f64(3.0));
}

#[test]
fn test_resize_linked_pair() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    let vid_id = app.project.timeline.tracks[0].clips[0].id;

    // Resize video clip to 8s → audio should also resize
    app.update(Message::ResizeClip {
        track_index: 0,
        clip_id: vid_id,
        new_end: TimelinePosition::from_secs_f64(8.0),
    });

    let vid = app.project.timeline.tracks[0].get_clip(vid_id).unwrap();
    assert_eq!(vid.timeline_range.end, TimelinePosition::from_secs_f64(8.0));

    let aud = &app.project.timeline.tracks[1].clips[0];
    assert_eq!(aud.timeline_range.end, TimelinePosition::from_secs_f64(8.0));
}

#[test]
fn test_place_selected_clip_with_audio() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.selected_asset_id = Some(asset_id);

    app.update(Message::PlaceSelectedClip {
        asset_id,
        track_index: 0,
        position: TimelinePosition::from_secs_f64(2.0),
    });

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    assert_eq!(app.project.timeline.tracks[1].clips.len(), 1);

    let vid = &app.project.timeline.tracks[0].clips[0];
    let aud = &app.project.timeline.tracks[1].clips[0];
    assert_eq!(vid.timeline_range.start, TimelinePosition::from_secs_f64(2.0));
    assert_eq!(aud.timeline_range.start, TimelinePosition::from_secs_f64(2.0));
    assert_eq!(vid.link_id, aud.link_id);
}

#[test]
fn test_undo_redo_grouped_add() {
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
    assert_eq!(app.project.timeline.tracks[1].clips.len(), 1);

    app.update(Message::Undo);
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 0);
    assert_eq!(app.project.timeline.tracks[1].clips.len(), 0);

    app.update(Message::Redo);
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    assert_eq!(app.project.timeline.tracks[1].clips.len(), 1);
}

#[test]
fn test_clip_at_position_returns_video_only() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    // clip_at_position should return only the video clip (track 0)
    let result = app.clip_at_position(TimelinePosition::from_secs_f64(2.0));
    assert!(result.is_some());
    let (track_idx, _) = result.unwrap();
    assert_eq!(track_idx, 0);

    // audio_clip_at_position should return the audio clip (track 1)
    let result = app.audio_clip_at_position(TimelinePosition::from_secs_f64(2.0));
    assert!(result.is_some());
    let (track_idx, _) = result.unwrap();
    assert_eq!(track_idx, 1);
}
