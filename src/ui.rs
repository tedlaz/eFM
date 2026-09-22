//! The interface: our own title bar, playback card, address bar and station list.

use egui::{
    Align, Align2, Color32, CornerRadius, FontFamily, FontId, Layout, Rect, RichText, Sense,
    Stroke, StrokeKind, pos2, vec2,
};

use crate::App;
use crate::player::{NowPlaying, Status};
use crate::search;
use crate::theme;

/// The margin on the left and right of the whole page.
const PAGE_PAD: f32 = 12.0;
const TITLEBAR_H: f32 = 40.0;
const ROW_H: f32 = 56.0;

/// What the user asked for in this frame. We collect it and run it at the end, so
/// the buttons do not borrow `App` while they are being drawn.
enum Action {
    /// A station of the list: it is an address, so it plays.
    Play(String),
    /// Whatever was typed: an address plays, anything else opens the search.
    Submit(String),
    /// Drops the stream: the next Play connects afresh.
    Stop,
    Forget(String),
    Export,
    Import,
    /// A palette from `theme::PRESETS`, by index.
    Theme(usize),
    /// The stations that were ticked in the search window, as (url, name).
    AddFound(Vec<(String, String)>),
    CloseSearch,
}

/// How long the export/import message stays on screen.
const NOTE_TTL: std::time::Duration = std::time::Duration::from_secs(4);

pub fn draw(app: &mut App, root: &mut egui::Ui) {
    // A fold asked for last frame. It waits until here so that the frame the
    // desktop stretches over the resize is the blank one — see `cover`.
    if let Some(height) = app.pending_fold.take() {
        ask_height(app, root.ctx(), height);
    }
    let snapshot = app.player.as_ref().map(|p| p.state());
    let status = snapshot.as_ref().map_or(Status::Idle, |s| s.status.clone());
    let now = snapshot.map(|s| s.now).unwrap_or_default();
    let mut action = None;

    app.note_station_name(&now);

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme::p().backdrop))
        .show(root, |ui| {
            draw_titlebar(ui, &status, &mut action);

            egui::Frame::NONE
                // Bottom margin as wide as the sides: otherwise the scroll bar
                // ran all the way into the bottom right corner of the window.
                .inner_margin(egui::Margin {
                    left: PAGE_PAD as i8,
                    right: PAGE_PAD as i8,
                    top: 0,
                    bottom: PAGE_PAD as i8,
                })
                .show(ui, |ui| {
                    draw_hero(app, ui, &now, &status, &mut action);
                    // The card ends here, and under it the margin: that is the
                    // whole of the folded window. It is measured rather than
                    // assumed, and kept, because the button that folds the
                    // window needs it at the moment it is clicked.
                    app.folded_height = ui.min_rect().bottom() + PAGE_PAD;

                    // Folded away, the lower half is not drawn at all.
                    if app.unfolded {
                        ui.add_space(10.0);
                        draw_address_bar(app, ui, &mut action);
                        ui.add_space(10.0);
                        draw_stations(app, ui, &now, &status, &mut action);
                    }
                });
        });

    fold_window(app, root.ctx());
    cover(app, root.ctx());

    draw_search_window(app, root.ctx(), &mut action);

    if let Some(action) = action {
        apply(app, action, root.ctx());
    }
}

