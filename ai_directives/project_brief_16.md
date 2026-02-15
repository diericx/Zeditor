# Pixel-Based Effect Pipeline

## Goal

Replace the current parametric-only effect system with a pixel-processing pipeline where effects receive a frame's pixel data, process it, and output modified pixel data. Every effect — including Transform — is a pixel effect. This creates a unified, extensible pipeline that future plugin systems (WASM, shaders, etc.) can hook into without architectural changes.

## Architecture

### FrameBuffer (zeditor-core)

A simple owned pixel buffer that all effects operate on. Lives in core so it has no media/UI dependencies.

```rust
pub struct FrameBuffer {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA, 4 bytes per pixel, row-major
}
```

Provide basic helpers: `new(w, h)` (fully transparent black), `pixel(x, y) -> &[u8; 4]`, `pixel_mut(x, y) -> &mut [u8; 4]`, `from_rgba_vec(w, h, data)`.

### PixelEffect trait (zeditor-core)

```rust
pub trait PixelEffect: Send + Sync {
    /// Process a frame, returning the modified frame.
    /// The output must have the same dimensions as the input.
    fn process(
        &self,
        input: &FrameBuffer,
        params: &[(String, ParameterValue)],
        ctx: &EffectContext,
    ) -> FrameBuffer;

    /// Returns true if the given parameters produce an identity transform
    /// (output == input). Used to skip processing for performance.
    fn is_identity(&self, params: &[(String, ParameterValue)]) -> bool { false }
}

pub struct EffectContext {
    pub time_secs: f64,
    pub frame_number: u64,
    pub fps: f64,
}
```

The trait lives in core and is pure Rust — no FFmpeg, no UI. Effects are deterministic functions: same input + params + context = same output. The trait is object-safe and designed so that future plugin systems can wrap external code behind the same interface.

### How Transform Becomes a Pixel Effect

Currently, Transform offsets are applied during the blit/composite step as position adjustments. Under the new pipeline, Transform is a pixel effect like everything else:

1. Decode the clip at its native resolution
2. Place (blit) the decoded clip onto a **canvas-sized** RGBA FrameBuffer — fitted/centered with aspect ratio preserved, same as current behavior
3. Run the effect pipeline on this canvas-sized buffer — effects execute in order
4. Transform effect shifts all pixels by (x_offset, y_offset) within the canvas-sized buffer. Pixels that move out of bounds are discarded; vacated areas become transparent.
5. Alpha-composite the final canvas-sized buffer onto the output

This means the blit/composite step no longer needs to know about transform offsets. It just receives the fully-processed canvas-sized buffer and composites it. Every effect, including Transform, is just another `process()` call in the chain.

**Alpha compositing**: Since Transform can shift a clip partially off-screen, and since we now have transparent areas in the canvas buffer, the compositor must do alpha-aware blending when layering clips (not opaque overwrite). Use standard alpha-over: `out = src + dst * (1 - src_alpha)`.

### EffectType expansion

Expand the existing `EffectType` enum with new variants. All variants are pixel effects.

```rust
pub enum EffectType {
    Transform,    // Pixel shift by x/y offset
    Grayscale,    // Luminance conversion
    Brightness,   // RGB channel shift
    Opacity,      // Alpha multiplier
}
```

Keep `parameter_definitions()` for each variant as today. Remove `resolve_transform()` and `ResolvedTransform` — Transform is now handled by the pixel pipeline like everything else.

### Effect Registry (zeditor-core)

A registry that maps `EffectType` to its `PixelEffect` implementation. Built-in effects are registered at startup. The registry is designed as the single extension point — future plugin systems would register their effects here alongside builtins.

```rust
pub struct EffectRegistry {
    effects: HashMap<EffectType, Box<dyn PixelEffect>>,
}

impl EffectRegistry {
    pub fn with_builtins() -> Self { ... }
    pub fn get(&self, effect_type: &EffectType) -> Option<&dyn PixelEffect> { ... }
    pub fn register(&mut self, effect_type: EffectType, effect: Box<dyn PixelEffect>) { ... }
}
```

### Built-in Effects (zeditor-core)

Implement 4 pixel effects. Use `rayon` for parallelization where applicable.

