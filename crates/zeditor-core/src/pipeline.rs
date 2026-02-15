use std::collections::HashMap;

use rayon::prelude::*;

use crate::effects::{EffectInstance, EffectType, ParameterValue};

// =============================================================================
// FrameBuffer
// =============================================================================

/// An owned RGBA pixel buffer. 4 bytes per pixel, row-major.
#[derive(Debug, Clone)]
pub struct FrameBuffer {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl FrameBuffer {
    /// Create a new transparent black buffer.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0u8; (width * height * 4) as usize],
        }
    }

    /// Create from existing RGBA data. Panics if data length doesn't match dimensions.
    pub fn from_rgba_vec(width: u32, height: u32, data: Vec<u8>) -> Self {
        assert_eq!(
            data.len(),
            (width * height * 4) as usize,
            "RGBA data length {} doesn't match {}x{}x4={}",
            data.len(),
            width,
            height,
            width * height * 4
        );
        Self {
            width,
            height,
            data,
        }
    }

    /// Get pixel RGBA at (x, y). Panics if out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> &[u8] {
        let idx = ((y * self.width + x) * 4) as usize;
        &self.data[idx..idx + 4]
    }

    /// Get mutable pixel RGBA at (x, y). Panics if out of bounds.
    pub fn pixel_mut(&mut self, x: u32, y: u32) -> &mut [u8] {
        let idx = ((y * self.width + x) * 4) as usize;
        &mut self.data[idx..idx + 4]
    }

    /// Total number of pixels.
    pub fn pixel_count(&self) -> usize {
        (self.width * self.height) as usize
    }
}

// =============================================================================
// PixelEffect trait and EffectContext
// =============================================================================

/// Context provided to effects for time-dependent processing.
pub struct EffectContext {
    pub time_secs: f64,
    pub frame_number: u64,
    pub fps: f64,
}

/// Trait for pixel-processing effects. All effects — including Transform —
/// implement this trait. The process method receives a canvas-sized RGBA buffer
/// and returns a processed buffer of the same dimensions.
pub trait PixelEffect: Send + Sync {
    /// Process a frame, returning the modified frame. Takes ownership of the
    /// input buffer so in-place effects can avoid allocating a new buffer.
    fn process(
        &self,
        input: FrameBuffer,
        params: &[(String, ParameterValue)],
        ctx: &EffectContext,
    ) -> FrameBuffer;

    /// Returns true if the given parameters produce an identity transform
    /// (output == input). Used to skip processing for performance.
    fn is_identity(&self, params: &[(String, ParameterValue)]) -> bool {
        let _ = params;
        false
    }

    /// Returns true if this effect can produce transparent pixels (alpha < 255)
    /// from fully-opaque input. Used to decide whether alpha compositing is needed.
    fn may_produce_transparency(&self) -> bool {
        false
    }
}

// =============================================================================
// Built-in effects
// =============================================================================

fn get_float_param(params: &[(String, ParameterValue)], name: &str) -> Option<f64> {
    params.iter().find_map(|(n, v)| {
        if n == name {
            match v {
                ParameterValue::Float(f) => Some(*f),
            }
        } else {
            None
        }
    })
}

/// Shifts all pixels by (x_offset, y_offset). Vacated areas become transparent.
pub struct TransformEffect;

impl PixelEffect for TransformEffect {
    fn process(
        &self,
        input: FrameBuffer,
        params: &[(String, ParameterValue)],
        _ctx: &EffectContext,
    ) -> FrameBuffer {
        let x_off = get_float_param(params, "x_offset").unwrap_or(0.0);
        let y_off = get_float_param(params, "y_offset").unwrap_or(0.0);

        let w = input.width as i64;
        let h = input.height as i64;
        let x_off_i = x_off.round() as i64;
        let y_off_i = y_off.round() as i64;

        // Transform can't work in-place (reads and writes overlap), so allocate
        let mut output = FrameBuffer::new(input.width, input.height);

        // Compute visible output range
        let out_y_start = 0i64.max(y_off_i) as u32;
        let out_y_end = (h.min(h + y_off_i)) as u32;
        let out_x_start = 0i64.max(x_off_i) as u32;
        let out_x_end = (w.min(w + x_off_i)) as u32;

        if out_x_start >= out_x_end || out_y_start >= out_y_end {
            return output;
        }

        let src_x_start = (out_x_start as i64 - x_off_i) as u32;
        let row_len = (out_x_end - out_x_start) as usize * 4;
        let src_stride = input.width as usize * 4;
        let dst_stride = output.width as usize * 4;

        for out_y in out_y_start..out_y_end {
            let src_y = (out_y as i64 - y_off_i) as u32;
            let src_offset = (src_y as usize * src_stride) + (src_x_start as usize * 4);
            let dst_offset = (out_y as usize * dst_stride) + (out_x_start as usize * 4);
            output.data[dst_offset..dst_offset + row_len]
                .copy_from_slice(&input.data[src_offset..src_offset + row_len]);
        }

        output
    }

    fn is_identity(&self, params: &[(String, ParameterValue)]) -> bool {
        let x = get_float_param(params, "x_offset").unwrap_or(0.0);
        let y = get_float_param(params, "y_offset").unwrap_or(0.0);
        x == 0.0 && y == 0.0
    }

