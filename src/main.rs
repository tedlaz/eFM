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
        // ponytail: no software-rendering knob here — HardwareAcceleration::Off was
        // tried and WGL has no non-accelerated pixel format that egui can use, so
        // glutin finds no config and the app does not start. The ~130MB the AMD
        // driver maps into this process is the price of having a window at all.
        ..Default::default()
    };

    eframe::run_native(
        "eFM",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, config)))),
    )
}

/// Asks the system to stay awake while a station plays, and lets go when it
/// stops. Without it the idle timer fires in the middle of a song and the radio
/// goes quiet — the machine has no way of knowing that the silence it is about to
/// impose is unwelcome.
///
/// Only the system is held, never the display: this is a radio, so the screen is
/// free to blank.
#[cfg(windows)]
fn keep_system_awake(awake: bool) {
    // One declaration is cheaper than a dependency on all of windows-sys for the
    // sake of a single symbol.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }

    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

    // The state belongs to the thread that sets it and lasts as long as that
    // thread does, which is why this is only ever called from the UI thread.
    let flags = if awake {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    unsafe { SetThreadExecutionState(flags) };
}

/// The two things that tell a Windows 11 window apart from a Windows 10 one:
/// rounded corners and a border in the colour of the app. The window is
/// undecorated — we paint the title bar ourselves — so Windows draws no frame of
/// its own, but DWM still owns the corners and the hairline around them, and
/// left alone it gives an app in `theme::p().backdrop` square corners and a pale
/// edge.
///
/// ponytail: no `windows` crate for two constants and one call, for the same
/// reason `keep_system_awake` declares its own. `raw-window-handle` is not a new
/// dependency either — eframe already builds it; we only name the version it
/// picked, so that we can read the handle it hands us.
#[cfg(windows)]
fn dress_for_windows_11(window: &impl raw_window_handle::HasWindowHandle) {
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attr: u32, value: *const u32, size: u32) -> i32;
    }

    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: u32 = 2;
    const DWMWA_BORDER_COLOR: u32 = 34;

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let raw_window_handle::RawWindowHandle::Win32(win32) = handle.as_ref() else {
        return;
    };
    let hwnd = win32.hwnd.get();

    // Both attributes are younger than the app's minimum Windows, so on 10 they
    // come back as an error — which is why the result is not worth looking at:
    // there is nothing to be done about a version that has no rounded corners to
    // give.
    for (attr, value) in [
        (DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND),
        (DWMWA_BORDER_COLOR, colorref(theme::p().backdrop)),
    ] {
        unsafe { DwmSetWindowAttribute(hwnd, attr, &raw const value, size_of::<u32>() as u32) };
    }
}

#[cfg(not(windows))]
fn dress_for_windows_11(_window: &impl raw_window_handle::HasWindowHandle) {}

/// A colour the way Windows spells one: COLORREF is 0x00BBGGRR, which is the
/// reverse of the order the theme — and everything else — writes it in. Getting
/// this backwards paints a border that looks plausible and is the wrong colour.
#[cfg(windows)]
fn colorref(color: egui::Color32) -> u32 {
    let [r, g, b, _] = color.to_array();
    u32::from(b) << 16 | u32::from(g) << 8 | u32::from(r)
}

// ponytail: Windows only, because that is where the sleeping-mid-song was seen.
// Linux wants a D-Bus call to org.freedesktop.ScreenSaver or a systemd inhibitor
// and macOS wants IOPMAssertionCreateWithName — a dependency each, for a symptom
// neither has been observed to have.
#[cfg(not(windows))]
fn keep_system_awake(_awake: bool) {}

/// How long to wait before the next reconnect: 3s, 6s, 12s, 24s, 48s, then a
/// minute for as long as it takes. It never gives up — a radio left playing
/// should come back on its own after an outage — but one request a minute is a
/// fair price to ask of a station that is gone, where the old fixed three
/// seconds meant some twelve hundred an hour.
fn backoff(tries: u32) -> std::time::Duration {
    const FIRST: u64 = 3;
    const CAP: u64 = 60;
    std::time::Duration::from_secs(FIRST.saturating_mul(1 << tries.min(5)).min(CAP))
}

