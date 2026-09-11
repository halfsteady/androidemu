//! The presentation geometry, the looks, and what the framebuffer's 64
//! indices mean in colour. Ported from Picture.kt, Filter.kt, Palette.kt and
//! SampleFrame.kt; the unit tests mirror the Kotlin ones.

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;
/// Rows hidden at the top and bottom when edges are trimmed.
pub const TRIM: i32 = 8;
/// The 2C02 emitted pixels 8/7 as wide as they were tall.
const PIXEL_ASPECT: f32 = 8.0 / 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Television,
    Hardware,
    Pixels,
}

impl Aspect {
    pub const ALL: [Aspect; 3] = [Aspect::Television, Aspect::Hardware, Aspect::Pixels];
    pub fn label(self) -> &'static str {
        match self {
            Aspect::Television => "4:3 television",
            Aspect::Hardware => "8:7 hardware",
            Aspect::Pixels => "Pixel-perfect",
        }
    }
    pub fn index(self) -> u32 {
        self as u32
    }
    pub fn from_index(i: u32) -> Aspect {
        Aspect::ALL
            .get(i as usize)
            .copied()
            .unwrap_or(Aspect::Television)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn visible_height(trim: bool) -> i32 {
    if trim {
        HEIGHT as i32 - 2 * TRIM
    } else {
        HEIGHT as i32
    }
}

pub fn trim_fraction(trim: bool) -> f32 {
    if trim {
        TRIM as f32 / HEIGHT as f32
    } else {
        0.0
    }
}

/// The largest presentation of the visible area that fits the surface, centred.
/// Television and hardware scale continuously; pixel-perfect drops to the next
/// whole multiple, so it leaves a wider border rather than resample.
pub fn layout(surface_width: i32, surface_height: i32, aspect: Aspect, trim: bool) -> Viewport {
    let visible = visible_height(trim);
    if surface_width <= 0 || surface_height <= 0 {
        return Viewport {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    }
    let (width, height) = match aspect {
        Aspect::Pixels => {
            let scale = (surface_width / WIDTH as i32)
                .min(surface_height / visible)
                .max(1);
            (WIDTH as i32 * scale, visible * scale)
        }
        _ => {
            let target = if aspect == Aspect::Television {
                4.0 / 3.0
            } else {
                WIDTH as f32 * PIXEL_ASPECT / visible as f32
            };
            let width = (surface_width as f32).min(surface_height as f32 * target);
            (width as i32, (width / target) as i32)
        }
    };
    Viewport {
        x: (surface_width - width) / 2,
        y: (surface_height - height) / 2,
        width,
        height,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Direct,
    Smoothed,
    Composite,
}

/// A look applied to the finished framebuffer. `id` is what gets saved, not
/// the position; a retired id is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Off,
    Scanlines,
    OldTv,
    DotMatrix,
    FourGreens,
    OldPhoto,
    Neon,
    Cartoon,
    Smooth,
    Composite,
}

impl Look {
    pub const ALL: [Look; 10] = [
        Look::Off,
        Look::Scanlines,
        Look::OldTv,
        Look::DotMatrix,
        Look::FourGreens,
        Look::OldPhoto,
        Look::Neon,
        Look::Cartoon,
        Look::Smooth,
        Look::Composite,
    ];
    /// 5 was "Black and white": Old photo with the tint thrown away. Retired.
    const RETIRED_GREY: i32 = 5;

    pub fn id(self) -> i32 {
        match self {
            Look::Off => 0,
            Look::Scanlines => 1,
            Look::OldTv => 2,
            Look::DotMatrix => 3,
            Look::FourGreens => 4,
            Look::OldPhoto => 6,
            Look::Neon => 7,
            Look::Cartoon => 8,
            Look::Smooth => 9,
            Look::Composite => 10,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Look::Off => "Off",
            Look::Scanlines => "Scanlines",
            Look::OldTv => "Old TV",
            Look::DotMatrix => "Dot matrix",
            Look::FourGreens => "Four greens",
            Look::OldPhoto => "Old photo",
            Look::Neon => "Neon",
            Look::Cartoon => "Cartoon",
            Look::Smooth => "Smooth",
            Look::Composite => "Composite",
        }
    }
    pub fn note(self) -> &'static str {
        match self {
            Look::Off => "The framebuffer exactly as the console drew it",
            Look::Scanlines => "A soft dark band between each row, the way a CRT drew them",
            Look::OldTv => "Scanlines, a curved tube, an aperture grille and a darkened edge",
            Look::DotMatrix => "A grid between the pixels, like a handheld's LCD",
            Look::FourGreens => "Everything remapped onto a handheld's four-shade screen",
            Look::OldPhoto => "Warm and faded",
            Look::Neon => "Bright things glow and the colour is turned all the way up",
            Look::Cartoon => "Smoothed into curves, inked, and painted in flat colour",
            Look::Smooth => "Stairsteps rounded into curves, with the colour left alone",
            Look::Composite => "Down an aerial lead: colours bleed and dithering turns solid",
        }
    }
    pub fn source(self) -> Source {
        match self {
            Look::Cartoon | Look::Smooth => Source::Smoothed,
            Look::Composite => Source::Composite,
            _ => Source::Direct,
        }
    }
    pub fn from_id(id: i32) -> Look {
        if id == Self::RETIRED_GREY {
            return Look::OldPhoto;
        }
        Look::ALL
            .into_iter()
            .find(|l| l.id() == id)
            .unwrap_or(Look::Off)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteChoice {
    Standard,
    Hardware,
    Soft,
    Vivid,
    File,
}

impl PaletteChoice {
    pub const ALL: [PaletteChoice; 5] = [
        PaletteChoice::Standard,
        PaletteChoice::Hardware,
        PaletteChoice::Soft,
        PaletteChoice::Vivid,
        PaletteChoice::File,
    ];
    pub fn id(self) -> i32 {
        match self {
            PaletteChoice::Standard => 0,
            PaletteChoice::Hardware => 1,
            PaletteChoice::Soft => 2,
            PaletteChoice::Vivid => 3,
            PaletteChoice::File => 5,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            PaletteChoice::Standard => "Standard",
            PaletteChoice::Hardware => "Hardware",
            PaletteChoice::Soft => "Soft",
            PaletteChoice::Vivid => "Vivid",
            PaletteChoice::File => "From a file",
        }
    }
    pub fn note(self) -> &'static str {
        match self {
            PaletteChoice::Standard => "The colours the app has always used",
            PaletteChoice::Hardware => "Decoded from what the chip actually emitted, nothing added",
            PaletteChoice::Soft => {
                "Gentler colour, the way a television left at the factory setting looked"
            }
            PaletteChoice::Vivid => "Colour turned up, the way most people ran theirs",
            PaletteChoice::File => "A .pal palette you imported",
        }
    }
    /// The table, or None for Standard (the core's own) and File (supplied at run time).
    pub fn colours(self) -> Option<[u32; 64]> {
        match self {
            PaletteChoice::Standard | PaletteChoice::File => None,
            PaletteChoice::Hardware => Some(model::build(1.0, 0.0, 1.0, 0.0, 1.0)),
            PaletteChoice::Soft => Some(model::build(0.78, 0.0, 0.96, 0.02, 1.06)),
            PaletteChoice::Vivid => Some(model::build(1.30, 0.0, 1.05, 0.0, 0.94)),
        }
    }
    pub fn from_id(id: i32) -> PaletteChoice {
        PaletteChoice::ALL
            .into_iter()
            .find(|p| p.id() == id)
            .unwrap_or(PaletteChoice::Standard)
    }
}

/// The 2C02 emitted a square wave, not RGB. These colours are generated by
/// doing what the chip did and decoding it the way a television did, so the
/// knobs are the ones a television had.
pub mod model {
    use std::f64::consts::PI;

    const LOW: [f64; 4] = [0.350, 0.518, 0.962, 1.550];
    const HIGH: [f64; 4] = [1.094, 1.506, 1.962, 1.962];
    const BLACK: f64 = LOW[1];
    const WHITE: f64 = HIGH[3];
    const BURST: f64 = 4.0;
    const SAMPLES: usize = 12;

    fn signal(hue: usize, level: usize, sample: usize) -> f64 {
        match hue {
            0 => HIGH[level],
            13 => LOW[level],
            h if h >= 14 => LOW[1],
            _ => {
                if (sample + hue) % SAMPLES < SAMPLES / 2 {
                    HIGH[level]
                } else {
                    LOW[level]
                }
            }
        }
    }

    fn channel(v: f64, gamma: f64) -> u32 {
        (255.0 * v.clamp(0.0, 1.0).powf(gamma))
            .round()
            .clamp(0.0, 255.0) as u32
    }

    pub fn build(
        saturation: f64,
        tint: f64,
        contrast: f64,
        brightness: f64,
        gamma: f64,
    ) -> [u32; 64] {
        let mut out = [0; 64];
        for (index, slot) in out.iter_mut().enumerate() {
            let hue = index & 15;
            let level = (index >> 4) & 3;
            let (mut y, mut i, mut q) = (0.0, 0.0, 0.0);
            for sample in 0..SAMPLES {
                let volts = (signal(hue, level, sample) - BLACK) / (WHITE - BLACK);
                let angle = 2.0 * PI * (sample as f64 + BURST + tint) / SAMPLES as f64;
                y += volts;
                i += volts * angle.cos();
                q += volts * angle.sin();
            }
            y = y / SAMPLES as f64 * contrast + brightness;
            i = i / SAMPLES as f64 * saturation * 2.0;
            q = q / SAMPLES as f64 * saturation * 2.0;
            *slot = channel(y + 0.956 * i + 0.619 * q, gamma) << 16
                | channel(y - 0.272 * i - 0.647 * q, gamma) << 8
                | channel(y - 1.106 * i + 1.703 * q, gamma);
        }
        out
    }

    /// The model's closest match to the table the core ships.
    pub fn standard() -> [u32; 64] {
        build(0.90, 0.0, 1.0, 0.0, 1.0)
    }

    /// A `.pal` file's bytes. Only the round trip below reads it: the shell
    /// imports palettes and never writes one out.
    #[allow(dead_code)]
    pub fn bytes(colours: &[u32; 64]) -> Vec<u8> {
        colours
            .iter()
            .flat_map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8])
            .collect()
    }

    /// A `.pal` file: 64 colours of RGB. Longer files carry emphasis variants, ignored.
    pub fn parse(file: &[u8]) -> Option<[u32; 64]> {
        if file.len() < 64 * 3 {
            return None;
        }
        let mut out = [0; 64];
        for (at, slot) in out.iter_mut().enumerate() {
            *slot = (file[at * 3] as u32) << 16
                | (file[at * 3 + 1] as u32) << 8
                | file[at * 3 + 2] as u32;
        }
        Some(out)
    }
}