/// The title bar. Dragging moves the window, a double click maximises it. While
/// a station is on the air it also carries the live indicator, over on the right
/// next to the window buttons.
fn draw_titlebar(ui: &mut egui::Ui, status: &Status, action: &mut Option<Action>) {
    let (rect, drag) = ui.allocate_exact_size(
        vec2(ui.available_width(), TITLEBAR_H),
        Sense::click_and_drag(),
    );

    let painter = ui.painter().clone();
    let cy = rect.center().y;
    /// The tallest bar of the mark. The title bar is 40px, so this leaves it
    /// breathing room above and below.
    const LOGO_H: f32 = 20.0;
    let half = logo_width(LOGO_H) * 0.5;
    let logo = pos2(rect.left() + PAGE_PAD + half, cy);
    app_logo(&painter, logo, LOGO_H);
    // The version comes from Cargo.toml at compile time, so bumping the release
    // is enough — there is no second place to keep in sync.
    theme::one_line(
        &painter,
        pos2(logo.x + half + 9.0, cy),
        Align2::LEFT_CENTER,
        concat!("eFM v", env!("CARGO_PKG_VERSION")),
        FontId::new(17.0, FontFamily::Proportional),
        theme::p().title,
        200.0,
    );

    // "✖" and not "✕": the latter is missing from the egui font and comes out as a box.
    let close = titlebar_button(
        ui,
        pos2(rect.right() - PAGE_PAD - 16.0, cy),
        "✖",
        theme::p().danger,
    );
    let minimize = titlebar_button(
        ui,
        pos2(rect.right() - PAGE_PAD - 52.0, cy),
        "—",
        theme::p().surface_hover,
    );
    // "◐": a half-filled disc, which is what every app that swaps palettes uses.
    let (palette, picked) = theme_button(ui, pos2(rect.right() - PAGE_PAD - 88.0, cy));
    if let Some(i) = picked {
        *action = Some(Action::Theme(i));
    }

    // The indicator sits just left of the window buttons: right of the middle of
    // the bar, but still clearly apart from them.
    if *status == Status::Playing {
        // The meter only moves while someone is there to see it. Animating it in
        // the background woke the whole window ten times a second for as long as
        // a station played — and playing in the background is what this app is
        // mostly for. At a fixed zero the four bars stand at four different
        // heights, so at rest it still reads as a meter rather than a glitch.
        let focused = ui.input(|i| i.focused);
        let time = if focused { ui.input(|i| i.time) } else { 0.0 };
        live_badge(&painter, pos2(palette.rect.left() - 14.0, cy), time);
        if focused {
            // Nothing else would ask for the next frame while the window sits
            // still. The meter is slow; a few frames a second are enough.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    if close.clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
    if minimize.clicked() {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }

    // Dragging moves the window — unless it started on a button.
    let on_button = close.hovered() || minimize.hovered() || palette.hovered();
    if drag.drag_started_by(egui::PointerButton::Primary) && !on_button {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag.double_clicked_by(egui::PointerButton::Primary) && !on_button {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}

/// The app mark: the level meter of the window icon, without the disc around it.
///
/// The disc is what makes the icon cohere as a tile on a desktop; in here the
/// title bar is already the container, and leaving it out buys the bars enough
/// room to read. Inside a 22px disc they did not: three bars of near-equal
/// height flanking a taller one turn into a pause symbol, which is the one thing
/// a player must not say about itself.
///
/// `height` is the tallest bar; everything else is a fraction of it. The rising
/// then falling shape matters as much as the size — a symmetric pair is exactly
/// what reads as pause, an uneven run reads as a meter.
fn app_logo(painter: &egui::Painter, centre: egui::Pos2, height: f32) {
    /// Bar heights as fractions of `height`.
    const HEIGHTS: [f32; 4] = [0.45, 0.75, 1.0, 0.60];
    /// Which one carries the gold, as the tall bar does in the icon.
    const ACCENT_BAR: usize = 2;
    /// Bar width and the gap between two, in the same units.
    const BAR: f32 = 0.20;
    const GAP: f32 = 0.125;

    let (w, gap) = (height * BAR, height * GAP);
    let left = centre.x - logo_width(height) * 0.5 + w * 0.5;

    for (i, factor) in HEIGHTS.iter().enumerate() {
        let bar = Rect::from_center_size(
            pos2(left + i as f32 * (w + gap), centre.y),
            vec2(w, height * factor),
        );
        painter.rect_filled(
            bar,
            CornerRadius::same((w * 0.5).round() as u8),
            if i == ACCENT_BAR {
                theme::p().title
            } else {
                theme::p().accent
            },
        );
    }
}

/// How wide `app_logo` draws, so the title bar can place what follows it.
fn logo_width(height: f32) -> f32 {
    4.0 * (height * 0.20) + 3.0 * (height * 0.125)
}

/// A title bar button. On hover the tile behind it fades in, and the glyph turns
/// white and grows slightly.
fn titlebar_button(
    ui: &mut egui::Ui,
    center: egui::Pos2,
    glyph: &str,
    hover_fill: Color32,
) -> egui::Response {
    let rect = Rect::from_center_size(center, vec2(32.0, 26.0));
    let response = ui.interact(rect, ui.id().with(glyph), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id, response.hovered(), 0.14);

    let painter = ui.painter();
    if t > 0.0 {
        // The tile "opens up" towards its final size as it fades in.
        let tile = rect.expand(egui::lerp(-4.0..=0.0, t));
        painter.rect_filled(tile, CornerRadius::same(6), hover_fill.gamma_multiply(t));
    }
    theme::one_line(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        font(egui::lerp(12.5..=14.0, t)),
        theme::p().muted.lerp_to_gamma(theme::p().text, t),
        rect.width(),
    );

    response
}

/// The palette picker: a title bar button with the list of `theme::PRESETS`
/// under it. Hands back its own response — the title bar needs it to place the
/// live meter and to know that a drag started on a button — and the palette the
/// user picked, if they picked one.
fn theme_button(ui: &mut egui::Ui, center: egui::Pos2) -> (egui::Response, Option<usize>) {
    let response = titlebar_button(ui, center, "◐", theme::p().surface_hover);
    let current = theme::current();
    let picked = egui::Popup::menu(&response)
        .show(|ui| {
            // Two short columns rather than one tall one: egui cannot paint past
            // the edge of the window, and folded, the window is shorter than the
            // ten palettes stacked up.
            ui.spacing_mut().button_padding.y = 4.0;
            ui.spacing_mut().item_spacing.y = 2.0;
            let mut picked = None;
            let rows = theme::PRESETS.len().div_ceil(2);
            ui.horizontal_top(|ui| {
                for (column, presets) in theme::PRESETS.chunks(rows).enumerate() {
                    ui.vertical(|ui| {
                        for (row, (name, _)) in presets.iter().enumerate() {
                            let i = column * rows + row;
                            if ui.selectable_label(i == current, *name).clicked() {
                                picked = Some(i);
                            }
                        }
                    });
                }
            });
            if picked.is_some() {
                ui.close();
            }
            picked
        })
        .and_then(|menu| menu.inner);
    (response, picked)
}

/// Takes the window to the height the drawer asks for, in one step.
///
/// There is no animation here on purpose. The height of a window cannot be
/// animated smoothly: every resize costs the app a frame — measured at some 90ms
/// on Windows against 5ms for an ordinary one — and for as long as it lasts the
/// desktop shows the frame it was last given, stretched to the new size. That
/// drags the title bar and the card into a motion that is supposed to belong to
/// the lower half alone. One step is over in one such frame; a slide would have
/// paid that price over and over.
///
fn fold_window(app: &mut App, ctx: &egui::Context) {
    let height = window_size(ctx).y;

    if let Some((asked, before)) = app.asked_height {
        // Wait for an answer: the height we asked for, or any change at all — a
        // compositor is free to have ideas of its own, and asking again on every
        // frame would be a resize storm.
        if (height - asked).abs() <= 1.0 || (height - before).abs() > 1.0 {
            app.asked_height = None;
        }
        return;
    }

    if app.unfolded {
        // Open and standing still, the height belongs to the user: they may drag
        // the window taller, and all we do is remember what they chose so it
        // opens back to it. A maximised window is not a choice of height, so it
        // is not one worth keeping.
        if !ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            app.unfolded_height = height;
        }
    } else if !app.folded_trimmed && (height - app.folded_height).abs() > 1.0 {
        // The window is born at a guessed height, because how tall the card
        // comes out is only known once it has been laid out. This trims it to
        // the real one, once.
        ask_height(app, ctx, app.folded_height);
    }
}

/// Paints the whole window flat while it is being resized.
///
/// A resize costs the app a frame, and for as long as it lasts the desktop shows
/// the frame it was last given, stretched to the new size — so the title bar, the
/// card and every line of text got squashed or pulled for some 90ms, which is the
/// ugly part of folding the drawer. Stretching a plain rectangle of the backdrop
/// looks like nothing at all, so that is what the desktop gets: the frame in
/// which the fold is asked for, and the ones until the new size arrives, are
/// covered over. The content underneath is still laid out — the fold needs the
/// heights it measures.
///
/// The deadline is what makes this safe: a compositor that never answers the
/// resize would otherwise leave the window blank for good.
fn cover(app: &App, ctx: &egui::Context) {
    let waiting = app.asked_height.is_some()
        && app
            .cover_until
            .is_some_and(|until| std::time::Instant::now() < until);
    if !(app.pending_fold.is_some() || waiting) {
        return;
    }
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("fold-cover"),
    ))
    .rect_filled(ctx.viewport_rect(), CornerRadius::ZERO, theme::p().backdrop);
    // Nothing else is asking for the frame that takes the cover off again.
    ctx.request_repaint();
}
/// Asks the window for a height, and remembers what it was when we asked so
/// [`fold_window`] can tell an answer from silence.
fn ask_height(app: &mut App, ctx: &egui::Context, height: f32) {
    let size = window_size(ctx);
    app.asked_height = Some((height, size.y));
    // Long enough for the ~90ms a resize takes on Windows, and short enough that
    // a resize that never lands is a flicker rather than a blank window.
    app.cover_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(250));
    app.folded_trimmed = true;
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(size.x, height)));
}