    fn may_produce_transparency(&self) -> bool {
        true
    }
}

/// Converts each pixel to luminance, preserving alpha.
pub struct GrayscaleEffect;

impl PixelEffect for GrayscaleEffect {
    fn process(
        &self,
        mut input: FrameBuffer,
        _params: &[(String, ParameterValue)],
        _ctx: &EffectContext,
    ) -> FrameBuffer {
        // In-place: row-based parallelism to avoid rayon micro-task overhead
        let row_bytes = input.width as usize * 4;
        input
            .data
            .par_chunks_exact_mut(row_bytes)
            .for_each(|row| {
                for pixel in row.chunks_exact_mut(4) {
                    let r = pixel[0] as f32;
                    let g = pixel[1] as f32;
                    let b = pixel[2] as f32;
                    let l = (0.299 * r + 0.587 * g + 0.114 * b).round() as u8;
                    pixel[0] = l;
                    pixel[1] = l;
                    pixel[2] = l;
                    // alpha unchanged
                }
            });
        input
    }
}

/// Shifts RGB channels by brightness * 255, clamped. Preserves alpha.
pub struct BrightnessEffect;

impl PixelEffect for BrightnessEffect {
    fn process(
        &self,
        mut input: FrameBuffer,
        params: &[(String, ParameterValue)],
        _ctx: &EffectContext,
    ) -> FrameBuffer {
        let brightness = get_float_param(params, "brightness").unwrap_or(0.0);
        let shift = (brightness * 255.0).round() as i16;

        // In-place: row-based parallelism to avoid rayon micro-task overhead
        let row_bytes = input.width as usize * 4;
        input
            .data
            .par_chunks_exact_mut(row_bytes)
            .for_each(|row| {
                for pixel in row.chunks_exact_mut(4) {
                    pixel[0] = (pixel[0] as i16 + shift).clamp(0, 255) as u8;
                    pixel[1] = (pixel[1] as i16 + shift).clamp(0, 255) as u8;
                    pixel[2] = (pixel[2] as i16 + shift).clamp(0, 255) as u8;
                    // alpha unchanged
                }
            });
        input
    }

    fn is_identity(&self, params: &[(String, ParameterValue)]) -> bool {
        get_float_param(params, "brightness").unwrap_or(0.0) == 0.0
    }
}

/// Multiplies alpha channel by opacity value.
pub struct OpacityEffect;

impl PixelEffect for OpacityEffect {
    fn process(
        &self,
        mut input: FrameBuffer,
        params: &[(String, ParameterValue)],
        _ctx: &EffectContext,
    ) -> FrameBuffer {
        let opacity = get_float_param(params, "opacity").unwrap_or(1.0);

        // In-place: row-based parallelism to avoid rayon micro-task overhead
        let row_bytes = input.width as usize * 4;
        input
            .data
            .par_chunks_exact_mut(row_bytes)
            .for_each(|row| {
                for pixel in row.chunks_exact_mut(4) {
                    pixel[3] = (pixel[3] as f32 * opacity as f32).round().clamp(0.0, 255.0) as u8;
                }
            });
        input
    }

    fn is_identity(&self, params: &[(String, ParameterValue)]) -> bool {
        get_float_param(params, "opacity").unwrap_or(1.0) == 1.0
    }

    fn may_produce_transparency(&self) -> bool {
        true
    }
}

// =============================================================================
// Effect Registry
// =============================================================================

/// Maps EffectType to its PixelEffect implementation. Built-in effects are
/// registered at startup. Future plugin systems register here too.
pub struct EffectRegistry {
    effects: HashMap<EffectType, Box<dyn PixelEffect>>,
}

impl EffectRegistry {
    /// Create a registry with all built-in effects registered.
    pub fn with_builtins() -> Self {
        let mut registry = Self {
            effects: HashMap::new(),
        };
        registry.register(EffectType::Transform, Box::new(TransformEffect));
        registry.register(EffectType::Grayscale, Box::new(GrayscaleEffect));
        registry.register(EffectType::Brightness, Box::new(BrightnessEffect));
        registry.register(EffectType::Opacity, Box::new(OpacityEffect));
        registry
    }

    /// Look up the pixel effect implementation for a given type.
    pub fn get(&self, effect_type: &EffectType) -> Option<&dyn PixelEffect> {
        self.effects.get(effect_type).map(|e| e.as_ref())
    }

    /// Register a new effect implementation (for plugins).
    pub fn register(&mut self, effect_type: EffectType, effect: Box<dyn PixelEffect>) {
        self.effects.insert(effect_type, effect);
    }
}

// =============================================================================
// Pipeline functions
// =============================================================================

