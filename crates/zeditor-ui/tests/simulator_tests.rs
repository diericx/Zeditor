#![allow(unused_must_use)]

use std::path::PathBuf;
use std::time::Duration;

use iced_test::simulator;
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
fn test_view_renders_source_library() {
    let app = App::new();
    let mut ui = simulator(app.view());

    // The view should contain these text elements.
    assert!(ui.find("Source Library").is_ok(), "Should show 'Source Library'");
    assert!(ui.find("Timeline").is_ok(), "Should show 'Timeline'");
    assert!(ui.find("Playback").is_ok(), "Should show 'Playback'");
}

#[test]
fn test_view_shows_play_button() {
    let app = App::new();
    let mut ui = simulator(app.view());

    assert!(
        ui.find("Play").is_ok(),
        "Should show 'Play' button when not playing"
    );
}

#[test]
fn test_click_play_produces_message() {
    let mut app = App::new();
    let mut ui = simulator(app.view());

    let _ = ui.click("Play");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert_eq!(messages.len(), 1);

    // Apply the message and check state.
    for msg in messages {
        app.update(msg);
    }
    assert!(app.is_playing);
}

#[test]
fn test_click_pause_when_playing() {
    let mut app = App::new();
    app.update(Message::Play);
    assert!(app.is_playing);

    let mut ui = simulator(app.view());

    // When playing, should show "Pause" button.
    assert!(
        ui.find("Pause").is_ok(),
        "Should show 'Pause' button when playing"
    );

    let _ = ui.click("Pause");
    for msg in ui.into_messages() {
        app.update(msg);
    }
    assert!(!app.is_playing);
}

#[test]
fn test_click_undo_redo_buttons() {
    let mut app = App::new();

    // Add an asset and a clip so there's something to undo.
    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);

    // Click Undo via simulator.
    let mut ui = simulator(app.view());
    let _ = ui.click("Undo");
    for msg in ui.into_messages() {
        app.update(msg);
    }
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 0);

    // Click Redo via simulator.
    let mut ui = simulator(app.view());
    let _ = ui.click("Redo");
    for msg in ui.into_messages() {
        app.update(msg);
    }
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
}

#[test]
fn test_view_shows_imported_assets() {
    let mut app = App::new();

    let asset = make_test_asset("my_video", 5.0);
    app.update(Message::MediaImported(Ok(asset)));

    let mut ui = simulator(app.view());

    // The view should display the asset name.
    assert!(
        ui.find("my_video").is_ok(),
        "Should display imported asset name"
    );
}

#[test]
fn test_view_shows_track_clip_count() {
    let mut app = App::new();

    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    let mut ui = simulator(app.view());

    // Track display should show "1 clips".
    assert!(
        ui.find("Track 0: 1 clips").is_ok(),
        "Should show track with 1 clip"
    );
}

#[test]
fn test_click_add_to_timeline_button() {
    let mut app = App::new();

    let asset = make_test_asset("intro", 5.0);
    app.update(Message::MediaImported(Ok(asset)));

    // Click "Add to Timeline" button via simulator.
    let mut ui = simulator(app.view());
    let _ = ui.click("Add to Timeline");

    for msg in ui.into_messages() {
        app.update(msg);
    }

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    assert_eq!(app.status_message, "Clip added");
}

#[test]
fn test_view_shows_status_message() {
    let mut app = App::new();
    app.status_message = "Test status".into();

    let mut ui = simulator(app.view());
    assert!(
        ui.find("Test status").is_ok(),
        "Should display status message"
    );
}

#[test]
fn test_full_simulator_workflow() {
    let mut app = App::new();

    // Import an asset.
    let asset = make_test_asset("scene1", 10.0);
    app.update(Message::MediaImported(Ok(asset)));

    // Click "Add to Timeline" via simulator.
    let mut ui = simulator(app.view());
    let _ = ui.click("Add to Timeline");
    for msg in ui.into_messages() {
        app.update(msg);
    }

    // Verify clip was added.
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);

    // Click "Play" via simulator.
    let mut ui = simulator(app.view());
    let _ = ui.click("Play");
    for msg in ui.into_messages() {
        app.update(msg);
    }
    assert!(app.is_playing);

    // Click "Pause" via simulator.
    let mut ui = simulator(app.view());
    let _ = ui.click("Pause");
    for msg in ui.into_messages() {
        app.update(msg);
    }
    assert!(!app.is_playing);

    // Click "Undo" via simulator.
    let mut ui = simulator(app.view());
    let _ = ui.click("Undo");
    for msg in ui.into_messages() {
        app.update(msg);
    }
    assert_eq!(app.project.timeline.tracks[0].clips.len(), 0);
}