/// How big the window is, in points.
///
/// This is the rectangle egui itself is drawing into, rather than
/// `viewport().inner_rect`, which is the obvious source and the wrong one: winit
/// will not tell a Wayland client where its window sits, so on Wayland that
/// whole rectangle is `None` — and the drawer could not open at all, because the
/// resize was never asked for.
fn window_size(ctx: &egui::Context) -> egui::Vec2 {
    ctx.viewport_rect().size()
}

/// The purple card with whatever is playing now, and the controls.
fn draw_hero(
    app: &mut App,
    ui: &mut egui::Ui,
    now: &NowPlaying,
    status: &Status,
    action: &mut Option<Action>,
) {
    egui::Frame::NONE
        .fill(theme::p().card)
        .stroke(Stroke::new(1.0, theme::p().card_outline))
        .corner_radius(theme::ROUND)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            let (headline, tint) = headline(app, now, status);
            line(ui, &headline, font(17.0), tint);

            ui.add_space(2.0);
            let (detail, detail_color) = detail_line(now, status);
            line(ui, &detail, font(12.5), detail_color);

            ui.add_space(12.0);
            draw_hero_buttons(app, ui, status, action);
        });
}

fn draw_hero_buttons(
    app: &mut App,
    ui: &mut egui::Ui,
    status: &Status,
    action: &mut Option<Action>,
) {
    let has_audio = app.player.is_some();
    let has_url = !app.pending_url().is_empty();
    // Connecting counts as live here: the button is the way out of a connection
    // that hangs, otherwise a second press would only start a third one.
    let live = status.is_active();

    ui.horizontal(|ui| {
        // Volume on the left, in the space the old "Stop" button left behind.
        if mute_button(ui, app.muted, has_audio).clicked() {
            app.muted = !app.muted;
            app.apply_volume();
        }
        let slider = ui.add_enabled(
            has_audio,
            egui::Slider::new(&mut app.config.volume, 0.0..=1.5)
                .show_value(false)
                .max_decimals(2),
        );
        if slider.changed() {
            // Dragging the slider lifts the mute: otherwise you would be pulling
            // the volume around without hearing a thing.
            app.muted = false;
            app.apply_volume();
        }
        if slider.drag_stopped() {
            app.config.save();
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let can_press = has_audio && (live || has_url);
            if play_button(ui, status, can_press).clicked() {
                *action = Some(if live {
                    Action::Stop
                } else {
                    Action::Submit(app.pending_url().to_owned())
                });
            }

            // The drawer handle sits just left of the play button, on the same row.
            ui.add_space(8.0);
            if fold_button(ui, app.unfolded).clicked() {
                app.unfolded = !app.unfolded;
                // Asked for, not done: the resize happens at the top of the next
                // frame, once this one has gone out blank.
                app.pending_fold = Some(if app.unfolded {
                    app.unfolded_height
                } else {
                    app.folded_height
                });
            }
        });
    });
}

