//! The "Aurora Night" palette and the setup of the egui style.
//!
//! Everything is dark, but the floor is a deep navy rather than black: the old
//! backdrop and surface sat some twelve steps of lightness apart, so the rows of
//! the list dissolved into the background. Here each layer is clearly a step
//! above the one below it.

use egui::style::WidgetVisuals;
use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

/// The background of the whole window.
pub const BACKDROP: Color32 = Color32::from_rgb(0x0F, 0x14, 0x24);
/// Cards, list rows, the search field.
pub const SURFACE: Color32 = Color32::from_rgb(0x1A, 0x21, 0x38);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x25, 0x30, 0x52);
/// The card of the station that is playing.
pub const CARD: Color32 = Color32::from_rgb(0x10, 0x34, 0x4F);
pub const CARD_OUTLINE: Color32 = Color32::from_rgb(0x2F, 0x7F, 0xB5);
pub const ACCENT: Color32 = Color32::from_rgb(0x38, 0xBD, 0xF8);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x7D, 0xD3, 0xFC);
/// What goes *on top* of `ACCENT`. The accent is a bright sky blue, so the light
/// `TEXT` would not read against it; this is the near-black of the backdrop
/// family instead.
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x08, 0x20, 0x32);
pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xEE, 0xF9);
pub const MUTED: Color32 = Color32::from_rgb(0x8F, 0xA0, 0xBF);
pub const OUTLINE: Color32 = Color32::from_rgb(0x2A, 0x35, 0x50);
/// The green of "Live". Kept a step lighter than the blue of `ACCENT`, so the
/// two saturated colours do not compete on the same card.
pub const LIVE: Color32 = Color32::from_rgb(0x34, 0xD3, 0x99);
/// The app name in the title bar, and the metadata of the track. The one warm
/// colour in the palette, which is what makes it read as an accent at all.
pub const TITLE: Color32 = Color32::from_rgb(0xFB, 0xBF, 0x24);
/// The red of the button that closes the window.
pub const DANGER: Color32 = Color32::from_rgb(0xF4, 0x3F, 0x5E);
/// Failure messages. A lighter tint of `DANGER`, because a whole line of the
/// saturated red is hard to read on the backdrop.
pub const ERROR: Color32 = Color32::from_rgb(0xFD, 0xA4, 0xAF);
/// Warnings, for the egui widgets that ask for one.
pub const WARN: Color32 = Color32::from_rgb(0xFC, 0xD3, 0x4D);

/// Rounded corners for cards and rows.
pub const ROUND: CornerRadius = CornerRadius::same(10);

pub fn apply(ctx: &egui::Context) {
    // The palette is always dark, so we write both egui themes.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(configure);
}

fn configure(style: &mut egui::Style) {
    let prop = |size: f32| FontId::new(size, FontFamily::Proportional);
    style.text_styles = [
        (TextStyle::Heading, prop(19.0)),
        (TextStyle::Body, prop(14.0)),
        (TextStyle::Button, prop(14.0)),
        (TextStyle::Small, prop(12.0)),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
    ]
    .into();

    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = false;

    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.panel_fill = BACKDROP;
    visuals.window_fill = BACKDROP;
    visuals.faint_bg_color = SURFACE;
    visuals.extreme_bg_color = SURFACE;
    visuals.hyperlink_color = ACCENT_HOVER;
    visuals.error_fg_color = ERROR;
    visuals.warn_fg_color = WARN;
    visuals.window_stroke = Stroke::new(1.0, OUTLINE);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0, TEXT);

    // Neutral buttons: understated, leaving the blue for the primary ones.
    visuals.widgets.noninteractive = widget(SURFACE, OUTLINE, MUTED);
    visuals.widgets.inactive = widget(SURFACE, Color32::TRANSPARENT, TEXT);
    visuals.widgets.hovered = widget(SURFACE_HOVER, OUTLINE, TEXT);
    visuals.widgets.active = widget(SURFACE_HOVER, ACCENT, TEXT);
    visuals.widgets.open = widget(SURFACE, OUTLINE, TEXT);
}

fn widget(fill: Color32, stroke: Color32, text: Color32) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(1.0, stroke),
        fg_stroke: Stroke::new(1.0, text),
        corner_radius: CornerRadius::same(8),
        expansion: 0.0,
    }
}

/// Single-line text inside `rect`, vertically centred. If it does not fit the
/// width, it scrolls continuously from right to left.
pub fn marquee(ui: &egui::Ui, rect: egui::Rect, text: &str, font: FontId, color: Color32) {
    if text.is_empty() || rect.width() < 1.0 {
        return;
    }

    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    // The text must escape neither `rect` nor the list that contains it.
    let painter = ui
        .painter()
        .with_clip_rect(rect.intersect(ui.painter().clip_rect()));
    let y = rect.center().y - galley.size().y * 0.5;

    if galley.size().x <= rect.width() {
        painter.galley(egui::pos2(rect.left(), y), galley, color);
        return;
    }

    /// How many pixels per second the text scrolls.
    const SPEED: f32 = 32.0;
    /// The gap between the end of the text and its repetition.
    const GAP: f32 = 56.0;

    // Two copies, so the wrap-around does not show an empty line.
    let span = galley.size().x + GAP;
    let offset = (ui.input(|i| i.time) as f32 * SPEED).rem_euclid(span);
    let x = rect.left() - offset;
    painter.galley(egui::pos2(x, y), galley.clone(), color);
    painter.galley(egui::pos2(x + span, y), galley, color);
    ui.ctx().request_repaint();
}

/// Single-line text, cut with "…" when it does not fit.
pub fn one_line(
    painter: &egui::Painter,
    pos: egui::Pos2,
    anchor: egui::Align2,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap.max_width = max_width.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = painter.layout_job(job);
    let rect = anchor.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, color);
}
