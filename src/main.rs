// On Windows the release build must not open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod player;
mod theme;
mod ui;

use config::Config;
use player::Player;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([480.0, 720.0])
            .with_min_inner_size([380.0, 420.0])
            // We paint the title bar ourselves, in the colours of the app.
            .with_decorations(false)
            .with_resizable(true)
            .with_icon(icon())
            .with_app_id("eFM"),
        ..Default::default()
    };

    eframe::run_native("eFM", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

/// The window icon. It is stored as raw RGBA pixels instead of a PNG, so that no
/// image decoder has to be linked into the binary.
fn icon() -> egui::IconData {
    const SIDE: u32 = 64;
    let rgba = include_bytes!("../assets/icon-64.rgba");
    debug_assert_eq!(rgba.len(), (SIDE * SIDE * 4) as usize);

    egui::IconData {
        rgba: rgba.to_vec(),
        width: SIDE,
        height: SIDE,
    }
}

struct App {
    config: Config,
    /// `None` when no audio device was found; the UI says so and plays nothing.
    player: Option<Player>,
    audio_error: Option<String>,
    /// The input field. It only serves to add a new station, so it stays empty:
    /// it never shows what is playing — the card and the list say that.
    url_input: String,
    /// The station that has been handed to the audio engine.
    playing_url: String,
    /// Deadline for the automatic reconnect after the stream drops.
    reconnect_at: Option<std::time::Instant>,
    /// Mute. It does not touch `config.volume`, so the volume comes back as it was.
    muted: bool,
    /// Short message about the list export/import, with the time it was written.
    note: Option<(String, std::time::Instant)>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let config = Config::load();

        let (player, audio_error) = match Player::new(cc.egui_ctx.clone()) {
            Ok(player) => {
                player.set_volume(config.volume);
                (Some(player), None)
            }
            Err(e) => (None, Some(format!("No audio device found: {e}"))),
        };

        let mut app = Self {
            url_input: String::new(),
            playing_url: String::new(),
            config,
            player,
            audio_error,
            reconnect_at: None,
            muted: false,
            note: None,
        };

        if app.config.autoplay && !app.config.last_url.is_empty() {
            app.start(app.config.last_url.clone());
        }
        app
    }

    /// Starts a station and records it in the settings.
    fn start(&mut self, url: String) {
        let url = url.trim().to_owned();
        if url.is_empty() {
            return;
        }
        let Some(player) = &self.player else { return };

        player.play_url(&url);
        self.playing_url = url.clone();
        // Whatever was typed has made it into the list by now, so the field is
        // cleared and ready for the next station.
        self.url_input.clear();
        self.reconnect_at = None;
        self.config.remember(&url);
        self.config.save();
    }

    /// What the card button will play: whatever was typed, otherwise the station
    /// heard last time — the field is only for new stations.
    fn pending_url(&self) -> &str {
        let typed = self.url_input.trim();
        if typed.is_empty() {
            &self.config.last_url
        } else {
            typed
        }
    }

    /// Shows a message on the counter line for a few seconds.
    fn say(&mut self, note: impl Into<String>) {
        self.note = Some((note.into(), std::time::Instant::now()));
    }

    /// Hands the audio engine the volume that should be heard right now.
    /// Muting only zeroes the output; the slider keeps its position.
    fn apply_volume(&self) {
        if let Some(player) = &self.player {
            player.set_volume(if self.muted { 0.0 } else { self.config.volume });
        }
    }

    /// Keeps the station name as soon as it arrives from the ICY metadata, so the
    /// list does not show a bare URL next time.
    fn note_station_name(&mut self, now: &player::NowPlaying) {
        if self.playing_url.is_empty() {
            return;
        }
        if let Some(name) = &now.station
            && self.config.name_station(&self.playing_url, name)
        {
            self.config.save();
        }
    }

    /// Live streams do drop; if the queue runs dry while we are supposedly
    /// playing, we reconnect after a short wait.
    fn watch_for_dropped_stream(&mut self, ctx: &egui::Context) {
        let Some(player) = &self.player else { return };

        match self.reconnect_at {
            // The queue can look empty for a single frame between two buffers.
            // If it filled up again in the meantime we cancel the reconnect:
            // otherwise the station restarted for no reason and the whole UI
            // flickered.
            Some(_) if !player.has_stalled() => self.reconnect_at = None,
            Some(at) if std::time::Instant::now() >= at => {
                let url = self.playing_url.clone();
                self.reconnect_at = None;
                self.start(url);
            }
            Some(_) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            None if player.has_stalled() && !self.playing_url.is_empty() => {
                self.reconnect_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
            None => {}
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.watch_for_dropped_stream(ui.ctx());
        ui::draw(self, ui);
    }

    // The glow backend hands over the OpenGL context here, for anyone who has GPU
    // resources to release. We just save the settings.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.config.save();
    }
}
