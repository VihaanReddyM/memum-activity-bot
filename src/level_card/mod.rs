/// Level card image generator.
///
/// Produces a 1000x350 PNG using only the `image` crate and the Minecraft
/// bitmap font sheet bundled at `src/font/assets/textures/font/ascii.png`.
/// No TTF/OTF fonts or `ab_glyph` are used.
///
/// # Font sheet layout
/// The sheet is 128x128 pixels, arranged as a 16x16 grid of 8x8-pixel
/// character cells.  Character `c` (ASCII code) lives at:
///   `grid_col = c % 16`,  `src_x = grid_col * 8`
///   `grid_row = c / 16`,  `src_y = grid_row * 8`
/// A pixel is considered "filled" when `alpha > 128`.
use std::io::Cursor;

use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
use tracing::debug;

// ---------------------------------------------------------------------------
// Embedded font sheet
// ---------------------------------------------------------------------------

static FONT_PNG: &[u8] = include_bytes!("../font/assets/textures/font/ascii.png");

// ---------------------------------------------------------------------------
// Colour constants
// ---------------------------------------------------------------------------

const BG: Rgba<u8> = Rgba([0x1a, 0x1a, 0x2e, 0xff]);
const PANEL: Rgba<u8> = Rgba([0x22, 0x22, 0x3a, 0xff]);
const WHITE: Rgba<u8> = Rgba([0xff, 0xff, 0xff, 0xff]);
const CYAN: Rgba<u8> = Rgba([0x00, 0xbf, 0xff, 0xff]);
const MUTED: Rgba<u8> = Rgba([0x88, 0x88, 0xaa, 0xff]);
const GREEN: Rgba<u8> = Rgba([0x44, 0xff, 0x88, 0xff]);
const GOLD: Rgba<u8> = Rgba([0xff, 0xd7, 0x00, 0xff]);
const BAR_BG: Rgba<u8> = Rgba([0x2a, 0x2a, 0x4a, 0xff]);
const DIVIDER: Rgba<u8> = Rgba([0x30, 0x30, 0x50, 0xff]);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// All data required to render a level card.
pub struct LevelCardParams {
    pub minecraft_username: String,
    pub level: i32,
    pub total_xp: f64,
    /// XP accumulated inside the current level (total_xp - xp_for_level(level)).
    pub xp_this_level: f64,
    /// XP span of the current level (xp_for_level(level+1) - xp_for_level(level)).
    pub xp_for_next_level: f64,
    /// `(display_name, delta)` pairs, already filtered to `delta > 0`, up to 8.
    pub stat_deltas: Vec<(String, f64)>,
    pub xp_gained: f64,
    /// Raw PNG / JPEG bytes of the player's 80x80 Crafatar avatar.
    /// `None` -> a placeholder rectangle is drawn instead.
    pub avatar_bytes: Option<Vec<u8>>,
    /// Rank of the user in the guild by total XP, if available.
    pub rank: Option<i64>,

    pub milestone_progress: Vec<(i32, bool)>, // (milestone level, achieved)
}

