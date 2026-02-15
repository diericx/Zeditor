## Execution Log

### Session 1 — Pixel Effect Pipeline Implementation

**Tasks completed:**

1. **Added `rayon` dependency** — Added `rayon = "1"` to workspace `Cargo.toml` and `zeditor-core/Cargo.toml`.

2. **Updated `EffectType` enum** (`zeditor-core/src/effects.rs`) — Added `Grayscale`, `Brightness`, `Opacity` variants with `Hash` derive. Added `parameter_definitions()` for each new type. Removed `ResolvedTransform` and `resolve_transform()` — Transform is now handled by the pixel pipeline.

3. **Created `pipeline.rs`** (`zeditor-core/src/pipeline.rs`) — New module containing:
   - `FrameBuffer` struct with `new()`, `pixel()`, `pixel_mut()`, `from_rgba_vec()`, `pixel_count()`
   - `PixelEffect` trait with `process()` and `is_identity()` (default false)
   - `EffectContext` struct
   - 4 built-in effects: `TransformEffect` (row-based copy), `GrayscaleEffect` (luminance), `BrightnessEffect` (RGB shift), `OpacityEffect` (alpha multiply) — all using `rayon::par_chunks_exact_mut(4)` for parallelization
   - `EffectRegistry` with `with_builtins()`, `get()`, `register()`
   - `blit_clip_to_canvas()` — nearest-neighbor scale + center
   - `run_effect_pipeline()` — main pipeline function with identity skip
   - `alpha_composite_rgba()` — Porter-Duff over operation with rayon
   - 37+ comprehensive unit tests

4. **Integrated pixel pipeline into preview path** (`zeditor-ui/src/app.rs`) —
   - Changed `ClipDecodeInfo.transform` field to `effects: Vec<EffectInstance>`
   - Removed `current_frame_transform` field and all assignments
   - Removed dead `composite_frame_for_preview()` function
   - Rewrote `decode_and_composite_multi()` with fast path (no effects → direct blit) and effect path (FrameBuffer → run_effect_pipeline → alpha_composite_rgba)
   - Created `EffectRegistry::with_builtins()` in `decode_worker()`

5. **Integrated pixel pipeline into render path** (`zeditor-media/src/renderer.rs`) —
   - Removed dead `decode_and_convert_video_frame()` function
   - Removed transform parameter from `decode_and_scale_clip()`
   - Added `decode_clip_to_rgba()` for RGBA decode path
   - Added `rgba_framebuffer_to_yuv()` for RGBA→YUV420P conversion via SWS
   - Rewrote `encode_video_frames()` with no-effect fast path (YUV pipeline) and effect path (RGBA → pipeline → alpha composite → YUV conversion)

6. **Added renderer integration tests** (`zeditor-media/tests/renderer_tests.rs`) — 4 new tests: grayscale render, brightness render, transform render, no-effects fast path.

7. **Added message-level tests** (`zeditor-ui/tests/message_tests.rs`) — 7 new tests: add grayscale/brightness/opacity, multiple effects on clip, undo new effect types, update brightness parameter, update opacity parameter.

8. **UI verification** — New effects appear automatically in the effects browser panel via `EffectType::all_builtin()`. Brightness and Opacity float parameters work with the existing inspector. Grayscale shows as name-only (no parameters).

**Issues encountered and resolved:**
- `test_pipeline_ordering_matters` initially failed because chosen input pixel (200,100,50) with brightness 0.2 produced identical results in both orderings. Fixed by using pure red (255,0,0) with brightness 0.5.
- `UpdateEffectParameter.value` field is `String`, not `f64` — fixed test values from numeric literals to `"0.75".to_string()` / `"0.5".to_string()`.

**Final test results:** 353 tests pass across all 4 crates, 0 failures.
