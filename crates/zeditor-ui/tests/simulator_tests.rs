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

    assert!(
        ui.find("Source Library").is_ok(),
        "Should show 'Source Library'"
    );
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

    assert!(
        ui.find("my_video").is_ok(),
        "Should display imported asset name"
    );
}

#[test]
fn test_click_add_to_timeline_button() {
    let mut app = App::new();

    let asset = make_test_asset("intro", 5.0);
    app.update(Message::MediaImported(Ok(asset)));

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
    // Status bar now contains combined info: "Test status | 00:00:00.000 | Zoom: 100% | Stopped"
    assert!(
        ui.find("Test status | 00:00:00.000 | Zoom: 100% | Stopped")
            .is_ok(),
        "Should display status bar with message"
    );
}

#[test]
fn test_view_renders_with_canvas() {
    // Just verify the view doesn't panic when rendering with canvas timeline
    let mut app = App::new();
    let asset = make_test_asset("clip1", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));
    app.update(Message::AddClipToTimeline {
        asset_id,
        track_index: 0,
        position: TimelinePosition::zero(),
    });

    let _ui = simulator(app.view());
}

#[test]
fn test_view_shows_no_video_placeholder() {
    let app = App::new();
    let mut ui = simulator(app.view());

    assert!(
        ui.find("No video").is_ok(),
        "Should show 'No video' placeholder"
    );
}

#[test]
fn test_three_panel_layout_renders() {
    let app = App::new();
    let mut ui = simulator(app.view());

    // Source panel
    assert!(ui.find("Source Library").is_ok());
    // Video viewport
    assert!(ui.find("No video").is_ok());
    // Controls (Undo/Redo visible)
    assert!(ui.find("Undo").is_ok());
    assert!(ui.find("Redo").is_ok());
}

#[test]
fn test_click_import_button() {
    let mut app = App::new();
    let mut ui = simulator(app.view());

    let _ = ui.click("Import");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert_eq!(messages.len(), 1);

    for msg in messages {
        app.update(msg);
    }
    assert_eq!(app.status_message, "Opening file dialog...");
}

#[test]
fn test_select_asset_shows_highlight() {
    let mut app = App::new();
    let asset = make_test_asset("my_clip", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    // Before selection: "Select" button visible
    {
        let mut ui = simulator(app.view());
        assert!(ui.find("Select").is_ok());
    }

    // Select the asset
    app.update(Message::SelectSourceAsset(Some(asset_id)));

    // After selection: "Selected" button visible + placement hint
    let mut ui = simulator(app.view());
    assert!(ui.find("Selected").is_ok());
    assert!(ui.find("Click timeline to place clip").is_ok());
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