/// Blit a source RGBA frame onto a canvas-sized buffer, centered and
/// aspect-ratio preserved (letterboxed). Nearest-neighbor scaling.
/// Takes ownership of clip to avoid cloning when dimensions already match.
pub fn blit_clip_to_canvas(
    clip: FrameBuffer,
    canvas_w: u32,
    canvas_h: u32,
) -> FrameBuffer {
    if clip.width == 0 || clip.height == 0 || canvas_w == 0 || canvas_h == 0 {
        return FrameBuffer::new(canvas_w, canvas_h);
    }

    // Fast path: dimensions match exactly — no scaling or centering needed.
    // This is the common case when preview resolution matches decoded frame size.
    if clip.width == canvas_w && clip.height == canvas_h {
        return clip;
    }

    let mut canvas = FrameBuffer::new(canvas_w, canvas_h);

    // Compute fit: scale clip to fit canvas, preserving aspect ratio
    let scale_x = canvas_w as f64 / clip.width as f64;
    let scale_y = canvas_h as f64 / clip.height as f64;
    let scale = scale_x.min(scale_y);

    let dst_w = (clip.width as f64 * scale).round() as u32;
    let dst_h = (clip.height as f64 * scale).round() as u32;

    if dst_w == 0 || dst_h == 0 {
        return canvas;
    }

    // Center on canvas
    let offset_x = (canvas_w.saturating_sub(dst_w)) / 2;
    let offset_y = (canvas_h.saturating_sub(dst_h)) / 2;

    let src_stride = clip.width as usize * 4;
    let dst_stride = canvas_w as usize * 4;

    // Nearest-neighbor blit using integer math to avoid f64 division per pixel
    for dy in 0..dst_h {
        let cy = offset_y + dy;
        if cy >= canvas_h {
            break;
        }
        let sy = ((dy as u64 * clip.height as u64) / dst_h as u64).min(clip.height as u64 - 1) as u32;
        let dst_row_offset = cy as usize * dst_stride;
        let src_row_offset = sy as usize * src_stride;

        for dx in 0..dst_w {
            let cx = offset_x + dx;
            if cx >= canvas_w {
                break;
            }
            let sx = ((dx as u64 * clip.width as u64) / dst_w as u64).min(clip.width as u64 - 1) as u32;
            let si = src_row_offset + sx as usize * 4;
            let di = dst_row_offset + cx as usize * 4;
            canvas.data[di..di + 4].copy_from_slice(&clip.data[si..si + 4]);
        }
    }

    canvas
}

/// Result of running the effect pipeline, including metadata for the compositor.
pub struct PipelineResult {
    pub frame: FrameBuffer,
    /// True if any active effect may have produced transparent pixels.
    /// When false, the compositor can skip alpha blending and do a direct copy.
    pub may_have_transparency: bool,
    /// True if the clip fills the entire canvas (dimensions matched exactly).
    /// When `fills_canvas && !may_have_transparency`, `composite_opaque` is safe
    /// because every pixel is guaranteed alpha=255 with no letterbox gaps.
    pub fills_canvas: bool,
}

/// Run the full effect pipeline on a decoded clip frame.
///
/// 1. Blits the clip onto a canvas-sized buffer (centered, aspect-fit)
/// 2. Runs each effect in order (skipping identity effects)
/// 3. Returns the processed canvas-sized FrameBuffer plus transparency metadata
///
/// Takes ownership of clip_frame to avoid cloning when dimensions match canvas.
pub fn run_effect_pipeline(
    clip_frame: FrameBuffer,
    canvas_width: u32,
    canvas_height: u32,
    effects: &[EffectInstance],
    registry: &EffectRegistry,
    ctx: &EffectContext,
) -> PipelineResult {
    // Track whether clip fills the canvas (dimensions match exactly).
    // blit_clip_to_canvas returns the clip unchanged when dimensions match,
    // so we check before calling it.
    let fills_canvas = clip_frame.width == canvas_width && clip_frame.height == canvas_height;
    let mut canvas = blit_clip_to_canvas(clip_frame, canvas_width, canvas_height);
    let mut may_have_transparency = false;

    for effect in effects {
        if let Some(pixel_effect) = registry.get(&effect.effect_type) {
            if !pixel_effect.is_identity(&effect.parameters) {
                if pixel_effect.may_produce_transparency() {
                    may_have_transparency = true;
                }
                canvas = pixel_effect.process(canvas, &effect.parameters, ctx);
            }
        }
    }

    PipelineResult {
        frame: canvas,
        may_have_transparency,
        fills_canvas,
    }
}

/// Alpha-over composite: blend src onto dst. Both must have the same dimensions.
/// Standard Porter-Duff "over" operation.
/// Uses row-based parallelism to avoid rayon per-pixel scheduling overhead.
pub fn alpha_composite_rgba(src: &FrameBuffer, dst: &mut FrameBuffer) {
    assert_eq!(src.width, dst.width);
    assert_eq!(src.height, dst.height);

    let row_bytes = src.width as usize * 4;
    let src_data = &src.data;
    let dst_data = &mut dst.data;

    // Process rows in parallel — much less rayon overhead than per-pixel
    dst_data
        .par_chunks_exact_mut(row_bytes)
        .enumerate()
        .for_each(|(row, dst_row)| {
            let src_start = row * row_bytes;
            let src_row = &src_data[src_start..src_start + row_bytes];

            for i in (0..row_bytes).step_by(4) {
                let sa = src_row[i + 3] as u32;
                if sa == 255 {
                    // Fully opaque source — just copy
                    dst_row[i..i + 4].copy_from_slice(&src_row[i..i + 4]);
                } else if sa > 0 {
                    let da = dst_row[i + 3] as u32;
                    let inv_sa = 255 - sa;
                    // out_a = sa + da * (1 - sa/255), scaled to 0..255
                    let out_a = sa + ((da * inv_sa + 127) / 255);
                    if out_a > 0 {
                        for c in 0..3 {
                            let sc = src_row[i + c] as u32;
                            let dc = dst_row[i + c] as u32;
                            // Porter-Duff: (sc * sa + dc * da * inv_sa / 255) / out_a
                            let num = sc * sa + ((dc * da * inv_sa + 127) / 255);
                            dst_row[i + c] = ((num + out_a / 2) / out_a).min(255) as u8;
                        }
                        dst_row[i + 3] = out_a.min(255) as u8;
                    }
                }
                // sa == 0: fully transparent source, dst unchanged
            }
        });
}

