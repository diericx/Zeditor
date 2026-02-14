# Multiple tracks

Don't worry about implementing the rendering of all these stacked tracks, focus on the timeline editing aspect.

- Make the track view fill the rest of the vertical space, all the way until down until the system messages bar
- Add a header to the left of each track that has the static track name (V1-VN for video, A1-AN for audio) that stays put while scrolling so you know which track is which
- If you right click on an audio track show a pop up menu with two options; "Add Audio Track Above" and "Add Audio Track Below"
- If you right click on a video track show a pop up menu with two optiosn; "Add Video Track Above" and "Add Video Track Below"
- Video tracks go on top and audio tracks are stacked vertically on top of audio tracks and there should be no way to interweave them
- You should be able to drag solo video clips (not grouped to any audio) onto any video track and vice versa for audio tracks
- Tracks should behave as they do in KDENLive. starting from the center, tracks are kind of linked such that if you try to drag grouped tracks around they will rise out from the center two vide and audio tracks. from the center there is V1 on top and A1 on bottom. If you have 2 vide and 2 audio tracks it would go

V2
V1
A1
A2

If you try to drag a video onto V2 that is grouped to audio it should also move the audio to A2 like this

V2 ==V===
V1
A1
A2 ==A===

If you say have 3 video tracks and only two audio tracks you would not be able to drag the grouped clip to V3 because there is no cooresponding audio track.

---

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
