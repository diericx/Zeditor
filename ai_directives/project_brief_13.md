# Effects

I want this program to be a free, open and living platform that will grow with community development. Therefore, I would like to create a system where users can create their own video and audio effects and have a marketplace of effects that can be added and used. There would be built in video effects like transform (including pos offset, scale, etc.), flip video horizontal or vertical, etc. and built in audio effects like gain (simple volume control). Uers could then create their own video effects like cartoonize or audio effects like vocal isolation or noise reduction.

If possible create a system where we can safely allow community to build effects while being able to create simple effects ourselves. Our system effects should be developed in the same way as community effects so that they can act as documentation. Make sure we can potentially allow for more complicated effects like face detection or object tracking.

If you decide to use something like WASM for effects, create a new code area where we have our built-in effects in Rust and build them at the same time we build our app and then ship them with our app.

Focus on implementing just Video effects for now.

- Create a simple tab menu where the source library is that has "Project Library" and "Effects". Project library is the default selection and is what is currently the Source Library.
- Effects tab is a simple text list of built in effects for now
- When a clip in the timeline is selected show an "effects" window to the right of the timeline that shows the controls for the effects for the current selected timeline clip
- Allow a user to add an effect to a clip in the timeline by clicking the effect and dragging onto the clip, or simply clicking a button if that is easier and there is too much scope right now
- Effects should be able to add their own functionality into the render pipeline while also accepting user input
  - For now, only expose basic user input like numbers (for x and y offset and scale for transform)
- Effects should show in the render output as well as in the editor playback window
- Implement a single simple Transform effect
  - Clips will default to how they are positioned now. Let transform offset the and y position from user input.

---

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
