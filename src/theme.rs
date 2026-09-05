//! The palettes and the setup of the egui style.
//!
//! A palette is data, not consts: `PRESETS` holds them all, `p()` hands out the
//! one in force, and `set` swaps it. Every colour in the app goes through `p()`,
//! so switching one is a store and a repaint.
//!
//! The default is "Aurora Night": everything dark, but the floor is a deep navy
//! rather than black. The old backdrop and surface sat some twelve steps of
//! lightness apart, so the rows of the list dissolved into the background; here
//! each layer is clearly a step above the one below it. The others follow the
//! same rule — read `Palette` for what each slot is for.

use std::sync::atomic::{AtomicUsize, Ordering};

use egui::style::WidgetVisuals;
use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

#[derive(Clone, Copy)]
pub struct Palette {
    /// Whether the palette is a dark one, which is all egui wants to know.
    pub dark: bool,
    /// The background of the whole window.
    pub backdrop: Color32,
    /// Cards, list rows, the search field.
    pub surface: Color32,
    pub surface_hover: Color32,
    /// The card of the station that is playing.
    pub card: Color32,
    pub card_outline: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    /// What goes *on top* of `accent`. The accent is a saturated colour, so the
    /// ordinary `text` would not read against it.
    pub on_accent: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub outline: Color32,
    /// The colour of "Live". Kept clear of `accent`, so the two saturated
    /// colours do not compete on the same card.
    pub live: Color32,
    /// The app name in the title bar, and the metadata of the track. The one
    /// colour outside the palette's family, which is what makes it read as an
    /// accent at all.
    pub title: Color32,
    /// The button that closes the window, and Stop.
    pub danger: Color32,
    /// `danger` under the pointer: the same lift `accent_hover` gives the accent.
    pub danger_hover: Color32,
    /// Failure messages. A lighter tint of `danger`, because a whole line of the
    /// saturated colour is hard to read on the backdrop.
    pub error: Color32,
    /// Warnings, for the egui widgets that ask for one.
    pub warn: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Deep navy, sky blue, and gold for the title. The app as it has always looked.
const AURORA: Palette = Palette {
    dark: true,
    backdrop: rgb(0x0F, 0x14, 0x24),
    surface: rgb(0x1A, 0x21, 0x38),
    surface_hover: rgb(0x25, 0x30, 0x52),
    card: rgb(0x10, 0x34, 0x4F),
    card_outline: rgb(0x2F, 0x7F, 0xB5),
    accent: rgb(0x38, 0xBD, 0xF8),
    accent_hover: rgb(0x7D, 0xD3, 0xFC),
    on_accent: rgb(0x08, 0x20, 0x32),
    text: rgb(0xE8, 0xEE, 0xF9),
    muted: rgb(0x8F, 0xA0, 0xBF),
    outline: rgb(0x2A, 0x35, 0x50),
    live: rgb(0x34, 0xD3, 0x99),
    title: rgb(0xFB, 0xBF, 0x24),
    danger: rgb(0xF4, 0x3F, 0x5E),
    danger_hover: rgb(0xFB, 0x71, 0x85),
    error: rgb(0xFD, 0xA4, 0xAF),
    warn: rgb(0xFC, 0xD3, 0x4D),
};

/// Neutral greys and amber. The warm one is the accent here, so the title takes
/// the cool colour instead — the two swap roles compared to Aurora.
const CARBON: Palette = Palette {
    dark: true,
    backdrop: rgb(0x12, 0x12, 0x12),
    surface: rgb(0x1E, 0x1E, 0x1E),
    surface_hover: rgb(0x2E, 0x2E, 0x2E),
    card: rgb(0x2A, 0x21, 0x18),
    card_outline: rgb(0xB4, 0x76, 0x2F),
    accent: rgb(0xF5, 0x9E, 0x0B),
    accent_hover: rgb(0xFB, 0xBF, 0x24),
    on_accent: rgb(0x1A, 0x12, 0x06),
    text: rgb(0xEC, 0xEC, 0xEC),
    muted: rgb(0x9A, 0x9A, 0x9A),
    outline: rgb(0x35, 0x35, 0x35),
    live: rgb(0x4A, 0xDE, 0x80),
    title: rgb(0x7D, 0xD3, 0xFC),
    danger: rgb(0xEF, 0x44, 0x44),
    danger_hover: rgb(0xF8, 0x71, 0x71),
    error: rgb(0xFC, 0xA5, 0xA5),
    warn: rgb(0xFC, 0xD3, 0x4D),
};

/// The light one. Every colour is darkened rather than lightened: on a white
/// floor it is the ink that has to carry, and the sky blue of Aurora would be
/// unreadable as text.
const DAYLIGHT: Palette = Palette {
    dark: false,
    backdrop: rgb(0xF4, 0xF6, 0xFA),
    surface: rgb(0xFF, 0xFF, 0xFF),
    surface_hover: rgb(0xE6, 0xEC, 0xF5),
    card: rgb(0xDC, 0xEB, 0xFA),
    card_outline: rgb(0x3B, 0x82, 0xF6),
    accent: rgb(0x0B, 0x72, 0xD0),
    accent_hover: rgb(0x1D, 0x8F, 0xE8),
    on_accent: rgb(0xFF, 0xFF, 0xFF),
    text: rgb(0x10, 0x18, 0x2A),
    muted: rgb(0x5B, 0x6B, 0x85),
    outline: rgb(0xC7, 0xD2, 0xE2),
    live: rgb(0x0E, 0x8F, 0x62),
    title: rgb(0xB4, 0x53, 0x09),
    danger: rgb(0xDC, 0x26, 0x26),
    danger_hover: rgb(0xB9, 0x1C, 0x1C),
    error: rgb(0xB9, 0x1C, 0x1C),
    warn: rgb(0xA1, 0x62, 0x07),
};

/// Black and green, for the people who like their radio in a terminal. The lime
/// of `live` is what keeps the badge apart from the green of the accent.
const TERMINAL: Palette = Palette {
    dark: true,
    backdrop: rgb(0x05, 0x08, 0x06),
    surface: rgb(0x0D, 0x14, 0x0F),
    surface_hover: rgb(0x18, 0x26, 0x1C),
    card: rgb(0x08, 0x21, 0x0F),
    card_outline: rgb(0x2F, 0x9E, 0x52),
    accent: rgb(0x22, 0xC5, 0x5E),
    accent_hover: rgb(0x4A, 0xDE, 0x80),
    on_accent: rgb(0x04, 0x14, 0x0A),
    text: rgb(0xD2, 0xF5, 0xDC),
    muted: rgb(0x7F, 0xA9, 0x8B),
    outline: rgb(0x1E, 0x2E, 0x22),
    live: rgb(0xA3, 0xE6, 0x35),
    title: rgb(0xE3, 0xB3, 0x41),
    danger: rgb(0xF4, 0x3F, 0x5E),
    danger_hover: rgb(0xFB, 0x71, 0x85),
    error: rgb(0xFD, 0xA4, 0xAF),
    warn: rgb(0xFC, 0xD3, 0x4D),
};

/// Nord: cold grey-blue, the frost cyan as the accent.
const NORD: Palette = Palette {
    dark: true,
    backdrop: rgb(0x2E, 0x34, 0x40),
    surface: rgb(0x3B, 0x42, 0x52),
    surface_hover: rgb(0x4A, 0x54, 0x69),
    card: rgb(0x35, 0x46, 0x5E),
    card_outline: rgb(0x5E, 0x81, 0xAC),
    accent: rgb(0x88, 0xC0, 0xD0),
    accent_hover: rgb(0xA6, 0xD8, 0xE4),
    on_accent: rgb(0x22, 0x28, 0x33),
    text: rgb(0xEC, 0xEF, 0xF4),
    muted: rgb(0xA6, 0xB1, 0xC4),
    outline: rgb(0x43, 0x4C, 0x5E),
    live: rgb(0xA3, 0xBE, 0x8C),
    title: rgb(0xEB, 0xCB, 0x8B),
    danger: rgb(0xBF, 0x61, 0x6A),
    danger_hover: rgb(0xD3, 0x81, 0x89),
    error: rgb(0xE0, 0xA3, 0xA8),
    warn: rgb(0xEB, 0xCB, 0x8B),
};

/// Dracula: the purple one. Its comment grey is too dark to read a subtitle in,
/// so `muted` is a step lighter than the palette's own.
const DRACULA: Palette = Palette {
    dark: true,
    backdrop: rgb(0x28, 0x2A, 0x36),
    surface: rgb(0x34, 0x37, 0x46),
    surface_hover: rgb(0x44, 0x47, 0x5A),
    card: rgb(0x3B, 0x33, 0x55),
    card_outline: rgb(0xBD, 0x93, 0xF9),
    accent: rgb(0xBD, 0x93, 0xF9),
    accent_hover: rgb(0xD6, 0xBC, 0xFA),
    on_accent: rgb(0x1E, 0x1F, 0x29),
    text: rgb(0xF8, 0xF8, 0xF2),
    muted: rgb(0xA3, 0xAA, 0xC8),
    outline: rgb(0x44, 0x47, 0x5A),
    live: rgb(0x50, 0xFA, 0x7B),
    title: rgb(0xF1, 0xFA, 0x8C),
    danger: rgb(0xFF, 0x55, 0x55),
    danger_hover: rgb(0xFF, 0x7B, 0x7B),
    error: rgb(0xFF, 0x9C, 0x9C),
    warn: rgb(0xFF, 0xB8, 0x6C),
};

/// Gruvbox: warm and low-contrast, orange for the accent and the blue for the
/// title — the one cool colour in it.
const GRUVBOX: Palette = Palette {
    dark: true,
    backdrop: rgb(0x1D, 0x20, 0x21),
    surface: rgb(0x28, 0x28, 0x28),
    surface_hover: rgb(0x3C, 0x38, 0x36),
    card: rgb(0x38, 0x2C, 0x1F),
    card_outline: rgb(0xD7, 0x99, 0x21),
    accent: rgb(0xFE, 0x80, 0x19),
    accent_hover: rgb(0xFE, 0xA9, 0x6A),
    on_accent: rgb(0x1D, 0x20, 0x21),
    text: rgb(0xEB, 0xDB, 0xB2),
    muted: rgb(0xA8, 0x99, 0x84),
    outline: rgb(0x3C, 0x38, 0x36),
    live: rgb(0xB8, 0xBB, 0x26),
    title: rgb(0x83, 0xA5, 0x98),
    danger: rgb(0xFB, 0x49, 0x34),
    danger_hover: rgb(0xFE, 0x7F, 0x70),
    error: rgb(0xFB, 0x9F, 0x93),
    warn: rgb(0xFA, 0xBD, 0x2F),
};

/// Solarized Dark: teal on the old ink-blue, unchanged since 2011.
const SOLARIZED: Palette = Palette {
    dark: true,
    backdrop: rgb(0x00, 0x2B, 0x36),
    surface: rgb(0x07, 0x36, 0x42),
    surface_hover: rgb(0x0E, 0x4A, 0x5A),
    card: rgb(0x05, 0x3F, 0x4B),
    card_outline: rgb(0x2A, 0xA1, 0x98),
    accent: rgb(0x2A, 0xA1, 0x98),
    accent_hover: rgb(0x4F, 0xC3, 0xBA),
    on_accent: rgb(0x00, 0x22, 0x2B),
    text: rgb(0xEE, 0xE8, 0xD5),
    muted: rgb(0x93, 0xA1, 0xA1),
    outline: rgb(0x0E, 0x4A, 0x5A),
    live: rgb(0x85, 0x99, 0x00),
    title: rgb(0xB5, 0x89, 0x00),
    danger: rgb(0xDC, 0x32, 0x2F),
    danger_hover: rgb(0xE8, 0x63, 0x5F),
    error: rgb(0xF0, 0x91, 0x8E),
    warn: rgb(0xCB, 0x4B, 0x16),
};

/// Ember: near-black with a coal-fire orange. The darkest of the set.
const EMBER: Palette = Palette {
    dark: true,
    backdrop: rgb(0x14, 0x10, 0x0F),
    surface: rgb(0x20, 0x1A, 0x18),
    surface_hover: rgb(0x2E, 0x25, 0x23),
    card: rgb(0x33, 0x1C, 0x1A),
    card_outline: rgb(0xB5, 0x45, 0x3C),
    accent: rgb(0xFF, 0x6B, 0x4A),
    accent_hover: rgb(0xFF, 0x8F, 0x73),
    on_accent: rgb(0x1A, 0x0C, 0x08),
    text: rgb(0xF2, 0xE7, 0xE3),
    muted: rgb(0xB2, 0x9A, 0x93),
    outline: rgb(0x33, 0x29, 0x26),
    live: rgb(0x6F, 0xCF, 0x97),
    title: rgb(0xF2, 0xC1, 0x4E),
    danger: rgb(0xE5, 0x48, 0x4D),
    danger_hover: rgb(0xF0, 0x6D, 0x71),
    error: rgb(0xF5, 0xA3, 0xA5),
    warn: rgb(0xF2, 0xC1, 0x4E),
};

/// Slate: plain blue-grey, no colour anywhere but the accent. For when the rest
/// of them are too much.
const SLATE: Palette = Palette {
    dark: true,
    backdrop: rgb(0x17, 0x1A, 0x1F),
    surface: rgb(0x22, 0x26, 0x2D),
    surface_hover: rgb(0x30, 0x36, 0x40),
    card: rgb(0x27, 0x2D, 0x37),
    card_outline: rgb(0x64, 0x74, 0x8B),
    accent: rgb(0x94, 0xA3, 0xB8),
    accent_hover: rgb(0xB8, 0xC5, 0xD2),
    on_accent: rgb(0x15, 0x18, 0x1D),
    text: rgb(0xE5, 0xE9, 0xEF),
    muted: rgb(0x8C, 0x97, 0xA8),
    outline: rgb(0x2F, 0x35, 0x3E),
    live: rgb(0x7C, 0xC4, 0x9B),
    title: rgb(0xD9, 0xB3, 0x87),
    danger: rgb(0xD1, 0x6B, 0x72),
    danger_hover: rgb(0xE2, 0x8D, 0x93),
    error: rgb(0xEB, 0xAF, 0xB3),
    warn: rgb(0xD9, 0xB3, 0x87),
};

/// Every palette the app carries. The name is what the settings store and what
/// the picker lists, so this one array is the whole registry: nothing else has
/// to learn about a palette added here.
pub const PRESETS: [(&str, Palette); 10] = [
    ("Aurora Night", AURORA),
    ("Nord", NORD),
    ("Dracula", DRACULA),
    ("Gruvbox", GRUVBOX),
    ("Solarized", SOLARIZED),
    ("Ember", EMBER),
    ("Slate", SLATE),
    ("Carbon", CARBON),
    ("Terminal", TERMINAL),
    // The one light palette, last, where it is easy to skip past.
    ("Daylight", DAYLIGHT),
];

/// Which of `PRESETS` is in force. An index rather than the palette itself: the
/// palettes are `'static`, so the switch is one relaxed store and `p()` stays a
/// load — it is called some ninety times a frame.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// The palette in force.
pub fn p() -> &'static Palette {
    &PRESETS[CURRENT.load(Ordering::Relaxed).min(PRESETS.len() - 1)].1
}