1. **Transform** — Parameters: `x_offset` (float), `y_offset` (float). Shifts all pixels by the offset. Source pixels that land outside the canvas are dropped; vacated pixels become transparent (alpha=0). This is an inverse-mapping operation: for each output pixel `(ox, oy)`, sample input at `(ox - x_offset, oy - y_offset)`. `is_identity` returns true when both offsets are 0.0.

2. **Grayscale** — No parameters. Converts each pixel to luminance: `L = 0.299R + 0.587G + 0.114B`, sets R=G=B=L, preserves alpha. Embarrassingly parallel. `is_identity` always returns false (no parameters to check).

3. **Brightness** — One float parameter `brightness` (-1.0 to 1.0, default 0.0). Adds `brightness * 255` to each RGB channel, clamped to 0-255. Preserves alpha. Embarrassingly parallel. `is_identity` returns true when brightness is 0.0.

4. **Opacity** — One float parameter `opacity` (0.0 to 1.0, default 1.0). Multiplies the alpha channel by the opacity value. Embarrassingly parallel. `is_identity` returns true when opacity is 1.0.

Each effect is a struct implementing `PixelEffect`. Use `rayon::par_chunks_mut` to process pixels across cores for the per-pixel effects.

### Performance Strategy

The pixel pipeline adds overhead per clip (canvas buffer allocation, per-pixel processing, alpha compositing). This is acceptable when effects are present but must not penalize clips without effects.

#### No-effect fast path

**If a clip has no effects, skip the pixel pipeline entirely and use the current direct blit path.** Most clips won't have effects, so this keeps the common case at today's performance — zero overhead.

```
if clip.effects.is_empty() {
    // Fast path: decode → blit directly onto output canvas (current behavior)
} else {
    // Effect path: decode → canvas buffer → run effects → alpha composite
}
```

#### Identity skip within the pipeline

When running the effect pipeline, check `is_identity()` before calling `process()` on each effect. If all effects in the chain are identity, the pipeline can skip processing entirely. This handles cases like Transform at (0,0) or Brightness at 0.0.

#### Buffer reuse

Reuse canvas-sized RGBA buffers between clips rather than allocating per clip. A `Vec<u8>` that gets cleared and refilled is nearly free after the first allocation. The pipeline runner should accept a reusable scratch buffer.

#### Performance budget

At 30fps the frame budget is 33ms. FFmpeg decode alone uses 5-20ms.

| Scenario | Added cost | Within budget? |
|---|---|---|
| No effects (fast path) | 0ms | Yes — identical to today |
| 1 effect at 960×540 preview | ~1ms | Yes |
| 5 effects at 960×540 preview | ~3-5ms | Yes |
| 1 effect at 1080p render | ~3-5ms | Yes (render is not real-time) |
| 5 effects at 4K render | ~20-40ms | Tight — GPU acceleration (future brief) solves this |

### Pipeline Integration

The pixel pipeline replaces the current approach in both preview and render paths. The key change: clips with effects are first placed onto a canvas-sized RGBA buffer, then effects run, then the result is composited via alpha blending.

#### Pipeline function (shared logic)

Create a shared function that both preview and render paths call:

```rust
pub fn run_effect_pipeline(
    clip_frame: &FrameBuffer,      // Decoded clip pixels (RGBA, clip resolution)
    canvas_width: u32,
    canvas_height: u32,
    effects: &[EffectInstance],
    registry: &EffectRegistry,
    ctx: &EffectContext,
    scratch: &mut Vec<u8>,         // Reusable scratch buffer
) -> FrameBuffer {
    // 1. Resize scratch to canvas dimensions, fill transparent
    // 2. Blit clip_frame onto scratch (centered, aspect-ratio preserved)
    // 3. Wrap scratch as FrameBuffer
    // 4. For each effect in order:
    //      if let Some(pixel_effect) = registry.get(&effect.effect_type) {
    //          if !pixel_effect.is_identity(&effect.parameters) {
    //              canvas = pixel_effect.process(&canvas, &effect.parameters, ctx);
    //          }
    //      }
    // 5. Return processed canvas
}
```

This function lives in `zeditor-core`. It has no FFmpeg dependency — it operates purely on `FrameBuffer`s.

#### Preview path (zeditor-ui/src/app.rs)

Current flow:
```
decode_next_frame_rgba_scaled() → blit_rgba_scaled() onto output canvas
```