/// Render the level card and return the PNG bytes.
pub fn render(params: &LevelCardParams) -> Vec<u8> {
    debug!(
        "level_card::render: minecraft_username={}, level={}, total_xp={}, xp_this_level={}, xp_for_next_level={}, stat_deltas_len={}, xp_gained={}, has_avatar={}",
        params.minecraft_username,
        params.level,
        params.total_xp,
        params.xp_this_level,
        params.xp_for_next_level,
        params.stat_deltas.len(),
        params.xp_gained,
        params.avatar_bytes.is_some()
    );

    let font = image::load_from_memory(FONT_PNG)
        .expect("embedded font sheet is valid PNG")
        .to_rgba8();

    let mut img = RgbaImage::from_pixel(1000, 350, BG);

    // == INNER PANEL (rounded rect) ==========================================
    fill_rounded_rect(&mut img, 8, 8, 984, 334, 12, PANEL);

    // == AVATAR ==============================================================
    if let Some(bytes) = &params.avatar_bytes {
        debug!(
            "level_card::render: loading avatar from provided bytes (len={})",
            bytes.len()
        );
        if let Ok(dyn_img) = image::load_from_memory(bytes) {
            let avatar = dyn_img.resize_exact(80, 80, image::imageops::FilterType::Nearest);
            // Draw avatar with rounded corners (r=8)
            for ay in 0..80u32 {
                for ax in 0..80u32 {
                    if is_inside_rounded_rect(ax, ay, 80, 80, 8) {
                        img.put_pixel(28 + ax, 28 + ay, avatar.get_pixel(ax, ay));
                    }
                }
            }
        } else {
            debug!("level_card::render: failed to decode avatar bytes, drawing placeholder");
            fill_rounded_rect(&mut img, 28, 28, 80, 80, 8, MUTED);
        }
    } else {
        debug!("level_card::render: no avatar provided, drawing placeholder");
        fill_rounded_rect(&mut img, 28, 28, 80, 80, 8, MUTED);
    }

    // == TOP SECTION (Player Identity) =======================================
    // Username: scale=3, WHITE
    render_text(
        &font,
        &mut img,
        124,
        29,
        &params.minecraft_username,
        3,
        WHITE,
    );

    // "LEVEL {n}": scale=2, CYAN
    render_text(
        &font,
        &mut img,
        124,
        62,
        &format!("LEVEL {}", params.level),
        2,
        CYAN,
    );

    let rank_colour = if let Some(rank) = params.rank {
        if rank == 1 {
            GOLD
        } else if rank <= 3 {
            GREEN
        } else {
            MUTED
        }
    } else {
        MUTED
    };

    // "RANK #{rank}": scale=2, GOLD (if rank is available)
    if let Some(rank) = params.rank {
        render_text(
            &font,
            &mut img,
            124,
            92,
            &format!("RANK #{}", rank),
            2,
            rank_colour,
        );
    }

    // == PROGRESS BAR ========================================================
    // Bar background (rounded)
    fill_rounded_rect(&mut img, 28, 120, 944, 18, 9, BAR_BG);

    // Bar fill (rounded)
    let pct = if params.xp_for_next_level > 0.0 {
        (params.xp_this_level / params.xp_for_next_level).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let fill_w = (944.0 * pct).round() as u32;
    if fill_w > 0 {
        // Ensure minimum width covers the radius so it still looks rounded
        let clamped_w = fill_w.max(18);
        fill_rounded_rect(&mut img, 28, 120, clamped_w, 18, 9, CYAN);
    }

    let percentage_complete = params.xp_this_level / params.xp_for_next_level * 100.0;

    // XP text below bar: scale=2, MUTED
    render_text(
        &font,
        &mut img,
        28,
        146,
        &format!(
            "{:.0} / {:.0} XP ({:.1}%)",
            params.xp_this_level, params.xp_for_next_level, percentage_complete
        ),
        2,
        MUTED,
    );

    // == DIVIDER =============================================================
    fill_rect(&mut img, 28, 172, 944, 2, DIVIDER);

    // == BOTTOM SECTION (Stat Changes) =======================================
    // Header
    render_text(&font, &mut img, 28, 188, "STAT CHANGES", 2, CYAN);

    // Stats in two columns (up to 4 per column, 8 total)
    if params.stat_deltas.is_empty() {
        render_text(&font, &mut img, 28, 214, "No changes yet", 2, MUTED);
    } else {
        let col1_x: u32 = 28;
        let col2_x: u32 = 520;
        let base_y: u32 = 214;
        let step: u32 = 24;

        for (i, (name, delta)) in params.stat_deltas.iter().take(8).enumerate() {
            let col_x = if i < 4 { col1_x } else { col2_x };
            let row = (i % 4) as u32;
            let y = base_y + row * step;
            let line = format!("+{:.0} {}", delta, name);
            render_text(&font, &mut img, col_x, y, &line, 2, GREEN);
        }
    }

    // == MILESTONE BADGES =======================================================
    let milestones_x = 540;
    let milestones_y = 188;
    render_text(
        &font,
        &mut img,
        milestones_x,
        milestones_y,
        "MILESTONES",
        2,
        CYAN,
    );

    let max_milestones = 8;
    let col1_x = milestones_x;
    let col2_x = milestones_x + 200;
    let base_y = milestones_y + 26;
    let row_step = 22;
    

    // This part took me fucking ages bro
    // OMG I HATE THIS SO MUCH
    // The calculation of percentage for each milestone
    // was a pain in the ass. i want a extra for this part on god.
    // Its either this was actually hard or i suck balls at programming (its probably the second one)
    let xp_pct = if params.xp_for_next_level > 0.0 {
        (params.xp_this_level / params.xp_for_next_level).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let current_fractional_level = params.level as f64 + xp_pct;

    for (i, (m_level, reached)) in params
        .milestone_progress
        .iter()
        .take(max_milestones)
        .enumerate()
    {
        let column = i / 4;
        let row = i % 4;
        let x = if column == 0 { col1_x } else { col2_x };
        let y = base_y + (row as u32) * row_step;

        let m_level_f = *m_level as f64;

        let percentage = if i == 0 {
            // first milestone: 0 -> milestone
            ((current_fractional_level / m_level_f) * 100.0)
                .clamp(0.0, 100.0)
                .round() as i32
        } else if let Some((next_m_tuple, _)) = params.milestone_progress.get(i + 1) {
            let next_m_f = *next_m_tuple as f64;

            if current_fractional_level >= next_m_f {
                100
            } else if current_fractional_level < m_level_f {
                0
            } else {
                (((current_fractional_level - m_level_f) / (next_m_f - m_level_f)) * 100.0).round()
                    as i32
            }
        } else {
            if params.level >= *m_level { 100 } else { 0 }
        };

        let color = if percentage > 0 {
            if *reached { GREEN } else { WHITE }
        } else {
            MUTED
        };

        render_text(
            &font,
            &mut img,
            x,
            y,
            &format!("Level {} ({}%)", m_level, percentage),
            2,
            color,
        );
    }

    // == XP GAINED (right-aligned to x=972) ==================================
    let xp_text = format!("+{:.0} XP GAINED", params.xp_gained);
    let text_w = measure_text(&font, &xp_text, 2);
    let xp_x = 972u32.saturating_sub(text_w);
    render_text(&font, &mut img, xp_x, 146, &xp_text, 2, MUTED);

    // == ENCODE PNG ===========================================================
    let mut buf: Vec<u8> = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .expect("PNG encoding should not fail");
    debug!(
        "level_card::render: finished encoding PNG (bytes={})",
        buf.len()
    );
    buf
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Fill a solid-colour axis-aligned rectangle (no rounding).
fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    debug!("level_card::fill_rect: x={}, y={}, w={}, h={}", x, y, w, h);
    let img_w = img.width();
    let img_h = img.height();
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < img_w && py < img_h {
                img.put_pixel(px, py, color);
            }
        }
    }
}

