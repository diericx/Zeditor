## Execution Log

### Implementation (Brief 13)

**Files created:**
- `crates/zeditor-core/src/effects.rs` — Effects data model: `EffectType`, `ParameterType`, `ParameterDefinition`, `ParameterValue`, `EffectInstance`, `ResolvedTransform`, `resolve_transform()`. 8 inline unit tests.

**Files modified:**
- `crates/zeditor-core/src/lib.rs` — Added `pub mod effects;`
- `crates/zeditor-core/src/timeline.rs` — Added `effects: Vec<EffectInstance>` field to `Clip`, initialized empty in `new()`. Propagated via `clone()` in `cut_at()` (both halves) and `add_clip_trimming_overlaps()` (split right piece). `ParameterValue` gets manual `Eq` impl (f64 doesn't impl Eq natively).
- `crates/zeditor-core/tests/timeline_tests.rs` — Added 2 tests: `test_cut_preserves_effects`, `test_split_by_overlap_preserves_effects`.
- `crates/zeditor-ui/src/message.rs` — Added `LeftPanelTab` enum (ProjectLibrary, Effects), 4 new message variants: `SwitchLeftPanelTab`, `AddEffectToSelectedClip`, `RemoveEffectFromClip`, `UpdateEffectParameter`.
- `crates/zeditor-ui/src/app.rs` — Added `left_panel_tab` and `current_frame_transform` fields to `App`. Added update handlers for all 4 new messages (with command_history for undo/redo). Replaced `view_source_library()` with tabbed `view_left_panel()` containing `view_source_library_content()` and `view_effects_browser()`. Added `view_clip_effects_inspector()` (250px panel right of timeline). Preview viewport applies transform offset via padding. `send_decode_seek()` resolves transform from clip effects.
- `crates/zeditor-media/src/renderer.rs` — `find_video_clip_at()` now returns `Vec<EffectInstance>`. `encode_video_frames()` resolves transform per frame. `decode_and_convert_video_frame()` accepts `&ResolvedTransform` and applies offset to `CanvasLayout.clip_x/clip_y`.
- `crates/zeditor-ui/tests/message_tests.rs` — Added 9 tests: add/remove/update effects, invalid value handling, undo/redo for all three operations, tab switching.
- `crates/zeditor-ui/tests/simulator_tests.rs` — Updated 2 tests to check for "Project Library" tab instead of "Source Library" title.

**Test results:** 276 tests pass, 0 failures (up from ~258).
