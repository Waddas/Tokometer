//! Builds the tray icon. Two styles are offered (see `state::TrayStyle`):
//!  - `gauge`: a colour-coded ring that fills with the 5h utilization. Legible
//!    at any size and on any background, so it's the default.
//!  - `text`: the figure as text — two stacked lines ("5h" over "73%") on the
//!    tall retina macOS menubar, or a single colour-coded number ("73") on the
//!    small Windows notification area, where stacked text would be unreadable.
//!
//! Colour: the ring and the Windows text use the usage ramp (`arc_color`). The
//! macOS text is monochrome `ink` (black), flagged as a template image so the
//! system tints it to the menubar; coloured icons must not be flagged as
//! templates (macOS would discard their colour). `ink` also draws the unknown
//! glyph — black on macOS (template), matched to the taskbar on Windows.
//!
//! `unknown` is the fallback glyph shown before the first reading or on error.

use tauri::image::Image;

/// Flat gauge glyph, shown when there is no percentage to display.
const UNKNOWN_PNG: &[u8] = include_bytes!("../icons/tray-unknown.png");

pub fn unknown() -> Image<'static> {
    let img = Image::from_bytes(UNKNOWN_PNG).expect("embedded tray icon is valid png");
    tint(img, ink())
}

/// The 5h utilization as text. macOS shows two stacked lines ("5h" over "73%"),
/// crisp on the taller retina menubar; Windows shows just the bare number,
/// the only text legible at notification-area size.
pub fn text(label: &str, pct: f64) -> Image<'static> {
    #[cfg(target_os = "windows")]
    {
        let _ = label;
        let prog = (pct / 100.0).clamp(0.0, 1.0) as f32;
        single_line(&format!("{pct:.0}"), arc_color(prog))
    }
    #[cfg(not(target_os = "windows"))]
    {
        labelled(label, &format!("{pct:.0}%"))
    }
}

/// Ink color for the monochrome glyphs (text and the unknown gauge). On Windows
/// the bitmap is drawn as-is, so it must match the taskbar: black on a light
/// taskbar, white on a dark one. macOS shows these as template images and tints
/// them itself, so black is always correct there.
fn ink() -> u8 {
    #[cfg(target_os = "windows")]
    {
        if system_uses_light_theme() {
            0x00
        } else {
            0xFF
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        0x00
    }
}

/// Recolor every opaque pixel to `ink`, preserving the alpha shape. On macOS
/// (ink = black) this leaves the already-black glyph unchanged.
fn tint(img: Image<'_>, ink: u8) -> Image<'static> {
    let (w, h) = (img.width(), img.height());
    let mut rgba = img.rgba().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        if px[3] != 0 {
            px[0] = ink;
            px[1] = ink;
            px[2] = ink;
        }
    }
    Image::new_owned(rgba, w, h)
}

// ---------------------------------------------------------------------------
// Colour-coded progress ring
// ---------------------------------------------------------------------------

/// Source resolution of the ring; the OS downscales it to the tray icon size,
/// so this is effectively a supersample for clean anti-aliasing.
const GAUGE_PX: usize = 64;

/// A ring that fills clockwise from 12 o'clock to `pct`, coloured green → orange
/// → red as usage climbs. The unfilled remainder is the same colour at low
/// alpha, so the track reads on any background.
pub fn gauge(pct: f64) -> Image<'static> {
    use std::f32::consts::TAU;

    let s = GAUGE_PX;
    let center = s as f32 / 2.0;
    let outer = s as f32 * 0.44;
    let inner = outer - s as f32 * 0.16;
    let prog = (pct / 100.0).clamp(0.0, 1.0) as f32;
    let (r, g, b) = arc_color(prog);

    // Angular anti-alias half-width (in turns) ≈ one source pixel at the edge.
    let aa = 1.0 / (TAU * outer);
    let mut rgba = vec![0u8; s * s * 4];

    for y in 0..s {
        for x in 0..s {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let dist = (dx * dx + dy * dy).sqrt();

            // Radial coverage: full inside the band, 1px feathering at each edge.
            let cov = (outer - dist + 0.5)
                .clamp(0.0, 1.0)
                .min((dist - inner + 0.5).clamp(0.0, 1.0));
            if cov <= 0.0 {
                continue;
            }

            // Angle as a fraction of a turn, clockwise from 12 o'clock.
            let mut turn = dx.atan2(-dy) / TAU;
            if turn < 0.0 {
                turn += 1.0;
            }

            // Blend the filled arc over the faint track at the progress edge.
            let fill = ((prog - turn) / aa + 0.5).clamp(0.0, 1.0);
            let alpha = cov * (fill + (1.0 - fill) * 0.22);

            let i = (y * s + x) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = (alpha * 255.0).round() as u8;
        }
    }

    Image::new_owned(rgba, s as u32, s as u32)
}