/// The volume glyph, which doubles as the mute switch.
fn mute_button(ui: &mut egui::Ui, silent: bool, enabled: bool) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 26.0), sense);
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id, enabled && response.hovered(), 0.12);

    let color = if !enabled {
        theme::p().outline
    } else {
        theme::p().muted.lerp_to_gamma(theme::p().text, t)
    };
    let painter = ui.painter();
    // Always the same speaker; the red diagonal on top is what shows the mute.
    // The ready-made "🔇" would be a heavy little circle, out of place next to
    // the thin "🔊".
    theme::one_line(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        "🔊",
        font(13.0),
        color,
        rect.width(),
    );
    if silent {
        let c = rect.center();
        let d = 9.0;
        painter.line_segment(
            [pos2(c.x - d, c.y - d), pos2(c.x + d, c.y + d)],
            Stroke::new(2.0, theme::p().danger),
        );
    }

    if !enabled {
        return response;
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if silent { "Unmute" } else { "Mute" })
}

/// The playback button. It only ever says what a click will do: Stop while the
/// stream plays, in red, and Play in blue otherwise. That the station is on the
/// air is the title bar's job to show, not this one's — so the green is gone from
/// here. Fixed width, so the row does not shift as the label changes.
fn play_button(ui: &mut egui::Ui, status: &Status, enabled: bool) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(104.0, 30.0), sense);

    let hot = enabled && response.hovered();
    let t = ui.ctx().animate_bool_with_time(response.id, hot, 0.12);
    // A connection on its way is stoppable too, and the button says so.
    let live = status.is_active();

    let (fill, text, label) = if !enabled {
        (theme::p().surface, theme::p().muted, "▶  Play")
    } else if live {
        (
            theme::p().danger.lerp_to_gamma(theme::p().danger_hover, t),
            theme::p().text,
            "⏹  Stop",
        )
    } else {
        (
            theme::p().accent.lerp_to_gamma(theme::p().accent_hover, t),
            theme::p().text,
            "▶  Play",
        )
    };

    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(8), fill);
    theme::one_line(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font(14.0),
        text,
        rect.width() - 10.0,
    );

    response
}

/// The live indicator of the title bar: the bouncing meter and the word "Live"
/// beside it. `right` is the middle of the group's right edge, so it can be hung
/// off whatever sits to its right.
fn live_badge(painter: &egui::Painter, right: egui::Pos2, time: f64) {
    let label_font = font(12.5);
    let galley = painter.layout_no_wrap("Live".to_owned(), label_font.clone(), theme::p().live);
    let left = right.x - (METER_WIDTH + METER_PAD + galley.size().x);

    live_meter(painter, pos2(left, right.y), theme::p().live, time);
    theme::one_line(
        painter,
        pos2(left + METER_WIDTH + METER_PAD, right.y),
        Align2::LEFT_CENTER,
        "Live",
        label_font,
        theme::p().live,
        galley.size().x + 1.0,
    );
}

/// The handle of the drawer: it pulls the address bar and the station list out
/// from under the card, and puts them back. The chevron points down at what is
/// hidden, and up once the list is out.
fn fold_button(ui: &mut egui::Ui, unfolded: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(38.0, 30.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id, response.hovered(), 0.12);

    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(8),
        // A well sunk into the card, which lifts towards the outline on hover.
        theme::p()
            .backdrop
            .lerp_to_gamma(theme::p().card_outline, 0.18 + 0.22 * t),
        Stroke::new(1.0, theme::p().card_outline),
        StrokeKind::Inside,
    );

    // Drawn as two strokes rather than a glyph: the chevrons of the default font
    // sit off centre.
    const HALF_W: f32 = 5.5;
    const HALF_H: f32 = 3.0;
    let c = rect.center();
    // Down while the list is hidden, up while it is showing.
    let tip = if unfolded { -HALF_H } else { HALF_H };
    let stroke = Stroke::new(2.0, theme::p().muted.lerp_to_gamma(theme::p().text, t));
    painter.line_segment(
        [pos2(c.x - HALF_W, c.y - tip), pos2(c.x, c.y + tip)],
        stroke,
    );
    painter.line_segment(
        [pos2(c.x, c.y + tip), pos2(c.x + HALF_W, c.y - tip)],
        stroke,
    );

    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if unfolded {
            "Hide the stations"
        } else {
            "Show the stations"
        })
}

/// One bar of the level meter, and the gap to the next one.
const METER_BAR: f32 = 2.0;
const METER_GAP: f32 = 2.0;
/// The meter as a whole: four bars and the three gaps between them.
const METER_WIDTH: f32 = 4.0 * METER_BAR + 3.0 * METER_GAP;
/// The distance between the meter and the word beside it.
const METER_PAD: f32 = 7.0;