/// Gives an address the scheme it is missing. `search::looks_like_address` sends
/// anything starting with "www." straight to the player, and the player parses it
/// as a URL — which, without a scheme, it is not. So the two agreed that "www."
/// was an address and then produced "Invalid URL" for every one of them.
fn normalise_url(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() || text.contains("://") {
        return text.to_owned();
    }
    format!("https://{text}")
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
    /// How many times we have reconnected without the user asking. It widens the
    /// wait, and only a deliberate Play or Stop puts it back to zero: a station
    /// that connects, plays a second and dies again is exactly the case the
    /// backoff is for, so recovering the stream must not by itself reset it.
    reconnect_tries: u32,
    /// Whether the system has been asked to stay awake for us, so that the ask is
    /// made when it changes rather than on every frame.
    keeps_awake: bool,
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
    /// The height the window needs with the drawer folded away — the card and
    /// the margin under it — measured as it is drawn.
    folded_height: f32,
    /// A height we have asked the window for, with the height it had when we
    /// asked, while we wait to see what it does about it.
    asked_height: Option<(f32, f32)>,
    /// A fold the user asked for, waiting for the blanked frame to be shown
    /// before the window is actually resized. See `ui::cover`.
    pending_fold: Option<f32>,
    /// How long the blank is allowed to last. A compositor that never answers
    /// the resize must not leave the window empty for good.
    cover_until: Option<std::time::Instant>,
    /// Whether the folded window has already been trimmed to the card. The
    /// window is born at a guessed height, and this is what stops us from asking
    /// about it over and over if it will not take it.
    folded_trimmed: bool,
    /// Set when the palette changes: the border DWM paints around the window is
    /// ours too, and it is only repainted when we ask.
    repaint_border: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        // The palette is chosen before the style is written, so the first frame is
        // already in the right colours.
        theme::set(&config.theme);
        theme::apply(&cc.egui_ctx);
        dress_for_windows_11(cc);

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
            reconnect_tries: 0,
            keeps_awake: false,
            muted: false,
            note: None,
            search: None,
            unfolded: false,
            unfolded_height: UNFOLDED_HEIGHT,
            folded_height: FOLDED_HEIGHT,
            asked_height: None,
            pending_fold: None,
            cover_until: None,
            folded_trimmed: false,
            repaint_border: false,
        };

        if app.config.autoplay && !app.config.last_url.is_empty() {
            app.start(app.config.last_url.clone());
        }
        app
    }

    /// Starts a station and records it in the settings.
    fn start(&mut self, url: String) {
        let url = normalise_url(url.trim());
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

    /// Stops the stream. Nothing is kept to resume from: the station is live, so
    /// the next Play reconnects and picks it up where it is by then.
    fn stop(&mut self) {
        if let Some(player) = &self.player {
            player.stop();
        }
        self.playing_url.clear();
        self.reconnect_at = None;
        self.reconnect_tries = 0;
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

    /// Holds the system awake for as long as a station is on. `is_active` covers
    /// connecting as well as playing, and a stalled stream is still playing — so
    /// the machine also stays up through a reconnect wait rather than dropping
    /// asleep inside one.
    fn watch_sleep(&mut self) {
        let wanted = self
            .player
            .as_ref()
            .is_some_and(|player| player.status().is_active());
        if wanted != self.keeps_awake {
            keep_system_awake(wanted);
            self.keeps_awake = wanted;
        }
    }

    /// Live streams do drop; if the stream is not running while we are supposedly
    /// playing, we reconnect after a short wait.
    fn watch_for_dropped_stream(&mut self, ctx: &egui::Context) {
        let Some(player) = &self.player else { return };

        let now = std::time::Instant::now();
        // A station is meant to be on for as long as we hold its address: only
        // Stop clears it, and only the user presses Stop.
        let wanted = !self.playing_url.is_empty();
        match recovery(is_broken(player), wanted, self.reconnect_at, now) {
            Recovery::Cancel => self.reconnect_at = None,
            Recovery::Reconnect => {
                let url = self.playing_url.clone();
                self.reconnect_tries = self.reconnect_tries.saturating_add(1);
                self.start(url);
            }
            // Nothing else asks for a frame while we wait, and the wait can now be
            // a minute long — so we sleep to the deadline instead of polling at it.
            Recovery::Wait(at) => ctx.request_repaint_after(at.saturating_duration_since(now)),
            Recovery::Schedule => {
                let wait = backoff(self.reconnect_tries);
                self.reconnect_at = Some(now + wait);
                ctx.request_repaint_after(wait);
            }
            Recovery::Nothing => {}
        }
    }
}

/// Whether the station we think is on is not actually coming through.
///
/// Two ways for that to be true, and for a long time only the first was counted:
/// the queue ran dry under a `Playing` status, or the attempt to get the stream
/// back failed and left an error behind. Leaving the second one out is what made
/// a station that dropped stay dropped — the reconnect set the status to
/// `Error`, `Error` is not `Playing`, so nothing ever asked for another go and
/// the app sat silent until it was restarted. One failed retry was enough.
fn is_broken(player: &Player) -> bool {
    player.has_stalled() || matches!(player.status(), player::Status::Error(_))
}

/// What the reconnect loop should do this frame.
#[derive(Debug, PartialEq, Eq)]
enum Recovery {
    Nothing,
    /// It came back on its own; drop the pending reconnect.
    Cancel,
    /// Start waiting.
    Schedule,
    /// Keep waiting, and ask to be woken at the deadline.
    Wait(std::time::Instant),
    /// The wait is over.
    Reconnect,
}

fn recovery(
    broken: bool,
    wanted: bool,
    reconnect_at: Option<std::time::Instant>,
    now: std::time::Instant,
) -> Recovery {
    match reconnect_at {
        // The queue can look empty for a single frame between two buffers. If it
        // filled up again in the meantime we cancel the reconnect: otherwise the
        // station restarted for no reason and the whole UI flickered.
        Some(_) if !broken => Recovery::Cancel,
        Some(at) if now >= at => Recovery::Reconnect,
        Some(at) => Recovery::Wait(at),
        None if broken && wanted => Recovery::Schedule,
        None => Recovery::Nothing,
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.watch_window_pos(ui.ctx());
        self.watch_for_dropped_stream(ui.ctx());
        self.watch_sleep();
        ui::draw(self, ui);
        // A palette was picked this frame. `Frame` is a window handle, which is
        // all the DWM call wants, so the border simply gets painted again.
        if std::mem::take(&mut self.repaint_border) {
            dress_for_windows_11(frame);
        }
    }

    // The glow backend hands over the OpenGL context here, for anyone who has GPU
    // resources to release. We just save the settings.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.config.save();
        // A power request left standing after the app is gone would keep the
        // machine up for nothing at all.
        keep_system_awake(false);
    }
}