/// A framebuffer to preview looks against when no game is open: colour bars
/// from palette indices (so a palette change restains them), a ramp, a
/// one-pixel checkerboard and bright blocks on a dark field. RGBA, top-down.
pub fn sample_frame(palette: &[u32; 64]) -> Vec<u8> {
    const BAR_INDICES: [usize; 8] = [0x30, 0x28, 0x2a, 0x1a, 0x2c, 0x21, 0x16, 0x14];
    let bars: Vec<[u8; 3]> = BAR_INDICES
        .iter()
        .map(|&i| {
            let c = palette[i];
            [(c >> 16) as u8, (c >> 8) as u8, c as u8]
        })
        .collect();
    let mut out = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let rgb = if y < 72 {
                bars[(x * bars.len() / WIDTH).min(bars.len() - 1)]
            } else if y < 120 {
                [x as u8, x as u8, x as u8]
            } else if y < 168 {
                if (x + y) % 2 == 0 {
                    [232, 232, 240]
                } else {
                    [24, 24, 40]
                }
            } else if (16..48).contains(&(x % 64)) && (8..32).contains(&(y % 40)) {
                [248, 216, 96]
            } else {
                [16, 20, 32]
            };
            let at = (y * WIDTH + x) * 4;
            out[at..at + 3].copy_from_slice(&rgb);
            out[at + 3] = 255;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aspect_of(v: Viewport) -> f32 {
        v.width as f32 / v.height as f32
    }

    #[test]
    fn television_is_four_thirds_and_fits_the_surface() {
        let wide = layout(1000, 500, Aspect::Television, false);
        assert!((aspect_of(wide) - 4.0 / 3.0).abs() < 0.01);
        assert!(wide.width <= 1000 && wide.height <= 500);
        let tall = layout(600, 2000, Aspect::Television, false);
        assert!((aspect_of(tall) - 4.0 / 3.0).abs() < 0.01);
        assert_eq!(tall.width, 600);
    }

    #[test]
    fn hardware_is_narrower_than_television() {
        let television = layout(1000, 1000, Aspect::Television, false);
        let hardware = layout(1000, 1000, Aspect::Hardware, false);
        assert!((aspect_of(hardware) - 256.0 * 8.0 / 7.0 / 240.0).abs() < 0.01);
        assert!(hardware.height > television.height);
    }

    #[test]
    fn trimming_raises_the_aspect_and_narrows_the_source() {
        assert_eq!(visible_height(false), 240);
        assert_eq!(visible_height(true), 224);
        assert_eq!(trim_fraction(false), 0.0);
        assert!((trim_fraction(true) - 8.0 / 240.0).abs() < 0.0001);
        let whole = layout(1000, 1000, Aspect::Hardware, false);
        let trimmed = layout(1000, 1000, Aspect::Hardware, true);
        assert!(aspect_of(trimmed) > aspect_of(whole));
    }

    #[test]
    fn pixel_perfect_uses_whole_multiples_only() {
        assert_eq!(
            layout(1024, 960, Aspect::Pixels, false),
            Viewport {
                x: 0,
                y: 0,
                width: 1024,
                height: 960
            }
        );
        assert_eq!(
            layout(1000, 900, Aspect::Pixels, false),
            Viewport {
                x: 116,
                y: 90,
                width: 768,
                height: 720
            }
        );
        assert_eq!(
            layout(1024, 900, Aspect::Pixels, true),
            Viewport {
                x: 0,
                y: 2,
                width: 1024,
                height: 896
            }
        );
    }

    #[test]
    fn every_layout_is_centred_and_inside_the_surface() {
        for aspect in Aspect::ALL {
            for trim in [false, true] {
                for width in [1, 320, 721, 1000, 2400] {
                    for height in [1, 240, 519, 900, 1600] {
                        let v = layout(width, height, aspect, trim);
                        assert_eq!(v.x, (width - v.width) / 2);
                        assert_eq!(v.y, (height - v.height) / 2);
                        if aspect != Aspect::Pixels {
                            assert!(v.width <= width && v.height <= height);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_degenerate_surface_draws_nothing() {
        let none = Viewport {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert_eq!(layout(0, 0, Aspect::Television, false), none);
        assert_eq!(layout(-4, 100, Aspect::Pixels, false), none);
    }

    #[test]
    fn look_ids_are_the_saved_values_so_they_must_not_move() {
        let ids: Vec<i32> = Look::ALL.iter().map(|l| l.id()).collect();
        assert_eq!(ids, vec![0, 1, 2, 3, 4, 6, 7, 8, 9, 10]);
        assert_eq!(Look::from_id(5), Look::OldPhoto);
        assert_eq!(Look::from_id(2), Look::OldTv);
        assert_eq!(Look::from_id(99), Look::Off);
        assert_eq!(Look::from_id(-1), Look::Off);
        let smoothed: Vec<Look> = Look::ALL
            .iter()
            .copied()
            .filter(|l| l.source() == Source::Smoothed)
            .collect();
        assert_eq!(smoothed, vec![Look::Cartoon, Look::Smooth]);
        let composite: Vec<Look> = Look::ALL
            .iter()
            .copied()
            .filter(|l| l.source() == Source::Composite)
            .collect();
        assert_eq!(composite, vec![Look::Composite]);
        for look in Look::ALL {
            assert!(!look.label().is_empty() && !look.note().is_empty());
        }
    }

    #[test]
    fn palette_ids_are_the_saved_values_so_they_must_not_move() {
        let ids: Vec<i32> = PaletteChoice::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(ids, vec![0, 1, 2, 3, 5]);
        assert_eq!(PaletteChoice::from_id(4), PaletteChoice::Standard);
        assert_eq!(PaletteChoice::from_id(99), PaletteChoice::Standard);
        assert_eq!(PaletteChoice::from_id(5), PaletteChoice::File);
        assert!(PaletteChoice::Standard.colours().is_none());
        assert!(PaletteChoice::File.colours().is_none());
        for p in [
            PaletteChoice::Hardware,
            PaletteChoice::Soft,
            PaletteChoice::Vivid,
        ] {
            for c in p.colours().unwrap() {
                assert_eq!(c & 0xff00_0000, 0);
            }
        }
    }

    fn channels(c: u32) -> [i32; 3] {
        [
            ((c >> 16) & 255) as i32,
            ((c >> 8) & 255) as i32,
            (c & 255) as i32,
        ]
    }

    #[test]
    fn the_model_reproduces_the_table_the_core_ships() {
        let model = model::standard();
        let mut errors = Vec::new();
        for (index, &modelled) in model.iter().enumerate() {
            if index & 15 >= 14 {
                continue;
            }
            let core = crate::palette::PALETTE[index];
            if core == 0 && (index & 15) != 13 {
                continue;
            }
            let (a, b) = (channels(modelled), channels(core));
            errors.push((0..3).map(|i| (a[i] - b[i]).abs()).max().unwrap());
        }
        assert!(errors.len() >= 50);
        let mean = errors.iter().sum::<i32>() as f64 / errors.len() as f64;
        assert!(mean < 15.0, "mean error {mean}");
        assert!(*errors.iter().max().unwrap() < 40);
    }

    #[test]
    fn the_blanking_colours_are_never_brighter_than_the_darkest_real_one() {
        fn luma(c: u32) -> f64 {
            let [r, g, b] = channels(c);
            0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64
        }
        for knobs in [
            model::build(1.0, 0.0, 1.0, 0.0, 1.0),
            model::build(2.0, 0.0, 1.5, 0.2, 1.0),
            model::standard(),
        ] {
            let blanking: Vec<f64> = (0..64)
                .filter(|i| i & 15 >= 14)
                .map(|i| luma(knobs[i]))
                .collect();
            let real: Vec<f64> = (0..64)
                .filter(|i| i & 15 < 13)
                .map(|i| luma(knobs[i]))
                .collect();
            assert!(blanking.iter().all(|&l| l == blanking[0]));
            let max_blank = blanking.iter().cloned().fold(f64::MIN, f64::max);
            let min_real = real.iter().cloned().fold(f64::MAX, f64::min);
            assert!(max_blank <= min_real + 0.001);
        }
        let plain = model::build(1.0, 0.0, 1.0, 0.0, 1.0);
        for level in 0..4 {
            for hue in 14..16 {
                assert_eq!(plain[level << 4 | hue], 0);
            }
        }
    }

    #[test]
    fn turning_the_colour_down_leaves_greys() {
        for c in model::build(0.0, 0.0, 1.0, 0.0, 1.0) {
            let ch = channels(c);
            assert!(ch.iter().max().unwrap() - ch.iter().min().unwrap() <= 2);
        }
    }

    #[test]
    fn a_palette_survives_the_round_trip_to_bytes_and_back() {
        let original = model::build(1.2, 0.5, 1.0, 0.0, 1.0);
        assert_eq!(model::parse(&model::bytes(&original)), Some(original));
    }

    #[test]
    fn a_file_too_short_to_be_a_palette_is_refused() {
        assert!(model::parse(&[0; 191]).is_none());
        assert!(model::parse(&[0; 192]).is_some());
        assert!(model::parse(&[0; 64 * 3 * 8]).is_some());
    }

    #[test]
    fn the_sample_frame_is_a_whole_opaque_framebuffer_with_something_for_every_look() {
        let pixels = sample_frame(&model::standard());
        assert_eq!(pixels.len(), WIDTH * HEIGHT * 4);
        assert!(pixels.iter().skip(3).step_by(4).all(|&a| a == 255));
        let luma = |x: usize, y: usize| {
            let at = (y * WIDTH + x) * 4;
            pixels[at] as i32 + pixels[at + 1] as i32 + pixels[at + 2] as i32
        };
        let bars: std::collections::BTreeSet<i32> =
            (0..WIDTH).step_by(8).map(|x| luma(x, 20)).collect();
        assert!(bars.len() > 4);
        assert!(luma(240, 100) > luma(8, 100));
        assert_ne!(luma(10, 140), luma(11, 140));
        let band: Vec<i32> = (168..HEIGHT)
            .step_by(4)
            .flat_map(|y| (0..WIDTH).step_by(4).map(move |x| (x, y)))
            .map(|(x, y)| luma(x, y))
            .collect();
        assert!(band.iter().max().unwrap() > &(band.iter().min().unwrap() + 300));
    }
}