/// Four little bars that bounce while the stream plays, the way the level meter
/// on a tuner does. `left` is the middle of the meter's left edge.
fn live_meter(painter: &egui::Painter, left: egui::Pos2, color: Color32, time: f64) {
    /// The shortest and tallest a bar gets.
    const SHORT: f32 = 4.0;
    const TALL: f32 = 14.0;
    /// Rate and starting phase per bar. The rates are deliberately not multiples
    /// of one another, so the four never fall into a visible lockstep.
    const MOTION: [(f64, f64); 4] = [(3.7, 0.0), (5.1, 1.7), (2.9, 3.1), (4.3, 0.8)];

    for (i, (rate, phase)) in MOTION.iter().enumerate() {
        let swing = 0.5 + 0.5 * (time * rate + phase).sin() as f32;
        let height = SHORT + (TALL - SHORT) * swing;
        let bar = Rect::from_min_size(
            pos2(
                left.x + i as f32 * (METER_BAR + METER_GAP),
                left.y - height * 0.5,
            ),
            vec2(METER_BAR, height),
        );
        painter.rect_filled(bar, CornerRadius::same(1), color);
    }
}

/// The address bar, styled like a search field.
fn draw_address_bar(app: &mut App, ui: &mut egui::Ui, action: &mut Option<Action>) {
    egui::Frame::NONE
        .fill(theme::p().surface)
        .corner_radius(theme::ROUND)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🔍").size(13.0).color(theme::p().muted));

                // Right to left: the button keeps its place and the field spreads
                // over the remaining space.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // The same field does both jobs, so the button says which one
                    // the text in it will do.
                    let typed = app.url_input.trim().to_owned();
                    let searching = !typed.is_empty() && !search::looks_like_address(&typed);
                    let button = ui.add_enabled(
                        !typed.is_empty(),
                        egui::Button::new(
                            RichText::new(if searching { "Search" } else { "Connect" })
                                .color(theme::p().on_accent),
                        )
                        .fill(theme::p().accent),
                    );

                    let field = ui.add(
                        egui::TextEdit::singleline(&mut app.url_input)
                            .hint_text("https://… stream address, or a name to search")
                            .frame(egui::Frame::NONE)
                            .desired_width(ui.available_width()),
                    );

                    let entered =
                        field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if button.clicked() || entered {
                        *action = Some(Action::Submit(app.url_input.clone()));
                    }
                });
            });
        });
}

/// The window with the search results: one tick box per station, and a button
/// that adds all the ticked ones to the main list.
fn draw_search_window(app: &mut App, ctx: &egui::Context, action: &mut Option<Action>) {
    let list_full = app.config.is_full();
    let Some(search) = &mut app.search else {
        return;
    };
    search.poll();

    let mut open = true;
    let title = format!("Search: {}", search.query);
    let mut picked: Option<Vec<(String, String)>> = None;

    egui::Window::new(title)
        .id(egui::Id::new("search-window"))
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(420.0, 460.0))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .frame(
            egui::Frame::NONE
                .fill(theme::p().backdrop)
                .stroke(Stroke::new(1.0, theme::p().card_outline))
                .corner_radius(theme::ROUND)
                .inner_margin(egui::Margin::same(12)),
        )
        .open(&mut open)
        .show(ctx, |ui| match &mut search.state {
            search::State::Searching => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("Searching…")
                            .size(12.5)
                            .color(theme::p().muted),
                    );
                });
                // No repaint is scheduled by itself while we wait; the worker asks
                // for one when it is done, but the spinner needs to keep turning.
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            search::State::Failed(error) => {
                ui.colored_label(theme::p().danger, format!("The search failed: {error}"));
            }
            search::State::Ready(found) if found.is_empty() => {
                ui.label(
                    RichText::new("No station found.")
                        .size(12.5)
                        .color(theme::p().muted),
                );
            }
            search::State::Ready(found) => {
                let selected = found.iter().filter(|f| f.checked).count();

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(match found.len() {
                            1 => "1 station found".to_owned(),
                            n => format!("{n} stations found"),
                        })
                        .size(12.0)
                        .color(theme::p().muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if list_button(ui, "None", selected > 0).clicked() {
                            found.iter_mut().for_each(|f| f.checked = false);
                        }
                        if list_button(ui, "All", selected < found.len()).clicked() {
                            found.iter_mut().for_each(|f| f.checked = true);
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // The buttons at the bottom keep their place; the list takes the rest.
                let reserved = 46.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - reserved).max(80.0))
                    .show(ui, |ui| {
                        for station in found.iter_mut() {
                            draw_found_row(ui, station);
                        }
                    });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if list_full {
                        ui.label(
                            RichText::new("The list is full")
                                .size(11.5)
                                .color(theme::p().danger),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = match selected {
                            0 => "Add".to_owned(),
                            n => format!("Add {n}"),
                        };
                        if list_button(ui, &label, selected > 0 && !list_full).clicked() {
                            picked = Some(
                                found
                                    .iter()
                                    .filter(|f| f.checked)
                                    .map(|f| (f.url.clone(), f.name.clone()))
                                    .collect(),
                            );
                        }
                        if list_button(ui, "Close", true).clicked() {
                            *action = Some(Action::CloseSearch);
                        }
                    });
                });
            }
        });

    if let Some(picked) = picked {
        *action = Some(Action::AddFound(picked));
    } else if !open {
        *action = Some(Action::CloseSearch);
    }
}

