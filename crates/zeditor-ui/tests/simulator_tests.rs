#![allow(unused_must_use)]

use std::path::PathBuf;
use std::time::Duration;

use iced_test::simulator;
use zeditor_core::media::MediaAsset;
use zeditor_core::timeline::TimelinePosition;
use zeditor_ui::app::App;
use zeditor_ui::message::{MenuId, Message};

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
        ui.find("Project Library").is_ok(),
        "Should show 'Project Library' tab"
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
fn test_drag_to_timeline_adds_clip() {
    let mut app = App::new();

    let asset = make_test_asset("intro", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    // Simulate full drag workflow via messages
    app.update(Message::StartDragFromSource(asset_id));
    app.update(Message::DragEnteredTimeline);
    app.update(Message::DragOverTimeline(iced::Point::new(200.0, 70.0)));
    app.update(Message::DragReleased);

    assert_eq!(app.project.timeline.tracks[0].clips.len(), 1);
    assert_eq!(app.status_message, "Clip added");
}

#[test]
fn test_view_shows_status_message() {
    let mut app = App::new();
    app.status_message = "Test status".into();

    let mut ui = simulator(app.view());
    // Status bar is now separate and shows just the message
    assert!(
        ui.find("Test status").is_ok(),
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
    assert!(ui.find("Project Library").is_ok());
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
fn test_full_simulator_workflow() {
    let mut app = App::new();

    // Import an asset.
    let asset = make_test_asset("scene1", 10.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    // Drag to timeline via messages.
    app.update(Message::StartDragFromSource(asset_id));
    app.update(Message::DragEnteredTimeline);
    app.update(Message::DragOverTimeline(iced::Point::new(100.0, 70.0)));
    app.update(Message::DragReleased);

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

// ===== Menu bar simulator tests =====

#[test]
fn test_view_shows_menu_bar() {
    let app = App::new();
    let mut ui = simulator(app.view());

    assert!(ui.find("File").is_ok(), "Should show 'File' menu button");
    assert!(ui.find("Edit").is_ok(), "Should show 'Edit' menu button");
}

#[test]
fn test_click_file_opens_submenu() {
    let mut app = App::new();

    // Click File in the menu bar
    let mut ui = simulator(app.view());
    let _ = ui.click("File");
    for msg in ui.into_messages() {
        app.update(msg);
    }
    assert_eq!(app.open_menu, Some(MenuId::File));

    // Now the dropdown should show File menu items
    let mut ui = simulator(app.view());
    assert!(ui.find("New Project").is_ok(), "Should show 'New Project'");
    assert!(ui.find("Save").is_ok(), "Should show 'Save'");
    assert!(ui.find("Exit").is_ok(), "Should show 'Exit'");
}

#[test]
fn test_click_edit_opens_submenu() {
    let mut app = App::new();

    // Click Edit in the menu bar
    let mut ui = simulator(app.view());
    let _ = ui.click("Edit");
    for msg in ui.into_messages() {
        app.update(msg);
    }
    assert_eq!(app.open_menu, Some(MenuId::Edit));

    // Now the dropdown should show Edit menu items
    let mut ui = simulator(app.view());
    assert!(ui.find("Undo").is_ok(), "Should show 'Undo' in Edit menu");
    assert!(ui.find("Redo").is_ok(), "Should show 'Redo' in Edit menu");
}

#[test]
fn test_view_renders_with_open_menu() {
    let mut app = App::new();
    app.open_menu = Some(MenuId::File);

    // Should not panic when rendering with open menu
    let _ui = simulator(app.view());
}

#[test]
fn test_view_renders_with_open_edit_menu() {
    let mut app = App::new();
    app.open_menu = Some(MenuId::Edit);

    // Should not panic when rendering with open Edit menu
    let _ui = simulator(app.view());
}

// ===== Brief 8: Media management simulator tests =====

#[test]
fn test_view_shows_source_card_with_name() {
    let mut app = App::new();
    let asset = make_test_asset("my_video_clip", 5.0);
    app.update(Message::MediaImported(Ok(asset)));

    let mut ui = simulator(app.view());
    assert!(
        ui.find("my_video_clip").is_ok(),
        "Should display asset name in source card"
    );
}

#[test]
fn test_view_renders_thumbnail_grid() {
    let mut app = App::new();

    // Import two assets
    let asset1 = make_test_asset("clip_a", 5.0);
    let asset2 = make_test_asset("clip_b", 3.0);
    app.update(Message::MediaImported(Ok(asset1)));
    app.update(Message::MediaImported(Ok(asset2)));

    // Should render without panic
    let mut ui = simulator(app.view());
    assert!(ui.find("clip_a").is_ok());
    assert!(ui.find("clip_b").is_ok());
}

#[test]
fn test_view_renders_with_drag_state() {
    let mut app = App::new();
    let asset = make_test_asset("dragging_clip", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    // Set up drag state
    app.update(Message::StartDragFromSource(asset_id));
    app.update(Message::DragMoved(iced::Point::new(300.0, 200.0)));

    // Should render without panic with drag overlay
    let _ui = simulator(app.view());
}

#[test]
fn test_view_renders_onion_skin_during_drag() {
    let mut app = App::new();
    let asset = make_test_asset("onion_clip", 5.0);
    let asset_id = asset.id;
    app.update(Message::MediaImported(Ok(asset)));

    // Set up drag over timeline
    app.update(Message::StartDragFromSource(asset_id));
    app.update(Message::DragEnteredTimeline);
    app.update(Message::DragOverTimeline(iced::Point::new(200.0, 70.0)));
    app.update(Message::DragMoved(iced::Point::new(200.0, 400.0)));

    // Should render both ghost overlay and timeline preview without panic
    let _ui = simulator(app.view());
}

// ===== Brief 9: Save / Load / New Project simulator tests =====

#[test]
fn test_file_menu_shows_load_project() {
    let mut app = App::new();
    app.open_menu = Some(MenuId::File);

    let mut ui = simulator(app.view());
    assert!(
        ui.find("Load Project").is_ok(),
        "File menu should show 'Load Project'"
    );
}

#[test]
fn test_window_title_reflects_project_name() {
    let app = App::new();
    assert_eq!(app.title(), "Untitled - Zeditor");
}

#[test]
fn test_window_title_updates_after_name_change() {
    let mut app = App::new();
    app.project.name = "Custom Project".into();
    assert_eq!(app.title(), "Custom Project - Zeditor");
}

// ===== Brief 10: Render simulator tests =====

#[test]
fn test_file_menu_shows_render() {
    let mut app = App::new();
    app.open_menu = Some(MenuId::File);

    let mut ui = simulator(app.view());
    assert!(
        ui.find("Render").is_ok(),
        "File menu should show 'Render'"
    );
}
