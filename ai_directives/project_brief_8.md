# Better Media Management

Right now the media management is looking rough. Let's fix that to look a bit more like Davinci Resolve's.

- Rather than a list of file names, make it a grid of buttons consisting of rounded rectangles with the file name centered under it
  - Inside the rounded rectangle should be a single frame from the video so we can kind of tell what video it is.
  - When the user hovers over a source clip, highlight it with a border
- Allow the user to DRAG media onto the timeline.
  - When we initially drag the piece of media, show an onion skin (a faded copy) of what is essentially the clip button (being the rounded rect with the frame displayed and the file name text) being dragged with our cursor. This indicates to us what we are dragging.
  - Once the cursor is dragged onto the timeline, show the video in the timeline the same way it would show if we had added it with the Add To Timeline button and dragged it around
  - Once we let go, add the clip to the timeline where it is while we are dragging it.
  - If we let go in an invalid area (for now this is just off the timeline) do not add the clip to the timeline and just remove the onion skin
  - We want to be able to use this same functionality for other buttons in the future. Develop for that in mind.

---

## Execution Log

### Phase 1: Thumbnail Generation + Message Types
- Added `generate_thumbnail_rgba_scaled(path, max_w, max_h)` to `crates/zeditor-media/src/thumbnail.rs` — opens decoder, calls `decode_next_frame_rgba_scaled()`, returns RGBA `VideoFrame`
- Added `DragPayload` enum (extensible for future drag sources), `SourceDragPreview`, `DragState` structs to `crates/zeditor-ui/src/message.rs`
- Added new `Message` variants: `ThumbnailGenerated`, `SourceCardHovered`, `StartDragFromSource`, `DragMoved`, `DragReleased`, `DragEnteredTimeline`, `DragExitedTimeline`, `DragOverTimeline`

### Phase 2: App State + Grid Layout
- Added `thumbnails: HashMap<Uuid, Handle>`, `drag_state: Option<DragState>`, `hovered_asset_id: Option<Uuid>` fields to `App` struct in `app.rs`
- Rewrote `view_source_library()` from flat text list to 2-column grid of thumbnail cards (130px wide, rounded rect with thumbnail image or "..." placeholder, filename centered below)
- Added `view_source_card()` method: renders one card with `mouse_area` wrapping for `on_enter`/`on_exit` (hover border highlight in blue) and `on_press` (start drag)
- Removed old Select/Add to Timeline buttons — drag replaces this workflow
- `MediaImported` handler now spawns `Task::perform` for async thumbnail generation; `ThumbnailGenerated` handler stores `Handle::from_rgba()` in the map

### Phase 3: Drag System
- Added `drag_event_filter` plain function for `event::listen_with` — tracks `Mouse::CursorMoved` → `DragMoved` and `Mouse::ButtonReleased(Left)` → `DragReleased`
- `subscription()` conditionally adds `event::listen_with(drag_event_filter)` when `drag_state.is_some()`
- `StartDragFromSource` handler creates `DragState` with `DragPayload::SourceAsset` (includes thumbnail clone and asset name)
- `DragMoved` updates cursor position; `DragEnteredTimeline`/`DragExitedTimeline` toggle `over_timeline`
- `DragOverTimeline(point)` computes track index (accounting for controls/ruler/track heights) and timeline position from widget-local coords + zoom/scroll; snaps to nearest video track
- `DragReleased` delegates to `AddClipToTimeline` if over timeline with valid track/position, otherwise just clears state
- Escape during drag cancels (clears `drag_state` in `KeyboardEvent` handler)

### Phase 4: Timeline Preview + Drag Overlay
- Added `source_drag: Option<SourceDragPreview>` field to `TimelineCanvas` struct
- `view_timeline()` computes `SourceDragPreview` from `drag_state` via `compute_source_drag_preview()` helper; wraps timeline in `mouse_area` with `on_enter`/`on_exit`/`on_move` when dragging
- Added `view_drag_overlay()`: renders semi-transparent ghost copy of the source card (thumbnail + filename) at cursor position using `stack!` overlay; non-interactive so mouse events pass through
- `view()` wraps base layout in `stack!` with ghost overlay when `drag_state` is Some

### Phase 4b: Accurate Timeline Preview (user feedback)
- **Replaced red overlay rendering** with integrated final-state preview: the timeline now looks identical during hover to how it will look after drop
- Added `audio_track_index: Option<usize>` to `SourceDragPreview` — computed via `find_paired_audio_track()` in `compute_source_drag_preview()`
- Added `SourceDragDrawInfo` struct in `timeline_canvas.rs` — pre-computed before the track loop, holds trim preview maps for both video and audio tracks using `preview_trim_overlaps()`
- In the clip drawing loop: clips affected by the source drop are drawn in their trimmed/split final form (using `draw_clip_shape()` for each `TrimPreview` piece), not with red overlays
- New source clips drawn on both video AND audio tracks inside the track loop (same color/style as real clips)
- Removed the entire old post-loop red overlay rendering block

### Phase 5: Tests
- Updated existing simulator tests that referenced removed "Select"/"Add to Timeline" buttons
- Added 13 message tests: `test_thumbnail_generated_stores_handle`, `test_thumbnail_generated_error_no_crash`, `test_source_card_hover_state`, `test_start_drag_from_source`, `test_start_drag_nonexistent_asset_no_crash`, `test_drag_moved_updates_position`, `test_drag_entered_timeline`, `test_drag_exited_timeline`, `test_drag_over_timeline_computes_position`, `test_drag_released_over_timeline_adds_clip`, `test_drag_released_off_timeline_no_clip`, `test_escape_cancels_drag`, `test_drag_released_clears_state`
- Added 4 simulator tests: `test_view_shows_source_card_with_name`, `test_view_renders_thumbnail_grid`, `test_view_renders_with_drag_state`, `test_view_renders_onion_skin_during_drag`

### Design Decisions
- `DragPayload` is an enum with `SourceAsset` variant — extensible for future drag sources (e.g., clips from timeline, effects)
- Global mouse tracking via `event::listen_with` with a plain function pointer (not closure) as required by iced's API
- Timeline detection via `mouse_area` wrapper on the timeline panel (only added during drag) — avoids always-present overhead
- Ghost overlay is non-interactive (no `opaque()`) so mouse events pass through to underlying widgets
- Track snapping targets nearest video track when dragging from source (audio tracks paired automatically by `AddClipToTimeline`)
- Thumbnail size: 160×90 max, generated asynchronously after import

### Test Results
All 188 tests pass (0 failures): 50 core + 13 media + 2 harness + 123 UI