/// One result: the tick box, the name and the details of the station underneath.
fn draw_found_row(ui: &mut egui::Ui, station: &mut search::Found) {
    let row = egui::Frame::NONE
        .fill(if station.checked {
            theme::p().surface_hover
        } else {
            theme::p().surface
        })
        .corner_radius(theme::ROUND)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let box_response = ui.checkbox(&mut station.checked, "");
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&station.name)
                                .size(13.5)
                                .color(theme::p().text),
                        )
                        .truncate(),
                    );
                    if !station.meta.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&station.meta)
                                    .size(11.0)
                                    .color(theme::p().muted),
                            )
                            .truncate(),
                        );
                    }
                });
                box_response
            })
            .inner
        });

    // Anywhere on the row toggles the tick, not just the little box — but a click
    // that the box itself took must not be counted a second time.
    let response = ui.interact(
        row.response.rect,
        ui.id().with(("found", &station.url)),
        Sense::click(),
    );
    if response.clicked() && !row.inner.clicked() {
        station.checked = !station.checked;
    }
    response.on_hover_text(&station.url);
    ui.add_space(4.0);
}

/// The counter, the separator and the list of recent stations.
fn draw_stations(
    app: &mut App,
    ui: &mut egui::Ui,
    now: &NowPlaying,
    status: &Status,
    action: &mut Option<Action>,
) {
    if let Some(error) = &app.audio_error {
        ui.colored_label(ui.visuals().error_fg_color, error);
        return;
    }

    let count = app.config.recent.len();
    draw_list_header(app, ui, action);
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);

    if count == 0 {
        ui.add_space(8.0);
        ui.label(
            RichText::new("Enter a stream address and press Connect.")
                .size(12.5)
                .color(theme::p().muted),
        );
        return;
    }

    // The scroll bar handle takes its colour from the `fg_stroke` of the shared
    // widget visuals (`scroll.foreground_color`), so we paint it yellow inside a
    // scope — outside of here the visuals stay as they were.
    ui.scope(|ui| {
        let widgets = &mut ui.visuals_mut().widgets;
        widgets.inactive.fg_stroke.color = theme::p().title;
        widgets.hovered.fg_stroke.color = theme::p().title.lerp_to_gamma(theme::p().text, 0.35);
        widgets.active.fg_stroke.color = theme::p().title.lerp_to_gamma(theme::p().text, 0.55);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for station in &app.config.recent {
                    let active = station.url == app.playing_url;
                    // Only the station that is playing has a track and a bitrate to
                    // show. Otherwise we show the URL — unless it is already the
                    // title of the row.
                    let live = if active && status.is_active() {
                        track_details(now)
                    } else {
                        String::new()
                    };
                    let meta = match () {
                        _ if !live.is_empty() => live,
                        _ if station.name.is_some() => url_meta(&station.url),
                        _ => String::new(),
                    };

                    let row = draw_station_row(ui, station.title(), &meta, active, status);
                    if row.play {
                        // The station that is on the air stops on a click; any
                        // other one starts.
                        *action = Some(if active && status.is_active() {
                            Action::Stop
                        } else {
                            Action::Play(station.url.clone())
                        });
                    }
                    if row.forget {
                        *action = Some(Action::Forget(station.url.clone()));
                    }
                    ui.add_space(6.0);
                }
            });
    });
}

/// The station counter, and on the right the export and import of the list.
fn draw_list_header(app: &mut App, ui: &mut egui::Ui, action: &mut Option<Action>) {
    let count = app.config.recent.len();

    // While it is fresh, the message takes the counter's place; then it fades out
    // on its own.
    let note = app.note.as_ref().and_then(|(text, at)| {
        let left = NOTE_TTL.checked_sub(at.elapsed())?;
        ui.ctx().request_repaint_after(left);
        Some(text.clone())
    });
    let (text, color) = match note {
        Some(note) => (note, theme::p().title),
        None => (
            match count {
                0 => "no stations yet".to_owned(),
                1 => "1 station".to_owned(),
                n => format!("{n} stations"),
            },
            theme::p().muted,
        ),
    };

    ui.horizontal(|ui| {
        ui.label(RichText::new(text).size(12.0).color(color));

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if list_button(ui, "Import", true)
                .on_hover_text("Read a list from a text file")
                .clicked()
            {
                *action = Some(Action::Import);
            }
            let mut export = list_button(ui, "Export", count > 0);
            if count > 0 {
                export = export.on_hover_text("Write the list to a text file");
            }
            if export.clicked() {
                *action = Some(Action::Export);
            }
        });
    });
}

