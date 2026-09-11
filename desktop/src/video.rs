//! The picture on the GPU: the framebuffer as a texture, the two off-screen
//! preparation passes, the present shader with one branch per look, and the
//! preview readback the settings panel shows. Ported from ScreenRenderer.kt.
//!
//! The game view draws through it every frame and the settings preview draws
//! through it into an off-screen buffer, so what the preview shows is what the
//! game will look like rather than a second implementation that drifts.

use crate::picture::{
    layout, trim_fraction, visible_height, Aspect, Look, Source, Viewport, HEIGHT, WIDTH,
};
use glow::HasContext;
use std::sync::Arc;

/// Doublings of the picture before a smoothing look is presented. Two is 4×.
pub const SMOOTH_STEPS: usize = 2;

/// Samples per console pixel in the signal pass. The subcarrier runs at two
/// thirds of a cycle per pixel, so six samples is exactly one cycle - which is
/// what lets the luma filter cancel the carrier instead of merely attenuating
/// it. Four is the smallest number that keeps that whole and still resolves
/// the subcarrier.
pub const SUBSAMPLES: i32 = 4;

const QUAD_VERT: &str = include_str!("shaders/quad.vert");
const SMOOTH_FRAG: &str = include_str!("shaders/smooth.frag");
const COMPOSITE_FRAG: &str = include_str!("shaders/composite.frag");
const PRESENT_FRAG: &str = include_str!("shaders/present.frag");

struct Program {
    id: glow::Program,
}

pub struct Video {
    gl: Arc<glow::Context>,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    frame: glow::Texture,
    present: Program,
    smooth: Program,
    composite: Program,
    /// The source-pass chain. Two doublings of the framebuffer, each one a
    /// scale2x that rounds off the corners a NES pixel leaves behind. Both live
    /// in picture space rather than screen space, so they cost the same
    /// whatever the display is doing: 256x240 and 512x480 fragments against the
    /// millions a full-screen pass would have to cover.
    smooth_textures: [glow::Texture; SMOOTH_STEPS],
    smooth_buffers: [glow::Framebuffer; SMOOTH_STEPS],
    /// The composite pass. Encode and decode happen in the same shader, so the
    /// signal is never stored: an 8-bit intermediate would quantise it, and the
    /// window each output sample needs is small enough to re-derive.
    ntsc_texture: glow::Texture,
    ntsc_buffer: glow::Framebuffer,
    preview: Option<(glow::Framebuffer, glow::Texture, i32, i32)>,
    /// Frames drawn, for the phase that makes the dots crawl rather than sit.
    frames: u32,
}

// SAFETY, for every helper below: the caller must have the GL context current
// on this thread, which is the case for everything `Video` does — it is only
// ever driven from the thread that made the context.

unsafe fn compile(gl: &glow::Context, kind: u32, source: &str) -> Result<glow::Shader, String> {
    let shader = gl.create_shader(kind)?;
    gl.shader_source(shader, source);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        return Err(format!("shader: {}", gl.get_shader_info_log(shader)));
    }
    Ok(shader)
}

unsafe fn link(gl: &glow::Context, fragment: &str) -> Result<Program, String> {
    let v = compile(gl, glow::VERTEX_SHADER, QUAD_VERT)?;
    let f = compile(gl, glow::FRAGMENT_SHADER, fragment)?;
    let id = gl.create_program()?;
    gl.attach_shader(id, v);
    gl.attach_shader(id, f);
    gl.link_program(id);
    if !gl.get_program_link_status(id) {
        return Err(format!("program: {}", gl.get_program_info_log(id)));
    }
    gl.delete_shader(v);
    gl.delete_shader(f);
    Ok(Program { id })
}

/// An empty RGBA texture. Every step of the smoothing chain but the last is
/// read by scale2x, which compares texels for equality and so must never see a
/// filtered sample; the last is the only texture the picture is ever magnified
/// from, so it carries mipmaps and the Cartoon glow reads one of the small
/// levels instead of spending taps on a blur.
unsafe fn texture(
    gl: &glow::Context,
    width: i32,
    height: i32,
    linear: bool,
    mipmap: bool,
) -> Result<glow::Texture, String> {
    let t = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(t));
    let min = if mipmap {
        glow::LINEAR_MIPMAP_LINEAR
    } else if linear {
        glow::LINEAR
    } else {
        glow::NEAREST
    };
    let mag = if linear || mipmap {
        glow::LINEAR
    } else {
        glow::NEAREST
    };
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, min as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, mag as i32);
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_S,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_T,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        width,
        height,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
    );
    if mipmap {
        gl.generate_mipmap(glow::TEXTURE_2D);
    }
    Ok(t)
}