/// Its index, which is what the picker ticks.
pub fn current() -> usize {
    CURRENT.load(Ordering::Relaxed)
}

/// Switches to the palette of that name, falling back to the first — a settings
/// file naming a palette that no longer exists must not leave the app blank.
/// Call `restyle` afterwards unless egui has not been built yet.
pub fn set(name: &str) {
    let i = PRESETS.iter().position(|(n, _)| *n == name).unwrap_or(0);
    CURRENT.store(i, Ordering::Relaxed);
}

/// Rounded corners for cards and rows.
pub const ROUND: CornerRadius = CornerRadius::same(10);

/// Loads the fonts and paints the style. Once, at startup.
pub fn apply(ctx: &egui::Context) {
    if let Some(fonts) = system_fonts() {
        ctx.set_fonts(fonts);
    }
    restyle(ctx);
}

/// Writes the palette in force into egui's style. Called again on every switch:
/// egui keeps the style it was handed, so a new palette shows nowhere until this
/// runs.
pub fn restyle(ctx: &egui::Context) {
    // One palette is written into both egui themes, so nothing of ours can flip
    // when the system theme does.
    ctx.set_theme(if p().dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    ctx.all_styles_mut(configure);
}

/// On Windows the app reads the faces Windows already has on disk instead of
/// carrying its own: Segoe UI is the typeface the rest of the desktop is set in,
/// so the app stops looking like a port, and the 1.4MB of Ubuntu, Hack and the
/// two emoji fonts that egui bundles comes out of the executable — a fifth of it.
///
/// Nothing is copied into the binary: the files are read at startup, which is
/// why only the two that are actually drawn from get opened.
///
/// Everywhere else this is `None` and egui's bundled fonts stand, which is why
/// `default_fonts` is still on for every target but this one.
///
/// If a face will not load we return `None` rather than half a font set, and
/// egui falls back to its own. On Windows that fallback is compiled out, so the
/// text would be missing — hence the whole set is required, never a part of it.
#[cfg(windows)]
fn system_fonts() -> Option<egui::FontDefinitions> {
    // The directory is asked for rather than assumed: Windows is not always on C:.
    let dir = std::path::PathBuf::from(std::env::var_os("SystemRoot")?).join("Fonts");
    let load = |file: &str| std::fs::read(dir.join(file)).ok();

    // Segoe UI carries the text — Latin, Greek and Cyrillic, which is what
    // station names come in. Segoe UI Symbol carries every glyph the app draws a
    // button with: ▶ ⏹ ✕ ✖ 🔊 🔇 🔍.
    //
    // ponytail: Segoe UI Emoji covers those too and is not loaded, because it is
    // 12.9MB — reading it cost ~37MB of working set to draw the same seven
    // glyphs in colour, which this flat, two-tone window does not want anyway.
    // If a station name ever needs a real colour emoji, that is the file to add.
    let faces = [
        ("segoe-ui", load("segoeui.ttf")?),
        ("segoe-ui-symbol", load("seguisym.ttf")?),
    ];

    let mut fonts = egui::FontDefinitions::empty();
    for (name, bytes) in faces {
        fonts.font_data.insert(
            name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        // Both families get both faces, in the same order: the app asks for
        // Monospace in its text styles, and a family egui cannot fill panics.
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push(name.to_owned());
        }
    }
    Some(fonts)
}

#[cfg(not(windows))]
fn system_fonts() -> Option<egui::FontDefinitions> {
    None
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

    let pal = p();
    let visuals = &mut style.visuals;
    visuals.dark_mode = pal.dark;
    visuals.panel_fill = pal.backdrop;
    visuals.window_fill = pal.backdrop;
    visuals.faint_bg_color = pal.surface;
    visuals.extreme_bg_color = pal.surface;
    visuals.hyperlink_color = pal.accent_hover;
    visuals.error_fg_color = pal.error;
    visuals.warn_fg_color = pal.warn;
    visuals.window_stroke = Stroke::new(1.0, pal.outline);
    visuals.selection.bg_fill = pal.accent.gamma_multiply(0.45);
    visuals.selection.stroke = Stroke::new(1.0, pal.text);

    // Neutral buttons: understated, leaving the accent for the primary ones.
    visuals.widgets.noninteractive = widget(pal.surface, pal.outline, pal.muted);
    visuals.widgets.inactive = widget(pal.surface, Color32::TRANSPARENT, pal.text);
    visuals.widgets.hovered = widget(pal.surface_hover, pal.outline, pal.text);
    visuals.widgets.active = widget(pal.surface_hover, pal.accent, pal.text);
    visuals.widgets.open = widget(pal.surface, pal.outline, pal.text);
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
/// width, it is cut with "…" and scrolls only while the pointer is over it.
pub fn marquee(ui: &egui::Ui, rect: egui::Rect, text: &str, font: FontId, color: Color32) {
    if text.is_empty() || rect.width() < 1.0 {
        return;
    }

    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), color);
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

    // At rest the text is simply cut, and the app asks for no frames at all.
    // Scrolling is what costs: egui has no partial redraw, so every frame the
    // marquee wants repaints and recomposites the whole window. Paying for that
    // while nobody is looking at the text is what kept the GPU busy for as long
    // as a station played.
    //
    // ponytail: hover is tested per line, not per row or card — pointing at the
    // title scrolls the title, not the line under it. If that feels finicky, give
    // `marquee` a `scroll: bool` and hand it the row's existing `hovered`.
    let id = egui::Id::new(("marquee", rect.min.x.to_bits(), rect.min.y.to_bits()));
    if !ui.rect_contains_pointer(rect) {
        // Forget where it had scrolled to, so the next hover starts from the left.
        ui.ctx().data_mut(|d| d.remove::<f64>(id));
        one_line(
            &painter,
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            font,
            color,
            rect.width(),
        );
        return;
    }

    // Measured from when the pointer arrived, not from the clock: the absolute
    // time would drop the reader into the middle of a word.
    let now = ui.input(|i| i.time);
    let started = ui
        .ctx()
        .data_mut(|d| *d.get_temp_mut_or_insert_with(id, || now));

    // Two copies, so the wrap-around does not show an empty line.
    let span = galley.size().x + GAP;
    let offset = ((now - started) as f32 * SPEED).rem_euclid(span);
    let x = rect.left() - offset;
    painter.galley(egui::pos2(x, y), galley.clone(), color);
    painter.galley(egui::pos2(x + span, y), galley, color);
    // The text moves one pixel every 31ms at this speed, so 30fps is exactly as
    // smooth as it needs to be, and no smoother.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(33));
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

#[cfg(test)]
mod palette_tests {
    /// The settings hold a name, so an old or misspelt one must land somewhere
    /// rather than nowhere — and the palettes must not all be the same colour.
    #[test]
    fn falls_back_to_the_first_palette() {
        super::set("Daylight");
        assert!(!super::p().dark);
        super::set("a palette that was dropped two versions ago");
        assert_eq!(super::current(), 0);
        assert!(super::p().dark);
    }
}

#[cfg(all(test, windows))]
mod tests {
    /// The Windows build carries no fonts of its own, so if this ever comes back
    /// `None` the window renders no text at all. Worth one test.
    #[test]
    fn finds_every_face_windows_is_supposed_to_have() {
        let fonts = super::system_fonts().expect("Segoe UI should be on a Windows box");
        assert_eq!(fonts.font_data.len(), 2);
        // Both families are filled: a text style pointing at an empty one panics.
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            assert_eq!(
                fonts.families[&family].len(),
                2,
                "{family:?} is short a face"
            );
        }
    }
}
