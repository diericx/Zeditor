# Small features and bug fixes

- Adding vertical video (rotated MOV) to the project that is currently set to 1920x1080 renders diagonal striped video in the preview
  - It seems to render with the correct letterboxing in the preview (looks like a vertical rectangle)
  - it should render in that same rectangle but with correct video pixels
- These same vertical videos show as similarly striped in their thumbnails in the project library. Should show a correct screenshot with letterboxing.
- Clicking on videos in the project library (sources) (highlihgt in red) should select them and then I should be able to press delete to remove them from the library
  - If there are any clips in the timeline currently using them, have a pop up asking if I'm sure I want to delete them
- Clicking clips in the timeline should allow me to select them (highlight in red) should select them and let me delete them by pressing delete
- Video tracks should always be green while audio tracks are blue. Color should be the same every time you open the project not random.

---

## Execution Log

### 1. Track Colors (Video=Green, Audio=Blue)
- **File:** `crates/zeditor-ui/src/widgets/timeline_canvas.rs`
- Replaced `color_from_uuid(asset_id)` with `color_for_track_type(track_type)` at all 5 call sites
  - Video → `Color::from_rgb(0.25, 0.65, 0.35)` (green)
  - Audio → `Color::from_rgb(0.25, 0.45, 0.75)` (blue)
- Removed `color_from_uuid` function entirely
- Removed unused `asset_id` field from `SourceDragDrawInfo`
- Added tests: `test_color_for_track_type_video_is_green`, `test_color_for_track_type_audio_is_blue`

### 2. Timeline Clip Selection + Delete
- **New state:** `App::selected_clip: Option<(usize, Uuid)>` — (track_index, clip_id)
- **New messages:** `SelectTimelineClip(Option<(usize, Uuid)>)`, `RemoveClip { track_index, clip_id }`
- **Canvas changes** (`timeline_canvas.rs`):
  - Added `selected_clip` field to `TimelineCanvas`
  - Added `start_x: f32` to `TimelineInteraction::Dragging` for click-vs-drag detection
  - On `ButtonReleased`, if movement < 5px, publish `SelectTimelineClip` instead of `MoveClip`
  - `TimelineClickEmpty` also clears selection
  - Selected clip drawn with 3px red border
- **App changes** (`app.rs`):
  - `SelectTimelineClip` → set `self.selected_clip`
  - `RemoveClip` → `command_history.execute()` for undo; linked clips also removed via `remove_clip_grouped`
  - Delete/Backspace key → dispatch `RemoveClip` if clip selected
- **Core** (`timeline.rs`): Added `Timeline::remove_clip_grouped(track_index, clip_id)`
- Added 7 tests: `test_select_timeline_clip`, `test_delete_selected_clip`, `test_delete_linked_removes_both`, `test_delete_is_undoable`, `test_click_empty_deselects_clip`, `test_delete_key_removes_selected_clip`, `test_delete_key_no_selection_noop`

### 3. Source Library Selection + Delete with Confirmation
- **New types:** `ConfirmAction::RemoveAsset { asset_id }`, `ConfirmDialog { message, action }`
- **New messages:** `ConfirmRemoveAsset(Uuid)`, `ConfirmDialogAccepted`, `ConfirmDialogDismissed`
- **New state:** `App::confirm_dialog: Option<ConfirmDialog>`
- `StartDragFromSource` now also sets `selected_asset_id`
- Source card shows red border when selected
- Delete key: checks `selected_asset_id`, dispatches `ConfirmRemoveAsset`
- `ConfirmRemoveAsset` handler: if clips using asset exist → show confirmation dialog; otherwise remove directly
- `ConfirmDialogAccepted`: removes all clips using the asset (via command history for undo), then removes asset
- Confirmation dialog: modal overlay using `stack!` + `opaque()` pattern with Delete/Cancel buttons
- Keyboard blocked when dialog open (Escape dismisses)
- **Core** (`timeline.rs`): Added `Timeline::clips_using_asset(asset_id)` and `Timeline::remove_clips_by_asset(asset_id)`
- Added 6 tests: `test_select_source_asset_on_drag`, `test_delete_key_removes_source_no_clips`, `test_delete_with_clips_shows_confirmation`, `test_confirm_removes_asset_and_clips`, `test_dismiss_keeps_asset`, `test_delete_key_no_selection_noop` (shared)