/// Green (low) → orange (mid) → red (high). Orange rather than yellow at the
/// midpoint so it still reads on a light background.
fn arc_color(prog: f32) -> (u8, u8, u8) {
    const GREEN: (f32, f32, f32) = (46.0, 160.0, 80.0);
    const ORANGE: (f32, f32, f32) = (224.0, 140.0, 30.0);
    const RED: (f32, f32, f32) = (214.0, 60.0, 60.0);
    let lerp = |a: (f32, f32, f32), b: (f32, f32, f32), t: f32| {
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
    };
    let (r, g, b) = if prog < 0.6 {
        lerp(GREEN, ORANGE, prog / 0.6)
    } else {
        lerp(ORANGE, RED, (prog - 0.6) / 0.4)
    };
    (r.round() as u8, g.round() as u8, b.round() as u8)
}

// ---------------------------------------------------------------------------
// Windows: taskbar theme detection (the text/unknown glyphs are drawn as-is)
// ---------------------------------------------------------------------------

/// Reads HKCU\…\Themes\Personalize\SystemUsesLightTheme — the taskbar/tray
/// theme (distinct from AppsUseLightTheme, which drives app windows). Missing
/// or unreadable → assume a dark taskbar (the common Windows default).
#[cfg(target_os = "windows")]
fn system_uses_light_theme() -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    let subkey = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value = wide("SystemUsesLightTheme");
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut data as *mut u32 as *mut core::ffi::c_void,
            &mut size,
        )
    };
    status == ERROR_SUCCESS && data == 1
}

#[cfg(target_os = "windows")]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------
// Text rendering
// ---------------------------------------------------------------------------

use ab_glyph::{point, Font, FontRef, OutlinedGlyph, PxScale, ScaleFont};

/// The app's UI typeface (Basic Latin subset) — reused so the text matches the
/// widget. Shared file: keep in sync with the frontend's @font-face.
const FONT: &[u8] = include_bytes!("../../src/fonts/SpaceGrotesk-Subset.ttf");

const VALUE_PX: f32 = 64.0; // the emphasis line (e.g. "73%" / "27")
const VALUE_WEIGHT: usize = 1; // faux-bold dilation radius, in source px
const PAD: usize = 2;

#[cfg(not(target_os = "windows"))]
const LABEL_PX: f32 = 50.0; // top line (e.g. "5h")
#[cfg(not(target_os = "windows"))]
const LABEL_WEIGHT: usize = 1;
#[cfg(not(target_os = "windows"))]
const LINE_GAP: usize = 4;

/// A single centered line in `color` (e.g. "73"). The canvas is widened to at
/// least a two-digit width so a lone digit, being tall and narrow, isn't scaled
/// up larger than the usual double-digit reading when fit to the square slot.
#[cfg(target_os = "windows")]
fn single_line(text: &str, color: (u8, u8, u8)) -> Image<'static> {
    let font = FontRef::try_from_slice(FONT).expect("embedded font is valid");
    let line = raster_line(&font, VALUE_PX, text, VALUE_WEIGHT);
    let min_w = raster_line(&font, VALUE_PX, "00", VALUE_WEIGHT).w;

    let content_w = line.w.max(min_w);
    let width = content_w + PAD * 2;
    let height = line.h + PAD * 2;
    let mut rgba = vec![0u8; width * height * 4];

    blit(&mut rgba, width, &line, PAD + (content_w - line.w) / 2, PAD, color);
    Image::new_owned(rgba, width as u32, height as u32)
}

/// Two stacked, horizontally-centered lines (e.g. "5h" over "73%"). Each line is
/// fitted tightly to its ink (no font leading) to use the full height, rendered
/// at high resolution for a crisp retina downscale, and lightly dilated to read
/// as a medium weight like the neighbouring native items.
#[cfg(not(target_os = "windows"))]
fn labelled(top: &str, bottom: &str) -> Image<'static> {
    let font = FontRef::try_from_slice(FONT).expect("embedded font is valid");

    let label = raster_line(&font, LABEL_PX, top, LABEL_WEIGHT);
    let value = raster_line(&font, VALUE_PX, bottom, VALUE_WEIGHT);

    let width = label.w.max(value.w) + PAD * 2;
    let height = label.h + LINE_GAP + value.h + PAD * 2;
    let mut rgba = vec![0u8; width * height * 4];

    let ink = ink();
    let mono = (ink, ink, ink);
    blit(&mut rgba, width, &label, (width - label.w) / 2, PAD, mono);
    blit(&mut rgba, width, &value, (width - value.w) / 2, PAD + label.h + LINE_GAP, mono);

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

/// Copy a line bitmap into the RGBA canvas at (ox, oy), filling each covered
/// pixel with `color` and carrying the glyph shape in the alpha channel.
fn blit(rgba: &mut [u8], canvas_w: usize, bmp: &Bitmap, ox: usize, oy: usize, color: (u8, u8, u8)) {
    let (r, g, b) = color;
    for y in 0..bmp.h {
        for x in 0..bmp.w {
            let a = bmp.alpha[y * bmp.w + x];
            if a == 0 {
                continue;
            }
            let i = ((oy + y) * canvas_w + (ox + x)) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = a;
        }
    }
}
