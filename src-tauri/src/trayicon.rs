//! Builds the menubar tray image: a flat gauge glyph while the percentage is
//! unknown, or a two-line "label / percent" block otherwise. Both are rendered
//! black on transparent and shown as macOS template images, so the system tints
//! them to match the menubar (light/dark).
//!
//! tray-icon scales the whole image to 18pt tall, so each line is fitted tightly
//! to its ink (no font leading) to use the full height, rendered at high
//! resolution for a crisp retina downscale, and lightly dilated to read as a
//! medium weight like the neighbouring native items.

use ab_glyph::{point, Font, FontRef, OutlinedGlyph, PxScale, ScaleFont};
use tauri::image::Image;

/// The app's UI typeface (Basic Latin subset) — reused so the menubar text
/// matches the widget. Shared file: keep in sync with the frontend's @font-face.
const FONT: &[u8] = include_bytes!("../../src/fonts/SpaceGrotesk-Subset.ttf");

/// Flat black gauge, shown when there is no percentage to display.
const UNKNOWN_PNG: &[u8] = include_bytes!("../icons/tray-unknown.png");

const LABEL_PX: f32 = 50.0; // top line (e.g. "5h")
const VALUE_PX: f32 = 64.0; // bottom line (e.g. "73%"), the emphasis
const LABEL_WEIGHT: usize = 1; // faux-bold dilation radius, in source px
const VALUE_WEIGHT: usize = 1;
const LINE_GAP: usize = 4;
const PAD: usize = 2;

pub fn unknown() -> Image<'static> {
    Image::from_bytes(UNKNOWN_PNG).expect("embedded tray icon is valid png")
}

/// Two stacked, horizontally-centered lines (e.g. "5h" over "73%").
pub fn labelled(top: &str, bottom: &str) -> Image<'static> {
    let font = FontRef::try_from_slice(FONT).expect("embedded font is valid");

    let label = raster_line(&font, LABEL_PX, top, LABEL_WEIGHT);
    let value = raster_line(&font, VALUE_PX, bottom, VALUE_WEIGHT);

    let width = label.w.max(value.w) + PAD * 2;
    let height = label.h + LINE_GAP + value.h + PAD * 2;
    let mut rgba = vec![0u8; width * height * 4];

    blit(&mut rgba, width, &label, (width - label.w) / 2, PAD);
    blit(&mut rgba, width, &value, (width - value.w) / 2, PAD + label.h + LINE_GAP);

    Image::new_owned(rgba, width as u32, height as u32)
}

/// A tight alpha-coverage bitmap of one line of text.
struct Bitmap {
    alpha: Vec<u8>,
    w: usize,
    h: usize,
}

/// Rasterize `text` into a bitmap cropped to its ink, dilated by `weight` px to
/// thicken the strokes (faux-bold).
fn raster_line(font: &FontRef, px: f32, text: &str, weight: usize) -> Bitmap {
    let scale = PxScale::from(px);
    let sf = font.as_scaled(scale);

    // Lay glyphs along a baseline at y = 0, then crop to the union of ink bounds.
    let mut outlines: Vec<OutlinedGlyph> = Vec::new();
    let mut x = 0.0_f32;
    let mut prev = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            x += sf.kern(p, id);
        }
        if let Some(o) = font.outline_glyph(id.with_scale_and_position(scale, point(x, 0.0))) {
            outlines.push(o);
        }
        x += sf.h_advance(id);
        prev = Some(id);
    }

    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    for o in &outlines {
        let b = o.px_bounds();
        min_x = min_x.min(b.min.x);
        min_y = min_y.min(b.min.y);
        max_x = max_x.max(b.max.x);
        max_y = max_y.max(b.max.y);
    }

    let w = (max_x - min_x).ceil() as usize + weight + 1;
    let h = (max_y - min_y).ceil() as usize + weight + 1;
    let mut alpha = vec![0u8; w * h];

    for o in &outlines {
        let b = o.px_bounds();
        let off_x = b.min.x - min_x;
        let off_y = b.min.y - min_y;
        o.draw(|gx, gy, coverage| {
            let a = (coverage * 255.0) as u8;
            let bx = off_x as usize + gx as usize;
            let by = off_y as usize + gy as usize;
            // Dilate down-right by `weight` px so strokes read heavier.
            for dy in 0..=weight {
                for dx in 0..=weight {
                    let i = (by + dy) * w + (bx + dx);
                    if a > alpha[i] {
                        alpha[i] = a;
                    }
                }
            }
        });
    }

    Bitmap { alpha, w, h }
}

/// Copy a line bitmap's alpha into the RGBA canvas at (ox, oy); text is black,
/// so only the alpha channel carries the shape (all a template image needs).
fn blit(rgba: &mut [u8], canvas_w: usize, bmp: &Bitmap, ox: usize, oy: usize) {
    for y in 0..bmp.h {
        for x in 0..bmp.w {
            let a = bmp.alpha[y * bmp.w + x];
            if a == 0 {
                continue;
            }
            let i = ((oy + y) * canvas_w + (ox + x)) * 4;
            rgba[i + 3] = a;
        }
    }
}