### 4. Vertical Video Fix (Rotation Metadata)
- **Probe** (`probe.rs`):
  - Added `extract_rotation_from_side_data()` using `av_packet_side_data_get` + `av_display_rotation_get` on codec parameters' `coded_side_data`
  - Added `normalize_rotation()` to round angles to 0/90/180/270
  - `probe()` now stores `asset.rotation = rotation`
- **Core** (`media.rs`):
  - Added `#[serde(default)] pub rotation: u32` to `MediaAsset`
  - Added `display_width()` / `display_height()` methods (swap w/h for 90/270)
- **Decoder** (`decoder.rs`):
  - Added `rotation` to `StreamInfo` and `FfmpegDecoder`, extracted from stream in `open()`
  - Added pure Rust `rotate_rgba_90/180/270` functions for RGBA pixel rotation
  - `frame_to_scaled` now scales based on post-rotation dimensions, then applies RGBA rotation
- **Renderer** (`renderer.rs`):
  - Added `rotation` to `CachedVideoDecoder`
  - `decode_and_convert_video_frame` uses `display_width()`/`display_height()` for canvas layout
  - Added `compose_clip_onto_canvas_rotated` — scales, rotates YUV420P planes, blits onto canvas
  - Added `rotate_yuv420p_frame` and `rotate_plane` for Y/U/V plane rotation
  - Removed old unused `compose_clip_onto_canvas`
- **Fixtures** (`fixtures.rs`):
  - Added `generate_test_video_rotated(dir, name, duration, w, h, rotation)` — creates base MP4 then remuxes with `-display_rotation` (negated for phone CW convention) to produce MOV with display matrix
- **Tests added:**
  - `test_generate_test_video_rotated` (fixture)
  - `test_probe_rotated_video`, `test_probe_non_rotated_video` (probe)
  - `test_open_rotated_video_has_rotation`, `test_decode_rotated_dimensions` (decoder)
  - `test_rotate_rgba_90`, `test_rotate_rgba_180`, `test_rotate_rgba_270` (pure rotation)

### Summary
- All 258 tests pass (`cargo test --workspace`)
- Sign convention: `rotation` stored as CW degrees to apply for correct display (matching phone/ffprobe convention). Extracted via `-av_display_rotation_get(matrix)` then normalized.
- Fixture generator uses `-display_rotation` with negated angle (CCW input → CW stored value)

### Bug Fixes (post-initial implementation)

#### Fix 1: Vertical video stride mismatch
- **Problem:** Rotated video showed correct rectangle shape but slanted lines (diagonal stripes) in preview
- **Root cause:** `frame_to_scaled()` in `decoder.rs` copied pixel data assuming stride == width * bytes_per_pixel, but FFmpeg pads rows to alignment boundaries. For rotated video dimensions, the stride didn't match.
- **Fix:** Copy row-by-row using `linesize` when it differs from `width * bytes_per_pixel`
- **File:** `crates/zeditor-media/src/decoder.rs`

#### Fix 2: Remove click-to-place, fix source selection
- **Problem:** Dragging a clip from the library selected it AND triggered the "click to place clip" feature
- **Fix:**
  - Removed `PlaceSelectedClip` message, handler, and all canvas emission code
  - Removed hint text ("Click timeline to place clip") and crosshair cursor
  - Removed `selected_asset_id` from `TimelineCanvas` struct
  - Source cards now select on `on_release` (mouse-up), not on drag start
  - Removed `self.selected_asset_id = Some(asset_id)` from `StartDragFromSource`
- **Files:** `message.rs`, `app.rs`, `timeline_canvas.rs`
- **Tests updated:** Removed `test_place_selected_clip`, `test_place_clears_selection`, `test_place_selected_clip_with_audio`, `test_select_asset_shows_placement_hint`. Updated `test_select_source_asset_on_drag` → `test_drag_does_not_select_source_asset`. Updated overlap test to use `AddClipToTimeline`.

#### Fix 3: Grouped clip selection highlighting
- **Problem:** Selecting a linked clip (video+audio pair) only highlighted the clicked clip, not its linked partner
- **Fix:** Before the track drawing loop, compute the `link_id` of the selected clip. When drawing each clip, check `clip.link_id == selected_link_id` in addition to direct ID match.
- **File:** `crates/zeditor-ui/src/widgets/timeline_canvas.rs`
- **Test added:** `test_select_grouped_clip_links_are_tracked`

### Final Summary
- All 253 tests pass (`cargo test --workspace`)
- Net test change: removed 4 PlaceSelectedClip tests, added 2 new tests