/// Test whether pixel (px, py) within a rect of size (w, h) falls inside a
/// rounded rectangle with corner radius `r`.  Uses circle-distance check in
/// corner regions.
fn is_inside_rounded_rect(px: u32, py: u32, w: u32, h: u32, r: u32) -> bool {
    // Which corner region (if any) are we in?
    let in_left = px < r;
    let in_right = px >= w.saturating_sub(r);
    let in_top = py < r;
    let in_bottom = py >= h.saturating_sub(r);

    if (in_left || in_right) && (in_top || in_bottom) {
        // Corner region: check circle distance.
        let cx = if in_left { r - 1 } else { w - r };
        let cy = if in_top { r - 1 } else { h - r };
        let dx = px as i64 - cx as i64;
        let dy = py as i64 - cy as i64;
        dx * dx + dy * dy <= (r as i64) * (r as i64)
    } else {
        true
    }
}

/// Fill a rounded rectangle at `(x, y)` with size `(w, h)` and corner
/// radius `r`.
fn fill_rounded_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, r: u32, color: Rgba<u8>) {
    debug!(
        "level_card::fill_rounded_rect: x={}, y={}, w={}, h={}, r={}",
        x, y, w, h, r
    );
    let img_w = img.width();
    let img_h = img.height();
    for dy in 0..h {
        for dx in 0..w {
            if is_inside_rounded_rect(dx, dy, w, h, r) {
                let px = x + dx;
                let py = y + dy;
                if px < img_w && py < img_h {
                    img.put_pixel(px, py, color);
                }
            }
        }
    }
}