New flow:
```
if clip has effects:
    decode_next_frame_rgba_scaled() → wrap in FrameBuffer
      → run_effect_pipeline(clip_frame, canvas_w, canvas_h, effects, registry, ctx)
      → alpha_composite result onto output canvas
else:
    decode_next_frame_rgba_scaled() → blit_rgba_scaled() onto output canvas (current path)
```

Remove the transform-specific handling (`transform.x_offset`, `transform.y_offset` in `decode_and_composite_multi`) for clips that go through the effect pipeline — Transform handles it inside the pipeline.

#### Render path (zeditor-media/src/renderer.rs)

Current flow:
```
decode raw → SWS to YUV420P → apply transform offset → blit_yuv_frame()
```

New flow:
```
if clip has effects:
    decode raw → SWS to RGBA → wrap in FrameBuffer
      → run_effect_pipeline(clip_frame, canvas_w, canvas_h, effects, registry, ctx)
      → convert result RGBA → YUV420P
      → alpha_composite onto YUV output canvas
else:
    decode raw → SWS to YUV420P → blit_yuv_frame() (current path)
```

The renderer needs to work in RGBA during the effect pipeline, then convert back to YUV for encoding.

### Alpha-Aware Compositing

For clips that go through the effect pipeline, replace opaque blit with alpha-over compositing:

```
For each pixel (x, y):
    src = processed_clip[x, y]    (RGBA from effect pipeline)
    dst = output_canvas[x, y]     (RGBA or YUV)
    alpha = src.a / 255.0
    out.r = src.r * alpha + dst.r * (1 - alpha)
    out.g = src.g * alpha + dst.g * (1 - alpha)
    out.b = src.b * alpha + dst.b * (1 - alpha)
    out.a = src.a + dst.a * (1 - alpha)
```

The no-effect fast path continues using opaque blit as today (clips without effects are fully opaque, no alpha needed).

### UI Updates

- Add Grayscale, Brightness, and Opacity to the Effects browser panel (they should appear via `EffectType::all_builtin()`)
- The existing effects inspector already handles float parameters — Brightness and Opacity will work automatically
- Grayscale has no parameters so it should show as just its name in the inspector (no sliders)

## What NOT to do in this brief

- Plugin loading (WASM or otherwise) — separate future brief. The registry's `register()` method is the hook point.
- GPU/shader acceleration — separate future brief
- Temporal effects (multi-frame access) — future extension to the trait
- Audio effects — separate concern
- Scale/rotation parameters on Transform — future enhancement (keep just x/y offset for now)

## Testing

### Unit tests (zeditor-core)

- `FrameBuffer` helpers: construction, pixel access, out-of-bounds safety
- Each built-in effect:
  - **Transform**: shift pixels by known offset → verify pixel positions, verify vacated area is transparent, verify out-of-bounds pixels are dropped
  - **Grayscale**: known RGB input → expected luminance output, alpha preserved
  - **Brightness**: zero brightness = identity, positive = brighter, negative = darker, clamping at 0/255, alpha preserved
  - **Opacity**: 1.0 = identity, 0.0 = fully transparent, 0.5 = half alpha
- **is_identity**: verify each effect correctly reports identity for default params
- **Pipeline ordering**: apply Brightness then Grayscale vs Grayscale then Brightness → verify different results (proves ordering matters)
- **EffectRegistry**: lookup builtin returns Some, lookup for registered effect works
- **run_effect_pipeline**: clip placed on canvas correctly, effects applied in order, empty effect list = clip placed on canvas without modification
- **Alpha compositing**: semi-transparent over opaque → correct blend, fully transparent → shows background

### Integration tests (zeditor-media)

- Render a clip with Grayscale effect → decode output → verify pixels are grayscale
- Render a clip with Brightness cranked up → verify pixels are brighter than original
- Render a clip with Transform offset → verify clip is shifted in output frame
- Render with no effects → verify output matches current behavior (fast path)

### Message-level tests (zeditor-ui)

- Add new pixel effects to clip → verify they appear in clip's effects list
- Existing Transform tests should continue working (behavior is the same, implementation is different)
- Effects survive undo/redo (already tested for Transform, verify for new types)

## Dependencies to add

- `rayon` in `zeditor-core/Cargo.toml`

---

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