/// A secondary button of the list. On hover the tile lightens and opens up a
/// touch, the outline turns purple and the text goes white — just like the title
/// bar buttons. The size it reserves does not change: the "opening" is only
/// painting, otherwise the row would shift.
fn list_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let label_font = font(12.0);
    let text_size = ui
        .painter()
        .layout_no_wrap(label.to_owned(), label_font.clone(), theme::p().text)
        .size();

    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(text_size + vec2(20.0, 9.0), sense);
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id, enabled && response.hovered(), 0.12);

    let painter = ui.painter();
    painter.rect(
        rect.expand(egui::lerp(0.0..=1.5, t)),
        CornerRadius::same(8),
        theme::p()
            .surface
            .lerp_to_gamma(theme::p().surface_hover, t),
        Stroke::new(1.0, theme::p().outline.lerp_to_gamma(theme::p().accent, t)),
        StrokeKind::Inside,
    );
    theme::one_line(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        label_font,
        if enabled {
            theme::p().muted.lerp_to_gamma(theme::p().text, t)
        } else {
            theme::p().outline
        },
        rect.width(),
    );

    if !enabled {
        return response;
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

struct RowClicks {
    play: bool,
    forget: bool,
}

/// One station row: round button, name, and its details underneath.
/// Painted by hand so that long names get cut instead of wrapping.
fn draw_station_row(
    ui: &mut egui::Ui,
    title: &str,
    meta: &str,
    active: bool,
    status: &Status,
) -> RowClicks {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return RowClicks {
            play: false,
            forget: false,
        };
    }

    // We declare the ✖ before painting, but without reserving space in the
    // layout: a reservation here would move the rows, the mouse would leave the
    // row, the ✖ would disappear and the layout would oscillate.
    let cy = rect.center().y;
    let forget_rect = Rect::from_center_size(pos2(rect.right() - 20.0, cy), vec2(26.0, 26.0));
    let forget_response = ui.interact(forget_rect, response.id.with("forget"), Sense::click());

    // While the cursor is over the ✖ itself, the row does not count as hovered —
    // hence we join the two, otherwise the ✖ would vanish the moment you went to
    // click it.
    let hovered = response.hovered() || forget_response.hovered();
    let painter = ui.painter().clone();
    painter.rect(
        rect,
        theme::ROUND,
        if hovered {
            theme::p().surface_hover
        } else {
            theme::p().surface
        },
        Stroke::new(
            1.0,
            if active {
                theme::p().card_outline
            } else {
                theme::p().outline
            },
        ),
        StrokeKind::Inside,
    );

    let knob = pos2(rect.left() + 12.0 + 16.0, cy);
    painter.circle_filled(
        knob,
        16.0,
        if hovered {
            theme::p().accent_hover
        } else {
            theme::p().accent
        },
    );
    // The glyph shows what a click on this row will do.
    let glyph = if active && status.is_active() {
        "⏹"
    } else {
        "▶"
    };
    theme::one_line(
        &painter,
        knob,
        Align2::CENTER_CENTER,
        glyph,
        font(13.0),
        theme::p().on_accent,
        30.0,
    );

    // The ✖ only shows on hover, as in the list we modelled this on.
    let mut forget = false;
    if hovered {
        let t =
            ui.ctx()
                .animate_bool_with_time(forget_response.id, forget_response.hovered(), 0.12);
        if t > 0.0 {
            painter.circle_filled(
                forget_rect.center(),
                egui::lerp(9.0..=13.0, t),
                theme::p().danger.gamma_multiply(t * 0.85),
            );
        }
        theme::one_line(
            &painter,
            forget_rect.center(),
            Align2::CENTER_CENTER,
            "✖",
            font(12.0),
            theme::p().muted.lerp_to_gamma(theme::p().text, t),
            forget_rect.width(),
        );
        forget = forget_response.clicked();
    }

    // The space for the ✖ stays reserved at all times, so the text does not jump
    // as the mouse enters and leaves the row.
    let text_left = knob.x + 16.0 + 12.0;
    let width = (forget_rect.left() - 8.0 - text_left).max(1.0);

    let title_font = font(14.5);
    let meta_font = font(12.0);
    let title_h = ui.fonts_mut(|f| f.row_height(&title_font));
    let meta_h = ui.fonts_mut(|f| f.row_height(&meta_font));

    // Without a second line the title sits in the centre, otherwise the two share
    // the height.
    let (title_rect, meta_rect) = if meta.is_empty() {
        (
            Rect::from_min_size(pos2(text_left, cy - title_h * 0.5), vec2(width, title_h)),
            None,
        )
    } else {
        let top = cy - (title_h + 2.0 + meta_h) * 0.5;
        (
            Rect::from_min_size(pos2(text_left, top), vec2(width, title_h)),
            Some(Rect::from_min_size(
                pos2(text_left, top + title_h + 2.0),
                vec2(width, meta_h),
            )),
        )
    };

    // The station that is playing scrolls its own metadata; the rest are simply cut.
    let title_color = if active {
        theme::p().text
    } else {
        theme::p().text.gamma_multiply(0.92)
    };
    if active {
        theme::marquee(ui, title_rect, title, title_font, title_color);
    } else {
        theme::one_line(
            &painter,
            title_rect.left_center(),
            Align2::LEFT_CENTER,
            title,
            title_font,
            title_color,
            width,
        );
    }

    if let Some(meta_rect) = meta_rect {
        if active {
            theme::marquee(ui, meta_rect, meta, meta_font, theme::p().title);
        } else {
            theme::one_line(
                &painter,
                meta_rect.left_center(),
                Align2::LEFT_CENTER,
                meta,
                meta_font,
                theme::p().muted,
                width,
            );
        }
    }

    RowClicks {
        play: response.clicked() && !forget,
        forget,
    }
}

fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// A line of text that takes whatever width is left and scrolls if it does not fit.
fn line(ui: &mut egui::Ui, text: &str, font: FontId, color: Color32) {
    let height = ui.fonts_mut(|f| f.row_height(&font));
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    theme::marquee(ui, rect, text, font, color);
}