/// Blit a source clip directly onto an existing canvas, centered and aspect-ratio
/// preserved (letterboxed). Only writes to the clip's actual pixel area — letterbox
/// regions of the canvas are untouched. This avoids creating an intermediate
/// canvas-sized buffer and prevents transparent letterbox pixels from erasing
/// background content.
pub fn blit_onto_canvas(clip: &FrameBuffer, canvas: &mut FrameBuffer) {
    if clip.width == 0 || clip.height == 0 || canvas.width == 0 || canvas.height == 0 {
        return;
    }

    // Fast path: dimensions match exactly — copy all data
    if clip.width == canvas.width && clip.height == canvas.height {
        canvas.data.copy_from_slice(&clip.data);
        return;
    }

    // Compute fit: scale clip to fit canvas, preserving aspect ratio
    let scale_x = canvas.width as f64 / clip.width as f64;
    let scale_y = canvas.height as f64 / clip.height as f64;
    let scale = scale_x.min(scale_y);

    let dst_w = (clip.width as f64 * scale).round() as u32;
    let dst_h = (clip.height as f64 * scale).round() as u32;

    if dst_w == 0 || dst_h == 0 {
        return;
    }

    // Center on canvas
    let offset_x = (canvas.width.saturating_sub(dst_w)) / 2;
    let offset_y = (canvas.height.saturating_sub(dst_h)) / 2;

    let src_stride = clip.width as usize * 4;
    let dst_stride = canvas.width as usize * 4;

    // Nearest-neighbor blit using integer math
    for dy in 0..dst_h {
        let cy = offset_y + dy;
        if cy >= canvas.height {
            break;
        }
        let sy =
            ((dy as u64 * clip.height as u64) / dst_h as u64).min(clip.height as u64 - 1) as u32;
        let dst_row_offset = cy as usize * dst_stride;
        let src_row_offset = sy as usize * src_stride;

        for dx in 0..dst_w {
            let cx = offset_x + dx;
            if cx >= canvas.width {
                break;
            }
            let sx = ((dx as u64 * clip.width as u64) / dst_w as u64)
                .min(clip.width as u64 - 1) as u32;
            let si = src_row_offset + sx as usize * 4;
            let di = dst_row_offset + cx as usize * 4;
            canvas.data[di..di + 4].copy_from_slice(&clip.data[si..si + 4]);
        }
    }
}

