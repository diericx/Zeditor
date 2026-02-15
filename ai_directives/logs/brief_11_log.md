## Execution Log

### Implementation (2026-02-13)

**1. Added `ProjectSettings` (zeditor-core/src/project.rs)**
- New `ProjectSettings` struct with `canvas_width: u32`, `canvas_height: u32`, `fps: f64`
- Default: 1920x1080 @ 30fps
- Added to `Project` with `#[serde(default)]` for backward compatibility with old save files
- Updated `Project::new()` and `PartialEq` impl to include settings

**2. Added `generate_test_video_with_size()` (zeditor-test-harness/src/fixtures.rs)**
- New fixture helper accepting `width` and `height` params for non-320x240 test sources
- Used by canvas composition tests to create 500x500 test videos

**3. Extended `RenderConfig` and `derive_render_config` (zeditor-media/src/renderer.rs)**
- Added `canvas_width` and `canvas_height` fields to `RenderConfig`
- Changed `derive_render_config` signature to accept `&ProjectSettings`
- Canvas and render dimensions set from project settings; FPS overridden by first source clip

**4. Canvas composition logic (zeditor-media/src/renderer.rs)**
- `compute_canvas_layout()` — pure math function: maps source onto canvas onto render output with scale-to-fit + centering. Forces all dimensions/offsets even for YUV420P alignment.
- `compose_clip_onto_canvas()` — SWS-scales source to clip dimensions, creates black frame at render dims, blits clip at computed position.
- `blit_yuv_frame()` — row-by-row YUV420P plane copy with bounds clamping (Y plane + half-res U/V planes for 4:2:0).

**5. Updated `decode_and_convert_video_frame` and `encode_video_frames`**
- `encode_video_frames` passes `canvas_w`/`canvas_h` from config
- `decode_and_convert_video_frame` computes `CanvasLayout`, creates SWS context targeting clip dimensions (not full render), then calls `compose_clip_onto_canvas`

**6. Updated UI call site (zeditor-ui/src/app.rs ~line 886)**
- Passes `&self.project.settings` as new argument to `derive_render_config`

**7. Tests added/updated**
- 3 new project settings tests: defaults, roundtrip with custom settings, backward compat (stripped settings field)
- 4 new renderer tests: `compute_canvas_layout` pure math, canvas composition (500x500 on 1080p), canvas downscale (320x240 on 1080p rendered at 720p), source-matches-canvas (no borders)
- All existing `RenderConfig` structs updated with `canvas_width`/`canvas_height`
- All `derive_render_config` calls updated with `&ProjectSettings`
- Full workspace: 236 tests passing