fn apply(app: &mut App, action: Action, ctx: &egui::Context) {
    // Asking for a station by hand is a fresh start: whatever the last one was
    // doing, the widening reconnect wait does not carry over to this one.
    if matches!(action, Action::Play(_) | Action::Submit(_)) {
        app.reconnect_tries = 0;
    }
    match action {
        Action::Play(url) => app.start(url),
        Action::Submit(text) => app.submit(text, ctx),
        Action::AddFound(stations) => {
            app.add_found(&stations);
            app.search = None;
        }
        Action::CloseSearch => app.search = None,
        Action::Stop => app.stop(),
        Action::Forget(url) => {
            app.config.forget(&url);
            app.config.save();
        }
        Action::Theme(i) => {
            let name = theme::PRESETS[i].0;
            theme::set(name);
            // egui keeps the style it was handed, and DWM the border it was
            // told about: both have to be told again.
            theme::restyle(ctx);
            app.repaint_border = true;
            app.config.theme = name.to_owned();
            app.config.save();
        }
        Action::Export => export_list(app),
        Action::Import => import_list(app),
    }
}

/// The file dialog holds the thread while it is open; the audio plays on another
/// thread, so the stream does not break while the user is choosing.
fn export_list(app: &mut App) {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Save list")
        .set_file_name("eFM-stations.txt")
        .add_filter("Text file", &["txt"])
        .save_file()
    else {
        return;
    };

    match std::fs::write(&path, app.config.stations_text()) {
        Ok(()) => {
            let count = app.config.recent.len();
            app.say(match count {
                1 => "Saved 1 station".to_owned(),
                n => format!("Saved {n} stations"),
            });
        }
        Err(e) => app.say(format!("Could not write the file: {e}")),
    }
}

fn import_list(app: &mut App) {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Open list")
        .add_filter("Text file", &["txt"])
        .pick_file()
    else {
        return;
    };

    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let added = app.config.import_text(&text);
            if added > 0 {
                app.config.save();
            }
            app.say(match added {
                0 => "No new stations".to_owned(),
                1 => "Added 1 station".to_owned(),
                n => format!("Added {n} stations"),
            });
        }
        Err(e) => app.say(format!("Could not read the file: {e}")),
    }
}

/// The card title: the track that is playing, otherwise the status.
fn headline(app: &App, now: &NowPlaying, status: &Status) -> (String, Color32) {
    if let Some(error) = &app.audio_error {
        return (error.clone(), theme::p().text);
    }
    // A stalled stream is still `Playing` as far as the player is concerned, so
    // without this the card would sit showing the last track it heard — fine for
    // the first three seconds, but the wait grows to a minute and the app would
    // just look frozen.
    if app.reconnect_at.is_some() {
        return ("Reconnecting…".to_owned(), theme::p().muted);
    }
    match status {
        Status::Error(message) => (message.clone(), theme::p().error),
        Status::Idle => ("No station".to_owned(), theme::p().text),
        Status::Connecting => ("Connecting to the station…".to_owned(), theme::p().text),
        _ => {
            let track = now
                .track
                .clone()
                .or_else(|| now.raw_title.clone())
                .or_else(|| now.station.clone());
            match track {
                // Yellow only when it really is station metadata, not a status.
                Some(track) => (track, theme::p().title),
                None => ("Playing".to_owned(), theme::p().text),
            }
        }
    }
}

/// The second line of the card, with its colour: yellow for metadata, dimmed for
/// the prompt we show while nothing is playing.
fn detail_line(now: &NowPlaying, status: &Status) -> (String, Color32) {
    if *status == Status::Idle {
        return (
            "Enter a stream address and press Play.".to_owned(),
            theme::p().muted,
        );
    }
    (track_details(now), theme::p().title)
}

/// Artist, album, genre, bitrate. The station name is missing on purpose: it is
/// already the title above, and in the list it is the title of the row.
fn track_details(now: &NowPlaying) -> String {
    let mut parts = Vec::new();
    if let Some(artist) = &now.artist {
        parts.push(artist.clone());
    }
    if let Some(album) = &now.album {
        parts.push(album.clone());
    }
    if let Some(genre) = &now.genre {
        parts.push(genre.clone());
    }
    if let Some(bitrate) = now.bitrate {
        parts.push(format!("{bitrate} kbps"));
    }
    parts.join("  •  ")
}

/// All we can say about a station that is not playing: its URL.
fn url_meta(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{detail_line, track_details, url_meta};
    use crate::player::{NowPlaying, Status};
    use crate::theme;

    #[test]
    fn strips_the_scheme_for_the_row_subtitle() {
        assert_eq!(url_meta("https://example.com/live"), "example.com/live");
    }

    #[test]
    fn joins_only_the_details_the_station_sent() {
        let now = NowPlaying {
            artist: Some("The Doors".into()),
            bitrate: Some(128),
            ..Default::default()
        };
        assert_eq!(track_details(&now), "The Doors  •  128 kbps");
    }

    #[test]
    fn says_nothing_when_the_station_sent_nothing() {
        assert!(track_details(&NowPlaying::default()).is_empty());
    }

    #[test]
    fn paints_metadata_yellow_but_not_the_idle_prompt() {
        let now = NowPlaying {
            artist: Some("The Doors".into()),
            ..Default::default()
        };
        assert_eq!(detail_line(&now, &Status::Playing).1, theme::p().title);
        assert_eq!(detail_line(&now, &Status::Idle).1, theme::p().muted);
    }
}