#[cfg(test)]
mod tests {
    use super::{Recovery, backoff, normalise_url, recovery};
    use std::time::{Duration, Instant};

    /// The regression this whole thing is about: a station drops, the reconnect
    /// fails once and leaves an error behind, and the app has to keep trying.
    /// Before, `broken` was only ever true while the status still said `Playing`,
    /// so a failed retry ended the retrying — and the radio stayed silent until
    /// it was restarted by hand.
    #[test]
    fn keeps_trying_after_a_reconnect_that_failed() {
        let now = Instant::now();
        assert_eq!(recovery(true, true, None, now), Recovery::Schedule);
    }

    #[test]
    fn waits_until_the_deadline_then_reconnects() {
        let now = Instant::now();
        let at = now + Duration::from_secs(3);
        assert_eq!(recovery(true, true, Some(at), now), Recovery::Wait(at));
        assert_eq!(recovery(true, true, Some(at), at), Recovery::Reconnect);
    }

    #[test]
    fn drops_the_reconnect_when_the_stream_comes_back_by_itself() {
        let now = Instant::now();
        let at = now + Duration::from_secs(3);
        assert_eq!(recovery(false, true, Some(at), now), Recovery::Cancel);
    }

    /// Stop clears the address, and that is what has to end the retrying — a
    /// station the user switched off must not quietly come back on.
    #[test]
    fn leaves_a_stopped_station_alone_however_broken_it_looks() {
        let now = Instant::now();
        assert_eq!(recovery(true, false, None, now), Recovery::Nothing);
        assert_eq!(recovery(false, false, None, now), Recovery::Nothing);
    }

    #[cfg(windows)]
    #[test]
    fn spells_a_colour_the_way_windows_does() {
        // Not a palindrome, so a swap that did nothing would still pass.
        assert_eq!(
            super::colorref(egui::Color32::from_rgb(0x0F, 0x14, 0x24)),
            0x0024_140F
        );
    }

    #[test]
    fn widens_the_wait_then_holds_it_at_a_minute() {
        let seconds: Vec<u64> = (0..8).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(seconds, [3, 6, 12, 24, 48, 60, 60, 60]);
    }

    #[test]
    fn never_waits_longer_than_the_cap_however_many_tries() {
        // The shift is what would overflow if the tries were not clamped first.
        assert_eq!(backoff(u32::MAX).as_secs(), 60);
    }

    #[test]
    fn gives_a_bare_address_the_scheme_it_lacks() {
        assert_eq!(
            normalise_url("www.example.com/live"),
            "https://www.example.com/live"
        );
    }

    #[test]
    fn leaves_an_address_that_has_a_scheme_alone() {
        assert_eq!(
            normalise_url("http://example.com/live"),
            "http://example.com/live"
        );
        assert_eq!(normalise_url("  "), "");
    }
}
