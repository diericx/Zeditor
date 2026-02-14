# Refining project rendering

Rendering is working okay right now but it needs to be refined. In reality we should have two settings:

1. Project Settings
   1. A project should have resolution and framerate defined. This defines what the playback window looks like (e.g. if it is 1000x1000 it would show a black square, 1920x1080 would show a rectangle)
   2. We can then edit our video within this canvas scaling, rotating, and stretching video if we want. If we set a video to be smaller within this canvas it will show black on the canvas that it does not cover
2. Render settings
   1. When we set our render settings it will take our clip within our project canvas (1920x1080 for example) and render it out.
      1. So say we put a 500x500 clip in the center of the canvas which is set to 1920x1080 in our project settings. It would have black all around the edges. If we then rendered at 1920x1080 it should have just that, black around the edges as the small video is centered within the larger canvas.
      2. Say we then render it at 1280x720 which is still 16:9. It should then scale the output down but maintain the fidelity of our designed image, being that small clip within a 16:9 canvas that was originally 1920x1080.
      3. Say we render it at another resolution, it should scale the entire canvas that we designed rather than just the clip

Eventually we want to enable effects that will allow us to scale and rotate clips within our project canvas to be rendered how they appear during editing but for now, if a clip does not fit within our project resolution default to scaling it such that it fits within our canvas and is dead center. It is already doing that perfectly when playing back during editing. That needs to be reflected in rendering.

---

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
