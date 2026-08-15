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
    Resume,
    Pause,
    Forget(String),
    Export,
    Import,
    /// The stations that were ticked in the search window, as (url, name).
    AddFound(Vec<(String, String)>),
    CloseSearch,
}

/// How long the export/import message stays on screen.
const NOTE_TTL: std::time::Duration = std::time::Duration::from_secs(4);

pub fn draw(app: &mut App, root: &mut egui::Ui) {
    let snapshot = app.player.as_ref().map(|p| p.state());
    let status = snapshot.as_ref().map_or(Status::Idle, |s| s.status.clone());
    let now = snapshot.map(|s| s.now).unwrap_or_default();
    let mut action = None;

    app.note_station_name(&now);

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme::BACKDROP))
        .show(root, |ui| {
            draw_titlebar(ui);

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
                    ui.add_space(10.0);
                    draw_address_bar(app, ui, &mut action);
                    ui.add_space(10.0);
                    draw_stations(app, ui, &now, &status, &mut action);
                });
        });

    draw_search_window(app, root.ctx(), &mut action);

    if let Some(action) = action {
        apply(app, action, root.ctx());
    }
}

/// The title bar. Dragging moves the window, a double click maximises it.
fn draw_titlebar(ui: &mut egui::Ui) {
    let (rect, drag) = ui.allocate_exact_size(
        vec2(ui.available_width(), TITLEBAR_H),
        Sense::click_and_drag(),
    );

    let painter = ui.painter().clone();
    let cy = rect.center().y;
    let logo = pos2(rect.left() + PAGE_PAD + 9.0, cy);
    painter.circle_filled(logo, 9.0, theme::ACCENT);
    theme::one_line(
        &painter,
        logo,
        Align2::CENTER_CENTER,
        "♪",
        font(11.0),
        theme::TEXT,
        20.0,
    );
    // The version comes from Cargo.toml at compile time, so bumping the release
    // is enough — there is no second place to keep in sync.
    theme::one_line(
        &painter,
        pos2(logo.x + 17.0, cy),
        Align2::LEFT_CENTER,
        concat!("eFM v", env!("CARGO_PKG_VERSION")),
        FontId::new(17.0, FontFamily::Proportional),
        theme::TITLE,
        200.0,
    );

    // "✖" and not "✕": the latter is missing from the egui font and comes out as a box.
    let close = titlebar_button(
        ui,
        pos2(rect.right() - PAGE_PAD - 16.0, cy),
        "✖",
        theme::DANGER,
    );
    let minimize = titlebar_button(
        ui,
        pos2(rect.right() - PAGE_PAD - 52.0, cy),
        "—",
        theme::SURFACE_HOVER,
    );

    if close.clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
    if minimize.clicked() {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }

    // Dragging moves the window — unless it started on a button.
    let on_button = close.hovered() || minimize.hovered();
    if drag.drag_started_by(egui::PointerButton::Primary) && !on_button {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag.double_clicked_by(egui::PointerButton::Primary) && !on_button {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
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
        theme::MUTED.lerp_to_gamma(theme::TEXT, t),
        rect.width(),
    );

    response
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
        .fill(theme::CARD)
        .stroke(Stroke::new(1.0, theme::CARD_OUTLINE))
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
    let playing = *status == Status::Playing;

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
            let can_press = has_audio && (playing || *status == Status::Paused || has_url);
            if play_button(ui, status, can_press).clicked() {
                *action = Some(if playing {
                    Action::Pause
                } else if *status == Status::Paused {
                    Action::Resume
                } else {
                    Action::Submit(app.pending_url().to_owned())
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
        theme::OUTLINE
    } else {
        theme::MUTED.lerp_to_gamma(theme::TEXT, t)
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
            Stroke::new(2.0, theme::DANGER),
        );
    }

    if !enabled {
        return response;
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if silent { "Unmute" } else { "Mute" })
}

/// The playback button. While the stream plays it shows "Live" in green; as soon
/// as the mouse goes over it, it turns red and says what the click will do, i.e.
/// pause. Fixed width, so the row does not shift as the label changes.
fn play_button(ui: &mut egui::Ui, status: &Status, enabled: bool) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(104.0, 30.0), sense);

    let hot = enabled && response.hovered();
    let t = ui.ctx().animate_bool_with_time(response.id, hot, 0.12);
    let playing = *status == Status::Playing;

    let (fill, text, label) = if !enabled {
        (theme::SURFACE, theme::MUTED, "▶  Play")
    } else if playing {
        (
            theme::LIVE.lerp_to_gamma(theme::DANGER, t),
            // Dark letters on the light green, white ones on the red.
            theme::BACKDROP.lerp_to_gamma(theme::TEXT, t),
            if hot { "⏸  Pause" } else { "Live" },
        )
    } else {
        (
            theme::ACCENT.lerp_to_gamma(theme::ACCENT_HOVER, t),
            theme::TEXT,
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

/// The address bar, styled like a search field.
fn draw_address_bar(app: &mut App, ui: &mut egui::Ui, action: &mut Option<Action>) {
    egui::Frame::NONE
        .fill(theme::SURFACE)
        .corner_radius(theme::ROUND)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🔍").size(13.0).color(theme::MUTED));

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
                                .color(theme::TEXT),
                        )
                        .fill(theme::ACCENT),
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
                .fill(theme::BACKDROP)
                .stroke(Stroke::new(1.0, theme::CARD_OUTLINE))
                .corner_radius(theme::ROUND)
                .inner_margin(egui::Margin::same(12)),
        )
        .open(&mut open)
        .show(ctx, |ui| match &mut search.state {
            search::State::Searching => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Searching…").size(12.5).color(theme::MUTED));
                });
                // No repaint is scheduled by itself while we wait; the worker asks
                // for one when it is done, but the spinner needs to keep turning.
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            search::State::Failed(error) => {
                ui.colored_label(theme::DANGER, format!("The search failed: {error}"));
            }
            search::State::Ready(found) if found.is_empty() => {
                ui.label(
                    RichText::new("No station found.")
                        .size(12.5)
                        .color(theme::MUTED),
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
                        .color(theme::MUTED),
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
                                .color(theme::DANGER),
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
            theme::SURFACE_HOVER
        } else {
            theme::SURFACE
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
                            RichText::new(&station.name).size(13.5).color(theme::TEXT),
                        )
                        .truncate(),
                    );
                    if !station.meta.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&station.meta).size(11.0).color(theme::MUTED),
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
                .color(theme::MUTED),
        );
        return;
    }

    // The scroll bar handle takes its colour from the `fg_stroke` of the shared
    // widget visuals (`scroll.foreground_color`), so we paint it yellow inside a
    // scope — outside of here the visuals stay as they were.
    ui.scope(|ui| {
        let widgets = &mut ui.visuals_mut().widgets;
        widgets.inactive.fg_stroke.color = theme::TITLE;
        widgets.hovered.fg_stroke.color = theme::TITLE.lerp_to_gamma(theme::TEXT, 0.35);
        widgets.active.fg_stroke.color = theme::TITLE.lerp_to_gamma(theme::TEXT, 0.55);

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
                        *action = Some(Action::Play(station.url.clone()));
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
        Some(note) => (note, theme::TITLE),
        None => (
            match count {
                0 => "no stations yet".to_owned(),
                1 => "1 station".to_owned(),
                n => format!("{n} stations"),
            },
            theme::MUTED,
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
        .layout_no_wrap(label.to_owned(), label_font.clone(), theme::TEXT)
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
        theme::SURFACE.lerp_to_gamma(theme::SURFACE_HOVER, t),
        Stroke::new(1.0, theme::OUTLINE.lerp_to_gamma(theme::ACCENT, t)),
        StrokeKind::Inside,
    );
    theme::one_line(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        label_font,
        if enabled {
            theme::MUTED.lerp_to_gamma(theme::TEXT, t)
        } else {
            theme::OUTLINE
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
            theme::SURFACE_HOVER
        } else {
            theme::SURFACE
        },
        Stroke::new(
            1.0,
            if active {
                theme::CARD_OUTLINE
            } else {
                theme::OUTLINE
            },
        ),
        StrokeKind::Inside,
    );

    let knob = pos2(rect.left() + 12.0 + 16.0, cy);
    painter.circle_filled(
        knob,
        16.0,
        if hovered {
            theme::ACCENT_HOVER
        } else {
            theme::ACCENT
        },
    );
    // The glyph shows what a click on this row will do.
    let glyph = if active && *status == Status::Playing {
        "⏸"
    } else {
        "▶"
    };
    theme::one_line(
        &painter,
        knob,
        Align2::CENTER_CENTER,
        glyph,
        font(13.0),
        theme::TEXT,
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
                theme::DANGER.gamma_multiply(t * 0.85),
            );
        }
        theme::one_line(
            &painter,
            forget_rect.center(),
            Align2::CENTER_CENTER,
            "✖",
            font(12.0),
            theme::MUTED.lerp_to_gamma(theme::TEXT, t),
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
        theme::TEXT
    } else {
        theme::TEXT.gamma_multiply(0.92)
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
            theme::marquee(ui, meta_rect, meta, meta_font, theme::TITLE);
        } else {
            theme::one_line(
                &painter,
                meta_rect.left_center(),
                Align2::LEFT_CENTER,
                meta,
                meta_font,
                theme::MUTED,
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
    match action {
        Action::Play(url) => app.start(url),
        Action::Submit(text) => app.submit(text, ctx),
        Action::AddFound(stations) => {
            app.add_found(&stations);
            app.search = None;
        }
        Action::CloseSearch => app.search = None,
        Action::Resume => {
            // If the buffer drained while we were paused, a fresh connection is needed.
            let resumed = app.player.as_ref().is_some_and(|p| p.resume());
            if !resumed {
                let url = app.playing_url.clone();
                app.start(url);
            }
        }
        Action::Pause => {
            if let Some(player) = &app.player {
                player.pause();
            }
        }
        Action::Forget(url) => {
            app.config.forget(&url);
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
        return (error.clone(), theme::TEXT);
    }
    match status {
        Status::Error(message) => (message.clone(), Color32::from_rgb(0xF4, 0x7A, 0x7A)),
        Status::Idle => ("No station".to_owned(), theme::TEXT),
        Status::Connecting => ("Connecting to the station…".to_owned(), theme::TEXT),
        _ => {
            let track = now
                .track
                .clone()
                .or_else(|| now.raw_title.clone())
                .or_else(|| now.station.clone());
            match track {
                // Yellow only when it really is station metadata, not a status.
                Some(track) => (track, theme::TITLE),
                None if *status == Status::Paused => ("Paused".to_owned(), theme::TEXT),
                None => ("Playing".to_owned(), theme::TEXT),
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
            theme::MUTED,
        );
    }
    (track_details(now), theme::TITLE)
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
        assert_eq!(detail_line(&now, &Status::Playing).1, theme::TITLE);
        assert_eq!(detail_line(&now, &Status::Idle).1, theme::MUTED);
    }
}