/// Return the rendered pixel width of `glyph` (code point `c`) in the font
/// sheet.  Scans the 8x8 cell and returns the index of the rightmost
/// non-transparent column + 1, or 4 as a fallback for empty cells.
fn measure_glyph_width(font: &RgbaImage, c: u8) -> u32 {
    debug!("level_card::measure_glyph_width: c={}", c);
    let grid_col = (c % 16) as u32;
    let grid_row = (c / 16) as u32;
    let src_x = grid_col * 8;
    let src_y = grid_row * 8;

    let mut rightmost: i32 = -1;
    for row in 0..8u32 {
        for col in 0..8u32 {
            let px = font.get_pixel(src_x + col, src_y + row);
            if px[3] > 128 {
                if col as i32 > rightmost {
                    rightmost = col as i32;
                }
            }
        }
    }
    if rightmost < 0 {
        4
    } else {
        (rightmost + 1) as u32
    }
}

/// Measure the total rendered pixel width of `text` at the given `scale`.
/// Mirrors the exact cursor logic of `render_text` so right-alignment is
/// pixel-perfect.
fn measure_text(font: &RgbaImage, text: &str, scale: u32) -> u32 {
    debug!(
        "level_card::measure_text: text_len={}, scale={}",
        text.len(),
        scale
    );
    let mut width: u32 = 0;
    let mut last_was_glyph = false;

    for ch in text.chars() {
        let c = ch as u32;

        if c < 0x20 || c > 0x7e {
            // Non-printable: advance like a space + gap
            width += 4 * scale + scale;
            last_was_glyph = false;
            continue;
        }

        let c = c as u8;

        if c == b' ' {
            width += 4 * scale;
            last_was_glyph = false;
            continue;
        }

        let glyph_w = measure_glyph_width(font, c);
        width += glyph_w * scale + scale; // glyph width + 1-px gap (scaled)
        last_was_glyph = true;
    }

    // Remove trailing gap if the last character was a drawn glyph
    if last_was_glyph {
        width = width.saturating_sub(scale);
    }

    debug!("level_card::measure_text: result_width={}", width);
    width
}

/// Render `text` onto `img` at pixel position `(x, y)` using the Minecraft
/// bitmap font.  Each glyph is drawn at `scale x scale` block size.
fn render_text(
    font: &RgbaImage,
    img: &mut RgbaImage,
    x: u32,
    y: u32,
    text: &str,
    scale: u32,
    color: Rgba<u8>,
) {
    debug!(
        "level_card::render_text: x={}, y={}, text_len={}, scale={}, color={:?}",
        x,
        y,
        text.len(),
        scale,
        color
    );
    let img_w = img.width();
    let img_h = img.height();
    let mut cursor_x = x;

    for ch in text.chars() {
        let c = ch as u32;

        // Only handle printable ASCII 0x20-0x7E.
        if c < 0x20 || c > 0x7e {
            cursor_x += 4 * scale + scale;
            continue;
        }

        let c = c as u8;

        // Space: advance without drawing.
        if c == b' ' {
            cursor_x += 4 * scale;
            continue;
        }

        let grid_col = (c % 16) as u32;
        let grid_row = (c / 16) as u32;
        let src_x = grid_col * 8;
        let src_y = grid_row * 8;
        let glyph_w = measure_glyph_width(font, c);

        for fy in 0..8u32 {
            for fx in 0..glyph_w {
                let fpx = font.get_pixel(src_x + fx, src_y + fy);
                if fpx[3] > 128 {
                    // Draw a scale x scale block.
                    for by in 0..scale {
                        for bx in 0..scale {
                            let px = cursor_x + fx * scale + bx;
                            let py = y + fy * scale + by;
                            if px < img_w && py < img_h {
                                img.put_pixel(px, py, color);
                            }
                        }
                    }
                }
            }
        }

        cursor_x += glyph_w * scale + scale; // glyph width + 1-px gap
    }
    debug!(
        "level_card::render_text: finished rendering text='{}'",
        text
    );
}
