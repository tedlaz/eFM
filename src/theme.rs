//! The dark/purple palette and the setup of the egui style.

use egui::style::WidgetVisuals;
use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

/// The background of the whole window.
pub const BACKDROP: Color32 = Color32::from_rgb(0x0B, 0x0A, 0x10);
/// Cards, list rows, the search field.
pub const SURFACE: Color32 = Color32::from_rgb(0x17, 0x15, 0x20);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x23, 0x20, 0x31);
/// The card of the station that is playing.
pub const CARD: Color32 = Color32::from_rgb(0x25, 0x12, 0x4A);
pub const CARD_OUTLINE: Color32 = Color32::from_rgb(0x4B, 0x2A, 0x8C);
pub const ACCENT: Color32 = Color32::from_rgb(0x7C, 0x3A, 0xED);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x93, 0x66, 0xF8);
pub const TEXT: Color32 = Color32::from_rgb(0xE9, 0xE7, 0xF2);
pub const MUTED: Color32 = Color32::from_rgb(0x87, 0x83, 0x9B);
pub const OUTLINE: Color32 = Color32::from_rgb(0x2C, 0x27, 0x3D);
/// The green of "Live". A saturated emerald, at the same intensity as the purple
/// of `ACCENT` — the old pastel looked washed out next to the purple card.
pub const LIVE: Color32 = Color32::from_rgb(0x10, 0xB9, 0x81);
/// The app name in the title bar, and the metadata of the track.
pub const TITLE: Color32 = Color32::from_rgb(0xE7, 0xC3, 0x4B);
/// The red of the button that closes the window.
pub const DANGER: Color32 = Color32::from_rgb(0xE8, 0x11, 0x23);

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
    visuals.error_fg_color = Color32::from_rgb(0xF4, 0x7A, 0x7A);
    visuals.warn_fg_color = Color32::from_rgb(0xE2, 0xB5, 0x4F);
    visuals.window_stroke = Stroke::new(1.0, OUTLINE);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0, TEXT);

    // Neutral buttons: understated, leaving the purple for the primary ones.
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
