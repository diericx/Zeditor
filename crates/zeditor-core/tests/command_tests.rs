use uuid::Uuid;
use zeditor_core::commands::CommandHistory;
use zeditor_core::timeline::*;

fn make_clip(asset_id: Uuid, start_secs: f64, duration_secs: f64) -> Clip {
    let source_range = TimeRange::new(
        TimelinePosition::zero(),
        TimelinePosition::from_secs_f64(duration_secs),
    )
    .unwrap();
    Clip::new(asset_id, TimelinePosition::from_secs_f64(start_secs), source_range)
}

#[test]
fn test_undo_redo() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);
    let mut history = CommandHistory::new();

    let asset_id = Uuid::new_v4();

    // Add a clip via command history.
    history
        .execute(&mut timeline, "Add clip", |tl| {
            tl.add_clip(0, make_clip(asset_id, 0.0, 5.0))
        })
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 1);
    assert!(history.can_undo());
    assert!(!history.can_redo());

    // Undo.
    history.undo(&mut timeline).unwrap();
    assert_eq!(timeline.tracks[0].clips.len(), 0);
    assert!(!history.can_undo());
    assert!(history.can_redo());

    // Redo.
    history.redo(&mut timeline).unwrap();
    assert_eq!(timeline.tracks[0].clips.len(), 1);
}

#[test]
fn test_multiple_undo_redo() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);
    let mut history = CommandHistory::new();

    let asset_id = Uuid::new_v4();

    history
        .execute(&mut timeline, "Add clip 1", |tl| {
            tl.add_clip(0, make_clip(asset_id, 0.0, 5.0))
        })
        .unwrap();

    history
        .execute(&mut timeline, "Add clip 2", |tl| {
            tl.add_clip(0, make_clip(asset_id, 5.0, 3.0))
        })
        .unwrap();

    assert_eq!(timeline.tracks[0].clips.len(), 2);

    // Undo twice.
    history.undo(&mut timeline).unwrap();
    assert_eq!(timeline.tracks[0].clips.len(), 1);

    history.undo(&mut timeline).unwrap();
    assert_eq!(timeline.tracks[0].clips.len(), 0);

    // Redo once.
    history.redo(&mut timeline).unwrap();
    assert_eq!(timeline.tracks[0].clips.len(), 1);
}

#[test]
fn test_redo_cleared_on_new_command() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);
    let mut history = CommandHistory::new();

    let asset_id = Uuid::new_v4();

    history
        .execute(&mut timeline, "Add clip 1", |tl| {
            tl.add_clip(0, make_clip(asset_id, 0.0, 5.0))
        })
        .unwrap();

    // Undo.
    history.undo(&mut timeline).unwrap();
    assert!(history.can_redo());

    // New command should clear redo stack.
    history
        .execute(&mut timeline, "Add clip 2", |tl| {
            tl.add_clip(0, make_clip(asset_id, 10.0, 3.0))
        })
        .unwrap();

    assert!(!history.can_redo());
}

#[test]
fn test_undo_empty_fails() {
    let mut timeline = Timeline::new();
    let mut history = CommandHistory::new();
    assert!(history.undo(&mut timeline).is_err());
}

#[test]
fn test_redo_empty_fails() {
    let mut timeline = Timeline::new();
    let mut history = CommandHistory::new();
    assert!(history.redo(&mut timeline).is_err());
}

#[test]
fn test_command_descriptions() {
    let mut timeline = Timeline::new();
    timeline.add_track("Video 1", TrackType::Video);
    let mut history = CommandHistory::new();

    let asset_id = Uuid::new_v4();

    history
        .execute(&mut timeline, "Add first clip", |tl| {
            tl.add_clip(0, make_clip(asset_id, 0.0, 5.0))
        })
        .unwrap();

    assert_eq!(history.undo_description(), Some("Add first clip"));
    assert_eq!(history.redo_description(), None);

    history.undo(&mut timeline).unwrap();
    assert_eq!(history.undo_description(), None);
    assert_eq!(history.redo_description(), Some("Add first clip"));
}