unsafe fn framebuffer(gl: &glow::Context, t: glow::Texture) -> Result<glow::Framebuffer, String> {
    let f = gl.create_framebuffer()?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
    gl.framebuffer_texture_2d(
        glow::FRAMEBUFFER,
        glow::COLOR_ATTACHMENT0,
        glow::TEXTURE_2D,
        Some(t),
        0,
    );
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    if status != glow::FRAMEBUFFER_COMPLETE {
        return Err(format!("framebuffer incomplete: {status}"));
    }
    Ok(f)
}

/// The VBO takes bytes and the quad is sixteen floats, in the machine's own
/// order, which is what GL reads them back as.
fn vertex_bytes(vertices: &[f32; 16]) -> Vec<u8> {
    vertices.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

impl Video {
    pub fn new(gl: Arc<glow::Context>) -> Result<Video, String> {
        unsafe {
            let vao = gl.create_vertex_array()?;
            let vbo = gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_size(glow::ARRAY_BUFFER, 16 * 4, glow::DYNAMIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);
            gl.bind_vertex_array(None);
            let frame = texture(&gl, WIDTH as i32, HEIGHT as i32, false, false)?;
            let mut smooth_textures = Vec::new();
            let mut smooth_buffers = Vec::new();
            for step in 0..SMOOTH_STEPS {
                let scale = 2 << step;
                let last = step == SMOOTH_STEPS - 1;
                let t = texture(&gl, WIDTH as i32 * scale, HEIGHT as i32 * scale, last, last)?;
                smooth_buffers.push(framebuffer(&gl, t)?);
                smooth_textures.push(t);
            }
            // Where the decoded picture lands: four samples to a console pixel,
            // which is what the subcarrier needs to be resolvable, and full
            // height because composite blurs along a scanline and not across.
            let ntsc_texture = texture(&gl, WIDTH as i32 * SUBSAMPLES, HEIGHT as i32, true, false)?;
            let ntsc_buffer = framebuffer(&gl, ntsc_texture)?;
            Ok(Video {
                present: link(&gl, PRESENT_FRAG)?,
                smooth: link(&gl, SMOOTH_FRAG)?,
                composite: link(&gl, COMPOSITE_FRAG)?,
                gl,
                vao,
                vbo,
                frame,
                smooth_textures: smooth_textures
                    .try_into()
                    .map_err(|_| "smoothing chain is the wrong length")?,
                smooth_buffers: smooth_buffers
                    .try_into()
                    .map_err(|_| "smoothing chain is the wrong length")?,
                ntsc_texture,
                ntsc_buffer,
                preview: None,
                frames: 0,
            })
        }
    }

    /// Uploads one 256×240 RGBA frame. The framebuffer is always uploaded
    /// whole; trimming moves the window the quad samples, which keeps this one
    /// unconditional call.
    pub fn upload(&mut self, rgba: &[u8]) {
        unsafe {
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.frame));
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                WIDTH as i32,
                HEIGHT as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(rgba)),
            );
        }
    }

    /// Texture rows run top-down while the quad runs bottom-up, so the top of
    /// the picture is v = trim and the bottom is 1 - trim.
    unsafe fn quad(&self, trim: bool) {
        let edge = trim_fraction(trim);
        let vertices: [f32; 16] = [
            -1.0,
            -1.0,
            0.0,
            1.0 - edge,
            1.0,
            -1.0,
            1.0,
            1.0 - edge,
            -1.0,
            1.0,
            0.0,
            edge,
            1.0,
            1.0,
            1.0,
            edge,
        ];
        self.gl.bind_vertex_array(Some(self.vao));
        self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        self.gl
            .buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, &vertex_bytes(&vertices));
        self.gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
    }

    /// Runs the smoothing chain and hands back the texture the picture should
    /// be presented from. Each step doubles the picture and rounds the corners
    /// a single NES pixel leaves behind; two steps is enough that the final
    /// magnification to the display has curves to stretch rather than stairs.
    ///
    /// The framebuffer and viewport to return to are passed in rather than
    /// asked of GL, because this runs both on the way to the screen and on the
    /// way into the settings preview.
    unsafe fn smooth_pass(
        &mut self,
        bound: Option<glow::Framebuffer>,
        viewport: [i32; 4],
    ) -> glow::Texture {
        let gl = &self.gl;
        gl.use_program(Some(self.smooth.id));
        gl.uniform_1_i32(gl.get_uniform_location(self.smooth.id, "src").as_ref(), 0);
        let mut source = self.frame;
        for step in 0..SMOOTH_STEPS {
            let scale = 1 << step;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.smooth_buffers[step]));
            gl.viewport(0, 0, WIDTH as i32 * scale * 2, HEIGHT as i32 * scale * 2);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(source));
            gl.uniform_2_i32(
                gl.get_uniform_location(self.smooth.id, "srcSize").as_ref(),
                WIDTH as i32 * scale,
                HEIGHT as i32 * scale,
            );
            self.quad(false);
            source = self.smooth_textures[step];
        }
        // Let go of the buffer this texture is attached to before rebuilding
        // its mip levels, or the read and the write are the same memory.
        gl.bind_framebuffer(glow::FRAMEBUFFER, bound);
        gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        gl.bind_texture(glow::TEXTURE_2D, Some(source));
        gl.generate_mipmap(glow::TEXTURE_2D);
        source
    }

    /// Encodes the framebuffer to a composite signal and decodes it back, which
    /// is what makes a one-pixel dither read as a colour rather than a
    /// checkerboard. Returns the texture to present from.
    unsafe fn composite_pass(
        &mut self,
        bound: Option<glow::Framebuffer>,
        viewport: [i32; 4],
    ) -> glow::Texture {
        let gl = &self.gl;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.ntsc_buffer));
        gl.viewport(0, 0, WIDTH as i32 * SUBSAMPLES, HEIGHT as i32);
        gl.use_program(Some(self.composite.id));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.frame));
        gl.uniform_1_i32(
            gl.get_uniform_location(self.composite.id, "src").as_ref(),
            0,
        );
        gl.uniform_2_i32(
            gl.get_uniform_location(self.composite.id, "srcSize")
                .as_ref(),
            WIDTH as i32,
            HEIGHT as i32,
        );
        // A frame advances the subcarrier by a third of a cycle, so the pattern
        // repeats every third frame. Kept as a whole number of thirds rather
        // than a growing float, which would lose its low bits inside an hour.
        gl.uniform_1_f32(
            gl.get_uniform_location(self.composite.id, "framePhase")
                .as_ref(),
            (self.frames % 3) as f32 / 3.0,
        );
        self.quad(false);
        gl.bind_framebuffer(glow::FRAMEBUFFER, bound);
        gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        self.ntsc_texture
    }

    /// Draws the uploaded frame through `look` into the bound framebuffer,
    /// letterboxed by `layout`. `bound` is the framebuffer to return to after
    /// any preparation pass (None for the window).
    unsafe fn draw_into(
        &mut self,
        bound: Option<glow::Framebuffer>,
        area: Viewport,
        look: Look,
        aspect: Aspect,
        trim: bool,
    ) {
        let mut view = layout(area.width, area.height, aspect, trim);
        view.x += area.x;
        view.y += area.y;
        let viewport = [view.x, view.y, view.width, view.height];
        // A look that wants its picture prepared gets that done off-screen
        // first, and is then presented from the result instead of from the
        // framebuffer.
        self.frames = self.frames.wrapping_add(1);
        let present = match look.source() {
            Source::Direct => self.frame,
            Source::Smoothed => self.smooth_pass(bound, viewport),
            Source::Composite => self.composite_pass(bound, viewport),
        };
        let gl = &self.gl;
        gl.viewport(view.x, view.y, view.width, view.height);
        gl.use_program(Some(self.present.id));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(present));
        gl.uniform_1_i32(
            gl.get_uniform_location(self.present.id, "screen").as_ref(),
            0,
        );
        gl.uniform_1_i32(
            gl.get_uniform_location(self.present.id, "kind").as_ref(),
            look.id(),
        );
        gl.uniform_1_f32(
            gl.get_uniform_location(self.present.id, "cols").as_ref(),
            WIDTH as f32,
        );
        gl.uniform_1_f32(
            gl.get_uniform_location(self.present.id, "rows").as_ref(),
            visible_height(trim) as f32,
        );
        // Where the visible rows sit inside the whole framebuffer, so effects
        // that work in picture space stay put when the edges are trimmed.
        let edge = trim_fraction(trim);
        gl.uniform_2_f32(
            gl.get_uniform_location(self.present.id, "window").as_ref(),
            edge,
            1.0 - 2.0 * edge,
        );
        self.quad(trim);
    }

    /// Draws the uploaded frame into the window, fitted inside `area` (device
    /// pixels, bottom-left origin). The caller clears the screen first.
    pub fn draw(&mut self, area: Viewport, look: Look, aspect: Aspect, trim: bool) {
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.disable(glow::BLEND);
            self.draw_into(None, area, look, aspect, trim);
        }
    }

    /// One still through the real pipeline, read back top-down, so the settings
    /// panel can show the real thing rather than a second implementation.
    /// Leaves the default framebuffer bound; the caller puts its own viewport
    /// back.
    pub fn preview(
        &mut self,
        rgba: &[u8],
        look: Look,
        aspect: Aspect,
        trim: bool,
        width: i32,
        height: i32,
    ) -> Result<Vec<u8>, String> {
        unsafe {
            if self
                .preview
                .is_none_or(|(_, _, w, h)| (w, h) != (width, height))
            {
                if let Some((f, t, _, _)) = self.preview.take() {
                    self.gl.delete_framebuffer(f);
                    self.gl.delete_texture(t);
                }
                let t = texture(&self.gl, width, height, true, false)?;
                let f = framebuffer(&self.gl, t)?;
                self.preview = Some((f, t, width, height));
            }
            let (f, _, _, _) = self.preview.unwrap();
            self.upload(rgba);
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.disable(glow::BLEND);
            self.gl.viewport(0, 0, width, height);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            self.draw_into(
                Some(f),
                Viewport {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                look,
                aspect,
                trim,
            );
            let mut out = vec![0u8; (width * height * 4) as usize];
            self.gl.read_pixels(
                0,
                0,
                width,
                height,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut out)),
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            // GL reads bottom-up.
            let row = (width * 4) as usize;
            let mut flipped = vec![0u8; out.len()];
            for y in 0..height as usize {
                flipped[y * row..(y + 1) * row].copy_from_slice(
                    &out[(height as usize - 1 - y) * row..(height as usize - y) * row],
                );
            }
            Ok(flipped)
        }
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.gl.delete_program(self.present.id);
            self.gl.delete_program(self.smooth.id);
            self.gl.delete_program(self.composite.id);
            self.gl.delete_texture(self.frame);
            for t in self.smooth_textures {
                self.gl.delete_texture(t);
            }
            for f in self.smooth_buffers {
                self.gl.delete_framebuffer(f);
            }
            self.gl.delete_texture(self.ntsc_texture);
            self.gl.delete_framebuffer(self.ntsc_buffer);
            if let Some((f, t, _, _)) = self.preview.take() {
                self.gl.delete_framebuffer(f);
                self.gl.delete_texture(t);
            }
            self.gl.delete_buffer(self.vbo);
            self.gl.delete_vertex_array(self.vao);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_shaders_are_desktop_glsl_with_the_android_constants() {
        for s in [
            super::SMOOTH_FRAG,
            super::COMPOSITE_FRAG,
            super::PRESENT_FRAG,
            super::QUAD_VERT,
        ] {
            assert!(s.starts_with("#version 330 core"));
            assert!(!s.contains("precision "));
        }
        assert!(super::COMPOSITE_FRAG.contains("const float SUBS = 4.0;"));
        assert!(super::PRESENT_FRAG.contains("textureLod(screen, toTexture(p), 4.0)"));
        assert!(
            super::PRESENT_FRAG.contains("kind == 10")
                || !super::PRESENT_FRAG.contains("kind == 9"),
            "Smooth and Composite take no branch"
        );
    }
}