/// Direct opaque overwrite: copy src onto dst. Both must have the same dimensions.
/// Used when the effect pipeline guarantees no transparency was produced (common case).
pub fn composite_opaque(src: &FrameBuffer, dst: &mut FrameBuffer) {
    assert_eq!(src.width, dst.width);
    assert_eq!(src.height, dst.height);
    dst.data.copy_from_slice(&src.data);
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_ctx() -> EffectContext {
        EffectContext {
            time_secs: 0.0,
            frame_number: 0,
            fps: 30.0,
        }
    }

    // --- FrameBuffer tests ---

    #[test]
    fn test_framebuffer_new() {
        let fb = FrameBuffer::new(4, 3);
        assert_eq!(fb.width, 4);
        assert_eq!(fb.height, 3);
        assert_eq!(fb.data.len(), 4 * 3 * 4);
        // All zeros (transparent black)
        assert!(fb.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_framebuffer_pixel_access() {
        let mut fb = FrameBuffer::new(4, 4);
        let px = fb.pixel_mut(2, 1);
        px[0] = 255;
        px[1] = 128;
        px[2] = 64;
        px[3] = 255;

        let px = fb.pixel(2, 1);
        assert_eq!(px, &[255, 128, 64, 255]);
    }

    #[test]
    fn test_framebuffer_from_rgba_vec() {
        let data = vec![255, 0, 0, 255, 0, 255, 0, 255]; // 2 pixels
        let fb = FrameBuffer::from_rgba_vec(2, 1, data);
        assert_eq!(fb.pixel(0, 0), &[255, 0, 0, 255]);
        assert_eq!(fb.pixel(1, 0), &[0, 255, 0, 255]);
    }

    #[test]
    #[should_panic(expected = "RGBA data length")]
    fn test_framebuffer_from_rgba_vec_wrong_size() {
        FrameBuffer::from_rgba_vec(2, 2, vec![0; 10]); // should be 16
    }

    // --- Transform effect tests ---

    #[test]
    fn test_transform_identity() {
        let effect = TransformEffect;
        let params = vec![
            ("x_offset".to_string(), ParameterValue::Float(0.0)),
            ("y_offset".to_string(), ParameterValue::Float(0.0)),
        ];
        assert!(effect.is_identity(&params));
    }

    #[test]
    fn test_transform_shift_right() {
        let effect = TransformEffect;
        // 4x2 image, red pixel at (0,0), green at (1,0)
        let mut fb = FrameBuffer::new(4, 2);
        fb.pixel_mut(0, 0).copy_from_slice(&[255, 0, 0, 255]);
        fb.pixel_mut(1, 0).copy_from_slice(&[0, 255, 0, 255]);

        let params = vec![
            ("x_offset".to_string(), ParameterValue::Float(2.0)),
            ("y_offset".to_string(), ParameterValue::Float(0.0)),
        ];
        let result = effect.process(fb, &params, &dummy_ctx());

        // Red should now be at (2,0), green at (3,0)
        assert_eq!(result.pixel(2, 0), &[255, 0, 0, 255]);
        assert_eq!(result.pixel(3, 0), &[0, 255, 0, 255]);
        // Vacated area should be transparent
        assert_eq!(result.pixel(0, 0)[3], 0);
        assert_eq!(result.pixel(1, 0)[3], 0);
    }

    #[test]
    fn test_transform_shift_down() {
        let effect = TransformEffect;
        let mut fb = FrameBuffer::new(2, 4);
        fb.pixel_mut(0, 0).copy_from_slice(&[255, 0, 0, 255]);

        let params = vec![
            ("x_offset".to_string(), ParameterValue::Float(0.0)),
            ("y_offset".to_string(), ParameterValue::Float(2.0)),
        ];
        let result = effect.process(fb, &params, &dummy_ctx());

        assert_eq!(result.pixel(0, 2), &[255, 0, 0, 255]);
        assert_eq!(result.pixel(0, 0)[3], 0);
    }

    #[test]
    fn test_transform_shift_out_of_bounds() {
        let effect = TransformEffect;
        let mut fb = FrameBuffer::new(2, 2);
        fb.pixel_mut(0, 0).copy_from_slice(&[255, 0, 0, 255]);

        // Shift completely off screen
        let params = vec![
            ("x_offset".to_string(), ParameterValue::Float(10.0)),
            ("y_offset".to_string(), ParameterValue::Float(0.0)),
        ];
        let result = effect.process(fb, &params, &dummy_ctx());
        // All transparent
        assert!(result.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_transform_negative_shift() {
        let effect = TransformEffect;
        let mut fb = FrameBuffer::new(4, 2);
        fb.pixel_mut(3, 0).copy_from_slice(&[255, 0, 0, 255]);

        let params = vec![
            ("x_offset".to_string(), ParameterValue::Float(-2.0)),
            ("y_offset".to_string(), ParameterValue::Float(0.0)),
        ];
        let result = effect.process(fb, &params, &dummy_ctx());

        // Red pixel should move from (3,0) to (1,0)
        assert_eq!(result.pixel(1, 0), &[255, 0, 0, 255]);
    }

    #[test]
    fn test_transform_may_produce_transparency() {
        assert!(TransformEffect.may_produce_transparency());
    }

    // --- Grayscale effect tests ---

    #[test]
    fn test_grayscale_pure_red() {
        let effect = GrayscaleEffect;
        let fb = FrameBuffer::from_rgba_vec(1, 1, vec![255, 0, 0, 255]);
        let result = effect.process(fb, &[], &dummy_ctx());
        // L = 0.299 * 255 = 76.245 → 76
        let l = result.pixel(0, 0);
        assert_eq!(l[0], 76);
        assert_eq!(l[1], 76);
        assert_eq!(l[2], 76);
        assert_eq!(l[3], 255); // alpha preserved
    }

    #[test]
    fn test_grayscale_pure_green() {
        let effect = GrayscaleEffect;
        let fb = FrameBuffer::from_rgba_vec(1, 1, vec![0, 255, 0, 255]);
        let result = effect.process(fb, &[], &dummy_ctx());
        // L = 0.587 * 255 = 149.685 → 150
        let l = result.pixel(0, 0)[0];
        assert_eq!(l, 150);
    }

    #[test]
    fn test_grayscale_preserves_alpha() {
        let effect = GrayscaleEffect;
        let fb = FrameBuffer::from_rgba_vec(1, 1, vec![100, 150, 200, 128]);
        let result = effect.process(fb, &[], &dummy_ctx());
        assert_eq!(result.pixel(0, 0)[3], 128);
    }

    #[test]
    fn test_grayscale_is_never_identity() {
        let effect = GrayscaleEffect;
        assert!(!effect.is_identity(&[]));
        assert!(!effect.may_produce_transparency());
    }

    // --- Brightness effect tests ---

    #[test]
    fn test_brightness_zero_is_identity() {
        let effect = BrightnessEffect;
        let params = vec![("brightness".to_string(), ParameterValue::Float(0.0))];
        assert!(effect.is_identity(&params));
        assert!(!effect.may_produce_transparency());

        let fb = FrameBuffer::from_rgba_vec(1, 1, vec![100, 150, 200, 255]);
        let expected = fb.pixel(0, 0).to_vec();
        let result = effect.process(fb, &params, &dummy_ctx());
        assert_eq!(result.pixel(0, 0), &expected[..]);
    }

    #[test]
    fn test_brightness_positive() {
        let effect = BrightnessEffect;
        let params = vec![("brightness".to_string(), ParameterValue::Float(0.5))];
        let fb = FrameBuffer::from_rgba_vec(1, 1, vec![100, 150, 200, 255]);
        let result = effect.process(fb, &params, &dummy_ctx());
        // shift = 0.5 * 255 = 128
        assert_eq!(result.pixel(0, 0), &[228, 255, 255, 255]); // 200+128=328→255 clamped
    }

    #[test]
    fn test_brightness_negative() {
        let effect = BrightnessEffect;
        let params = vec![("brightness".to_string(), ParameterValue::Float(-0.5))];
        let fb = FrameBuffer::from_rgba_vec(1, 1, vec![100, 150, 200, 255]);
        let result = effect.process(fb, &params, &dummy_ctx());
        // shift = -128
        assert_eq!(result.pixel(0, 0), &[0, 22, 72, 255]); // 100-128=-28→0 clamped
    }

    #[test]
    fn test_brightness_preserves_alpha() {
        let effect = BrightnessEffect;
        let params = vec![("brightness".to_string(), ParameterValue::Float(0.5))];
        let fb = FrameBuffer::from_rgba_vec(1, 1, vec![100, 150, 200, 128]);
        let result = effect.process(fb, &params, &dummy_ctx());
        assert_eq!(result.pixel(0, 0)[3], 128);
    }

    // --- Opacity effect tests ---

    #[test]
    fn test_opacity_one_is_identity() {
        let effect = OpacityEffect;
        let params = vec![("opacity".to_string(), ParameterValue::Float(1.0))];
        assert!(effect.is_identity(&params));
    }

    #[test]
    fn test_opacity_zero() {
        let effect = OpacityEffect;
        let params = vec![("opacity".to_string(), ParameterValue::Float(0.0))];
        let fb = FrameBuffer::from_rgba_vec(1, 1, vec![255, 0, 0, 255]);
        let result = effect.process(fb, &params, &dummy_ctx());
        assert_eq!(result.pixel(0, 0)[3], 0);
        // RGB preserved
        assert_eq!(result.pixel(0, 0)[0], 255);
    }

    #[test]
    fn test_opacity_half() {
        let effect = OpacityEffect;
        let params = vec![("opacity".to_string(), ParameterValue::Float(0.5))];
        let fb = FrameBuffer::from_rgba_vec(1, 1, vec![255, 0, 0, 200]);
        let result = effect.process(fb, &params, &dummy_ctx());
        assert_eq!(result.pixel(0, 0)[3], 100); // 200 * 0.5
        assert!(effect.may_produce_transparency());
    }

    // --- EffectRegistry tests ---

    #[test]
    fn test_registry_builtins() {
        let registry = EffectRegistry::with_builtins();
        assert!(registry.get(&EffectType::Transform).is_some());
        assert!(registry.get(&EffectType::Grayscale).is_some());
        assert!(registry.get(&EffectType::Brightness).is_some());
        assert!(registry.get(&EffectType::Opacity).is_some());
    }

    // --- Pipeline tests ---

    #[test]
    fn test_pipeline_empty_effects() {
        let registry = EffectRegistry::with_builtins();
        let clip = FrameBuffer::from_rgba_vec(2, 2, vec![
            255, 0, 0, 255,   0, 255, 0, 255,
            0, 0, 255, 255,   255, 255, 0, 255,
        ]);
        let result = run_effect_pipeline(clip, 4, 4, &[], &registry, &dummy_ctx());
        assert_eq!(result.frame.width, 4);
        assert_eq!(result.frame.height, 4);
        assert!(!result.may_have_transparency);
        // Clip should be centered on canvas, non-zero pixels exist
        assert!(result.frame.data.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_pipeline_ordering_matters() {
        let registry = EffectRegistry::with_builtins();
        // Pure red — grayscale and brightness interact differently depending on order
        let clip = FrameBuffer::from_rgba_vec(1, 1, vec![255, 0, 0, 255]);

        // Order 1: Brightness then Grayscale
        let mut brightness = EffectInstance::new(EffectType::Brightness);
        brightness.set_float("brightness", 0.5);
        let grayscale = EffectInstance::new(EffectType::Grayscale);
        let effects_1 = vec![brightness.clone(), grayscale.clone()];
        let result_1 = run_effect_pipeline(clip.clone(), 1, 1, &effects_1, &registry, &dummy_ctx());

        // Order 2: Grayscale then Brightness
        let effects_2 = vec![grayscale, brightness];
        let result_2 = run_effect_pipeline(clip, 1, 1, &effects_2, &registry, &dummy_ctx());

        // Results should differ (brightness applied to color vs gray gives different grayscale)
        assert_ne!(result_1.frame.data, result_2.frame.data);
    }

    #[test]
    fn test_pipeline_identity_skip() {
        let registry = EffectRegistry::with_builtins();
        let clip = FrameBuffer::from_rgba_vec(2, 2, vec![
            100, 150, 200, 255,   100, 150, 200, 255,
            100, 150, 200, 255,   100, 150, 200, 255,
        ]);

        // Transform at (0,0) should be skipped as identity
        let transform = EffectInstance::new(EffectType::Transform);
        let result_with = run_effect_pipeline(clip.clone(), 2, 2, &[transform], &registry, &dummy_ctx());

        let result_without = run_effect_pipeline(clip, 2, 2, &[], &registry, &dummy_ctx());
        assert_eq!(result_with.frame.data, result_without.frame.data);
    }

    #[test]
    fn test_pipeline_transparency_tracking() {
        let registry = EffectRegistry::with_builtins();
        let clip = FrameBuffer::from_rgba_vec(1, 1, vec![255, 0, 0, 255]);

        // Grayscale alone should not report transparency
        let effects = vec![EffectInstance::new(EffectType::Grayscale)];
        let result = run_effect_pipeline(clip.clone(), 1, 1, &effects, &registry, &dummy_ctx());
        assert!(!result.may_have_transparency);

        // Opacity (non-identity) should report transparency
        let mut opacity = EffectInstance::new(EffectType::Opacity);
        opacity.set_float("opacity", 0.5);
        let effects = vec![opacity];
        let result = run_effect_pipeline(clip.clone(), 1, 1, &effects, &registry, &dummy_ctx());
        assert!(result.may_have_transparency);

        // Opacity at identity (1.0) should NOT report transparency
        let opacity_id = EffectInstance::new(EffectType::Opacity);
        let effects = vec![opacity_id];
        let result = run_effect_pipeline(clip, 1, 1, &effects, &registry, &dummy_ctx());
        assert!(!result.may_have_transparency);
    }

    #[test]
    fn test_blit_clip_to_canvas_same_dimensions() {
        // Fast path: dimensions match exactly
        let clip = FrameBuffer::from_rgba_vec(4, 2, vec![
            255, 0, 0, 255,   0, 255, 0, 255,   0, 0, 255, 255,   128, 128, 128, 255,
            10, 20, 30, 255,   40, 50, 60, 255,   70, 80, 90, 255,   100, 110, 120, 255,
        ]);
        let data_copy = clip.data.clone();
        let canvas = blit_clip_to_canvas(clip, 4, 2);
        assert_eq!(canvas.width, 4);
        assert_eq!(canvas.height, 2);
        assert_eq!(canvas.data, data_copy);
    }

    #[test]
    fn test_blit_clip_to_canvas_centered() {
        // 2x1 clip on 4x2 canvas: should be centered
        let clip = FrameBuffer::from_rgba_vec(2, 1, vec![
            255, 0, 0, 255,   0, 255, 0, 255,
        ]);
        let canvas = blit_clip_to_canvas(clip, 4, 2);
        assert_eq!(canvas.width, 4);
        assert_eq!(canvas.height, 2);
        // Clip should be scaled to fit (2x1 → 4x2 preserving aspect: 4x2)
        // Center pixel should have content
        assert!(canvas.data.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_blit_clip_to_canvas_aspect_preserved() {
        // 4x2 clip on 4x4 canvas: clip is wider, so height is letterboxed
        let mut clip = FrameBuffer::new(4, 2);
        for y in 0..2 {
            for x in 0..4 {
                clip.pixel_mut(x, y).copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let canvas = blit_clip_to_canvas(clip, 4, 4);
        // Top and bottom rows should be transparent (letterboxed)
        assert_eq!(canvas.pixel(0, 0)[3], 0);
        assert_eq!(canvas.pixel(0, 3)[3], 0);
        // Middle rows should have content
        assert_eq!(canvas.pixel(0, 1)[3], 255);
        assert_eq!(canvas.pixel(0, 2)[3], 255);
    }

    // --- Alpha composite tests ---

    #[test]
    fn test_alpha_composite_opaque_over_opaque() {
        let src = FrameBuffer::from_rgba_vec(1, 1, vec![255, 0, 0, 255]);
        let mut dst = FrameBuffer::from_rgba_vec(1, 1, vec![0, 255, 0, 255]);
        alpha_composite_rgba(&src, &mut dst);
        assert_eq!(dst.pixel(0, 0), &[255, 0, 0, 255]);
    }

    #[test]
    fn test_alpha_composite_transparent_over_opaque() {
        let src = FrameBuffer::from_rgba_vec(1, 1, vec![255, 0, 0, 0]);
        let mut dst = FrameBuffer::from_rgba_vec(1, 1, vec![0, 255, 0, 255]);
        alpha_composite_rgba(&src, &mut dst);
        assert_eq!(dst.pixel(0, 0), &[0, 255, 0, 255]);
    }

    #[test]
    fn test_alpha_composite_half_transparent() {
        let src = FrameBuffer::from_rgba_vec(1, 1, vec![255, 0, 0, 128]);
        let mut dst = FrameBuffer::from_rgba_vec(1, 1, vec![0, 0, 255, 255]);
        alpha_composite_rgba(&src, &mut dst);
        let px = dst.pixel(0, 0);
        // src alpha ~= 0.502, so red blended with blue
        // out_a = 0.502 + 1.0 * 0.498 = 1.0
        // out_r = (255 * 0.502 + 0 * 1.0 * 0.498) / 1.0 ≈ 128
        // out_b = (0 * 0.502 + 255 * 1.0 * 0.498) / 1.0 ≈ 127
        assert!((px[0] as i32 - 128).unsigned_abs() <= 2);
        assert_eq!(px[1], 0);
        assert!((px[2] as i32 - 127).unsigned_abs() <= 2);
        assert_eq!(px[3], 255);
    }

    #[test]
    fn test_alpha_composite_onto_transparent() {
        let src = FrameBuffer::from_rgba_vec(1, 1, vec![255, 0, 0, 128]);
        let mut dst = FrameBuffer::from_rgba_vec(1, 1, vec![0, 0, 0, 0]);
        alpha_composite_rgba(&src, &mut dst);
        let px = dst.pixel(0, 0);
        assert_eq!(px[0], 255);
        assert_eq!(px[1], 0);
        assert_eq!(px[2], 0);
        assert_eq!(px[3], 128);
    }

    // --- blit_onto_canvas tests ---

    #[test]
    fn test_blit_onto_canvas_same_dimensions() {
        let clip = FrameBuffer::from_rgba_vec(4, 2, vec![
            255, 0, 0, 255,   0, 255, 0, 255,   0, 0, 255, 255,   128, 128, 128, 255,
            10, 20, 30, 255,   40, 50, 60, 255,   70, 80, 90, 255,   100, 110, 120, 255,
        ]);
        let mut canvas = FrameBuffer::new(4, 2);
        blit_onto_canvas(&clip, &mut canvas);
        assert_eq!(canvas.data, clip.data);
    }

    #[test]
    fn test_blit_onto_canvas_preserves_letterbox() {
        // 4x2 clip on 4x4 canvas: should be centered, letterbox rows untouched
        let mut clip = FrameBuffer::new(4, 2);
        for y in 0..2 {
            for x in 0..4 {
                clip.pixel_mut(x, y).copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        // Pre-fill canvas with green (simulating a background clip)
        let mut canvas = FrameBuffer::new(4, 4);
        for pixel in canvas.data.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0, 255, 0, 255]);
        }
        blit_onto_canvas(&clip, &mut canvas);
        // Letterbox rows (top/bottom) should remain green (untouched)
        assert_eq!(canvas.pixel(0, 0), &[0, 255, 0, 255]);
        assert_eq!(canvas.pixel(0, 3), &[0, 255, 0, 255]);
        // Middle rows should be red (from clip)
        assert_eq!(canvas.pixel(0, 1), &[255, 0, 0, 255]);
        assert_eq!(canvas.pixel(0, 2), &[255, 0, 0, 255]);
    }

    #[test]
    fn test_blit_onto_canvas_zero_dimensions() {
        let clip = FrameBuffer::new(0, 0);
        let mut canvas = FrameBuffer::new(4, 4);
        blit_onto_canvas(&clip, &mut canvas);
        // Canvas unchanged
        assert!(canvas.data.iter().all(|&b| b == 0));
    }

    // --- fills_canvas tracking tests ---

    #[test]
    fn test_pipeline_fills_canvas_true_when_matching() {
        let registry = EffectRegistry::with_builtins();
        let clip = FrameBuffer::from_rgba_vec(4, 4, vec![255; 4 * 4 * 4]);
        let result = run_effect_pipeline(clip, 4, 4, &[], &registry, &dummy_ctx());
        assert!(result.fills_canvas);
    }

    #[test]
    fn test_pipeline_fills_canvas_false_when_different() {
        let registry = EffectRegistry::with_builtins();
        let clip = FrameBuffer::from_rgba_vec(2, 1, vec![255; 2 * 1 * 4]);
        let result = run_effect_pipeline(clip, 4, 4, &[], &registry, &dummy_ctx());
        assert!(!result.fills_canvas);
    }

    #[test]
    fn test_pipeline_smart_composite_opaque_effect() {
        // Grayscale on matching dimensions: fills_canvas=true, may_have_transparency=false
        // → composite_opaque is safe
        let registry = EffectRegistry::with_builtins();
        let clip = FrameBuffer::from_rgba_vec(2, 2, vec![
            100, 150, 200, 255,   100, 150, 200, 255,
            100, 150, 200, 255,   100, 150, 200, 255,
        ]);
        let effects = vec![EffectInstance::new(EffectType::Grayscale)];
        let result = run_effect_pipeline(clip, 2, 2, &effects, &registry, &dummy_ctx());
        assert!(result.fills_canvas);
        assert!(!result.may_have_transparency);
    }

    #[test]
    fn test_pipeline_smart_composite_transparency_effect() {
        // Opacity < 1.0: fills_canvas=true, may_have_transparency=true
        // → must use alpha_composite_rgba
        let registry = EffectRegistry::with_builtins();
        let clip = FrameBuffer::from_rgba_vec(2, 2, vec![255; 2 * 2 * 4]);
        let mut opacity = EffectInstance::new(EffectType::Opacity);
        opacity.set_float("opacity", 0.5);
        let result = run_effect_pipeline(clip, 2, 2, &[opacity], &registry, &dummy_ctx());
        assert!(result.fills_canvas);
        assert!(result.may_have_transparency);
    }
}
