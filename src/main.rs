// On Windows the release build must not open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod player;
mod search;
mod theme;
mod ui;

use config::Config;
use player::Player;
use search::Search;

/// The window height with the drawer open. It is only the starting point: as
/// soon as the user resizes the window, that is the height the drawer gives back.
const UNFOLDED_HEIGHT: f32 = 720.0;

/// What the window opens at, folded. The exact height depends on how tall the
/// card comes out, which we only know once it has been laid out — the first frame
/// measures it and corrects this guess.
pub const FOLDED_HEIGHT: f32 = 196.0;

fn main() -> eframe::Result {
    // The settings are read before the window exists, because the place it should
    // open at is one of them.
    let config = Config::load();

    let mut viewport = egui::ViewportBuilder::default()
        // Folded is how the app starts, so that is the size it is born at: opening
        // tall and snapping shut on the first frame would be a flinch.
        .with_inner_size([480.0, FOLDED_HEIGHT])
        // Low enough for the folded window; the drawer is what makes the window
        // tall, not the user.
        .with_min_inner_size([380.0, 170.0])
        // We paint the title bar ourselves, in the colours of the app.
        .with_decorations(false)
        .with_resizable(true)
        .with_icon(icon())
        .with_app_id("eFM");
    // Where it stood when it was last closed. Without one — a first run, or a
    // system that does not let us ask — the desktop places the window itself.
    if let Some([x, y]) = config.window_pos {
        viewport = viewport.with_position([x, y]);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "eFM",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, config)))),
    )
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
    /// The search window, while it is open.
    search: Option<Search>,
    /// Whether the lower half — the address bar and the station list — is out.
    /// Every launch starts folded: the card is what the app is for, the list is
    /// a drawer that is pulled open when it is wanted.
    unfolded: bool,
    /// The height to give the window back when it unfolds. It is read off the
    /// window at the moment it folds, so one the user has resized returns as it
    /// was rather than to some size of ours.
    unfolded_height: f32,
    /// Set while the window is on its way to the height the drawer just asked
    /// for. Until it arrives, nothing else may touch the height.
    resizing: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        theme::apply(&cc.egui_ctx);

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
            search: None,
            unfolded: false,
            unfolded_height: UNFOLDED_HEIGHT,
            resizing: false,
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

    /// What the user asked for with Connect (or Enter): a stream address is played
    /// straight away, anything else is taken as a search of the directory.
    fn submit(&mut self, text: String, ctx: &egui::Context) {
        if text.trim().is_empty() {
            return;
        }
        if search::looks_like_address(&text) {
            self.start(text);
            return;
        }
        self.search = Some(Search::start(&text, ctx));
        // The window shows what was asked for, so the field is free again.
        self.url_input.clear();
    }

    /// Puts the stations that were ticked in the search window into the list.
    fn add_found(&mut self, stations: &[(String, String)]) {
        let mut added = 0;
        for (url, name) in stations {
            if self.config.add(url, Some(name)) {
                added += 1;
            }
        }
        if added > 0 {
            self.config.save();
        }

        let missed = stations.len() - added;
        self.say(match (added, missed) {
            (0, _) => "No new stations".to_owned(),
            (1, 0) => "Added 1 station".to_owned(),
            (n, 0) => format!("Added {n} stations"),
            (n, missed) => format!("Added {n}, skipped {missed}"),
        });
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

    /// Keeps track of where the window sits, so the next launch opens in the same
    /// place. The windowing system reports it in the same points that
    /// `with_position` expects, so it goes into the settings as it comes.
    fn watch_window_pos(&mut self, ctx: &egui::Context) {
        let pos = ctx.input(|i| {
            let viewport = i.viewport();
            // Minimised or maximised is not where the user put the window: the
            // position to come back to is the one it had before.
            if viewport.minimized.unwrap_or(false) || viewport.maximized.unwrap_or(false) {
                return None;
            }
            // Rounded: the points make a round trip through the pixels of the
            // screen, and on a scaled display that costs a fraction of a point
            // each time. Whole numbers keep the window from creeping across the
            // desktop over many launches.
            viewport
                .outer_rect
                .map(|rect| [rect.min.x.round(), rect.min.y.round()])
        });
        if let Some(pos) = pos {
            // Only the exit writes the file; here we just keep it up to date.
            self.config.window_pos = Some(pos);
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
        self.watch_window_pos(ui.ctx());
        self.watch_for_dropped_stream(ui.ctx());
        ui::draw(self, ui);
    }

    // The glow backend hands over the OpenGL context here, for anyone who has GPU
    // resources to release. We just save the settings.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.config.save();
    }
}
