## Execution Log

### Phase 1: Core — Track Ordering & Mirroring (`zeditor-core/src/timeline.rs`)
- Added `video_track_indices()`, `audio_track_indices()`, `first_audio_track_index()` query helpers
- Added `mirror_audio_track_for_video()` and `mirror_video_track_for_audio()` using center-out mirror logic (V1↔A1, V2↔A2, etc.)
- Added `insert_video_track_above/below()` and `insert_audio_track_above/below()` with video-before-audio invariant enforcement
- Added `renumber_tracks()` — renames VN..V1 (top-to-bottom) and A1..AN
- Updated `find_paired_audio_track()` to delegate to `mirror_audio_track_for_video()` (replaced group_id-based lookup)
- Updated `move_clip_grouped()` to validate mirror track existence for cross-track moves and move linked clips to mirror destination
- Added `NoMirrorTrack`, `TrackTypeMismatch`, `InvalidTrackInsertion` error variants to `error.rs`
- Updated `Project::new()` to use "V1"/"A1" track names (removed group_id-based approach)

### Phase 2: UI Messages & State
- Added `TrackContextMenu` struct to `message.rs` with `track_index`, `position`, `track_type` fields
- Added 6 new Message variants: `ShowTrackContextMenu`, `DismissTrackContextMenu`, `AddVideoTrackAbove/Below`, `AddAudioTrackAbove/Below`
- Added `track_context_menu: Option<TrackContextMenu>` to `App` struct

### Phase 3: Right-Click & Context Menu
- Added right-click handler in `timeline_canvas.rs` `update()` → emits `ShowTrackContextMenu` with track index from `track_at_y`
- Added handlers in `app.rs` `update()` for all 6 track context menu messages (insert via command_history, renumber, clear selected_clip)
- Added escape key dismissal for track context menu
- Added context menu rendering in `view_timeline()` using iced `stack!` overlay pattern with transparent click-off layer

### Phase 4: Track Headers & Layout
- Rewrote `view_timeline()` to include 60px track header column left of canvas with V1/V2/A1/A2 labels
- Changed canvas height from fixed 200 to `Length::Fill`
- Added `Length::Fill` to timeline_row height so timeline fills vertical space above status bar

### Phase 5: Drag Validation
- Added `validate_drag_dest_track()` method to `TimelineCanvas` with track type validation for solo and grouped clips
- Added `nearest_track_of_type()`, `nearest_video_track_with_mirror()`, `nearest_audio_track_with_mirror()` helpers
- Updated drag release handler to validate before emitting `MoveClip`
- Updated `DragOverTimeline` handler to account for 60px header_width offset

### Phase 6: Tests
**Core tests added** (in `timeline_tests.rs`):
- `test_mirror_computation_2v_2a`, `test_mirror_computation_3v_2a_no_match`
- `test_video_track_indices`, `test_audio_track_indices`, `test_first_audio_track_index`
- `test_insert_video_track_above`, `test_insert_video_track_below`
- `test_insert_audio_track_above`, `test_insert_audio_track_below`
- `test_renumber_tracks`, `test_ordering_invariant_after_insertions`
- `test_insert_wrong_type_rejected`
- `test_grouped_cross_track_move_with_mirror`
- `test_grouped_cross_track_move_no_mirror_rejected`
- `test_find_paired_audio_uses_mirror`
- `test_default_project_tracks`

**UI message tests added** (in `message_tests.rs`):
- `test_show_track_context_menu`, `test_dismiss_track_context_menu`, `test_escape_dismisses_track_context_menu`
- `test_add_video_track_above`, `test_add_video_track_below`
- `test_add_audio_track_above`, `test_add_audio_track_below`
- `test_add_track_is_undoable`, `test_add_track_clears_context_menu`
- `test_add_multiple_tracks_renumbers_correctly`
- `test_default_project_track_names`

**Existing test fixes:**
- Updated `project_tests.rs` `test_new_project_has_default_tracks` for V1/A1 naming
- Updated `message_tests.rs` `test_drag_over_timeline_computes_position` for 60px header offset

**All 302 tests pass** across the workspace (`cargo test --workspace` — 0 failures).
