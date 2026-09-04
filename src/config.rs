//! Persistent storage of the settings (last station, volume, history).

use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

/// How many stations the list holds. It is no longer a mere history: the search
/// hands over whole batches at a time, so there is room for a real collection.
const MAX_RECENT: usize = 200;

/// One history entry: the URL and, if the station sent it, its name.
#[derive(Debug, Clone, Serialize)]
pub struct Station {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Station {
    /// What we show as the title: the station name, otherwise the URL without its scheme.
    pub fn title(&self) -> &str {
        match &self.name {
            Some(name) if !name.trim().is_empty() => name,
            _ => self
                .url
                .trim_start_matches("https://")
                .trim_start_matches("http://"),
        }
    }
}

// Old configs held bare URLs; we still accept them so no history is lost.
impl<'de> Deserialize<'de> for Station {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Url(String),
            Full {
                url: String,
                #[serde(default)]
                name: Option<String>,
            },
        }

        Ok(match Raw::deserialize(deserializer)? {
            Raw::Url(url) => Self { url, name: None },
            Raw::Full { url, name } => Self { url, name },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The stream that played last; it starts automatically on launch.
    pub last_url: String,
    /// The stations of the list, newest first.
    pub recent: Vec<Station>,
    pub volume: f32,
    /// Whether `last_url` starts on its own at launch.
    pub autoplay: bool,
    /// Where the window sat on the desktop when it was last closed, in the
    /// logical points the windowing system counts in. `None` until a first run
    /// has ended, and on the platforms that will not tell us (Wayland).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_pos: Option<[f32; 2]>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            last_url: String::new(),
            recent: Vec::new(),
            volume: 1.0,
            autoplay: true,
            window_pos: None,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        // Before the app was renamed the settings lived elsewhere; as long as the
        // new file does not exist yet, we read them from there. The first save
        // writes them to the new location.
        let paths = [config_path(), legacy_config_path()];
        let found = paths
            .into_iter()
            .flatten()
            .find_map(|path| std::fs::read_to_string(&path).ok().map(|raw| (path, raw)));
        // No file anywhere: a first run, and nothing worth saying about it.
        let Some((path, raw)) = found else {
            return Self::default();
        };

        match serde_json::from_str(&raw) {
            Ok(config) => config,
            Err(e) => {
                // Carrying on from the defaults would have the next save write over
                // whatever is in there, and a truncated file is often still most of
                // a station list. So it is set aside rather than destroyed.
                let kept = path.with_extension("json.bak");
                eprintln!(
                    "the settings could not be read ({e}); the file is kept as {}",
                    kept.display()
                );
                if let Err(e) = std::fs::rename(&path, &kept) {
                    eprintln!("could not set the unreadable settings aside: {e}");
                }
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            eprintln!("could not create the settings folder: {e}");
            return;
        }
        let json = match serde_json::to_string_pretty(self) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("could not serialise the settings: {e}");
                return;
            }
        };

        // Written beside the real file and then moved onto it. `save` runs on every
        // play, every nudge of the volume and every name the metadata teaches us, so
        // a process that dies mid-write is not a rare shot: writing in place would
        // leave half a file, and `load` would quietly start again from nothing. The
        // rename is atomic on both platforms — MoveFileEx on Windows, rename(2)
        // elsewhere — so the file on disk is only ever the old one or the new one.
        let tmp = path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, json) {
            eprintln!("could not write the settings: {e}");
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &path) {
            eprintln!("could not replace the settings: {e}");
            // A leftover temp file next to a good config is only confusing later.
            let _ = std::fs::remove_file(&tmp);
        }
    }

    /// Records `url` as the last one and moves it to the top of the list.
    /// A station we already know is moved — not added a second time.
    pub fn remember(&mut self, url: &str) {
        let url = url.trim();
        if url.is_empty() {
            return;
        }
        self.last_url = url.to_owned();
        // Keep the name we may already have known for this station.
        let name = self.station(url).and_then(|s| s.name.clone());
        self.recent.retain(|s| s.url != url);
        self.recent.insert(
            0,
            Station {
                url: url.to_owned(),
                name,
            },
        );
        self.recent.truncate(MAX_RECENT);
    }

    /// The list as plain text, one stream per line.
    pub fn stations_text(&self) -> String {
        let mut text = String::new();
        for station in &self.recent {
            text.push_str(&station.url);
            text.push('\n');
        }
        text
    }

    /// Reads such a list. Unknown stations are appended in file order; anything we
    /// already know is left untouched, along with its name.
    /// Returns how many were added.
    pub fn import_text(&mut self, text: &str) -> usize {
        let mut added = 0;
        for line in text.lines() {
            let url = line.trim();
            // "#" allows comments in a list written by hand.
            if url.starts_with('#') {
                continue;
            }
            if self.is_full() {
                break;
            }
            if self.add(url, None) {
                added += 1;
            }
        }
        added
    }

    /// Appends a station at the end of the list, keeping its name if we were given
    /// one. Returns `false` for a station we already know, or once the list is full.
    pub fn add(&mut self, url: &str, name: Option<&str>) -> bool {
        let url = url.trim();
        // The gate sits here rather than in `import_text`, so that every way into
        // the list is held to the same standard: a file picked off the disk is not
        // a more trustworthy source than the directory, which is already filtered.
        if !is_stream_url(url) || self.is_full() || self.station(url).is_some() {
            return false;
        }
        let name = name.map(str::trim).filter(|n| !n.is_empty());
        self.recent.push(Station {
            url: url.to_owned(),
            name: name.map(str::to_owned),
        });
        true
    }

    pub fn is_full(&self) -> bool {
        self.recent.len() >= MAX_RECENT
    }

    /// Names the station as soon as the ICY metadata sends it.
    /// Returns `true` only when something changed, so we do not save every frame.
    pub fn name_station(&mut self, url: &str, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(station) = self.recent.iter_mut().find(|s| s.url == url) else {
            return false;
        };
        if station.name.as_deref() == Some(name) {
            return false;
        }
        station.name = Some(name.to_owned());
        true
    }

    pub fn station(&self, url: &str) -> Option<&Station> {
        self.recent.iter().find(|s| s.url == url)
    }

    pub fn forget(&mut self, url: &str) {
        self.recent.retain(|s| s.url != url);
    }
}

/// What counts as a stream we are willing to keep: something we can actually
/// hand to the player. One rule, used both by the import and by the directory
/// results, so the two cannot come to disagree.
pub fn is_stream_url(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("http://") || url.starts_with("https://")
}

fn config_path() -> Option<PathBuf> {
    app_config_path("eFM")
}

/// Where the settings lived while the app was called "papari".
fn legacy_config_path() -> Option<PathBuf> {
    app_config_path("papari")
}

fn app_config_path(app: &str) -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", app).map(|dirs| dirs.config_dir().join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::{Config, is_stream_url};

    #[test]
    fn keeps_only_what_the_player_could_open() {
        let mut config = Config::default();
        // An imported file is not a more trustworthy source than the directory,
        // which is filtered by the very same rule.
        assert!(!config.add("not a url at all", None));
        assert!(!config.add("ftp://example.com/live", None));
        assert!(!config.add("www.example.com/live", None));
        assert!(config.add("https://example.com/live", None));
        assert_eq!(config.recent.len(), 1);
    }

    #[test]
    fn counts_only_the_lines_it_actually_took() {
        let mut config = Config::default();
        let added = config.import_text(
            "# a list written by hand
https://one.example/live
rubbish
http://two.example/live
",
        );
        assert_eq!(added, 2);
        assert_eq!(config.recent.len(), 2);
    }

    #[test]
    fn agrees_with_the_directory_on_what_a_stream_is() {
        assert!(is_stream_url("http://example.com/live"));
        assert!(is_stream_url("  https://example.com/live  "));
        assert!(!is_stream_url("example.com/live"));
        assert!(!is_stream_url(""));
    }

    #[test]
    fn adds_a_new_station_at_the_top() {
        let mut config = Config::default();
        config.remember("https://one.example/live");
        config.remember("https://two.example/live");

        let urls: Vec<&str> = config.recent.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(
            urls,
            ["https://two.example/live", "https://one.example/live"]
        );
        assert_eq!(config.last_url, "https://two.example/live");
    }

    #[test]
    fn moves_a_known_station_up_without_duplicating_it() {
        let mut config = Config::default();
        config.remember("https://one.example/live");
        config.remember("https://two.example/live");
        assert!(config.name_station("https://one.example/live", "One"));

        config.remember("https://one.example/live");

        let urls: Vec<&str> = config.recent.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(
            urls,
            ["https://one.example/live", "https://two.example/live"]
        );
        // The name travels with it.
        assert_eq!(
            config.station("https://one.example/live").unwrap().title(),
            "One"
        );
    }

    #[test]
    fn writes_one_stream_per_line() {
        let mut config = Config::default();
        config.remember("https://one.example/live");
        config.remember("https://two.example/live");
        assert_eq!(
            config.stations_text(),
            "https://two.example/live\nhttps://one.example/live\n"
        );
    }

    #[test]
    fn imports_only_the_unknown_streams() {
        let mut config = Config::default();
        config.remember("https://one.example/live");

        let added = config
            .import_text("# my list\nhttps://one.example/live\n\n  https://three.example/live  \n");

        assert_eq!(added, 1);
        let urls: Vec<&str> = config.recent.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(
            urls,
            ["https://one.example/live", "https://three.example/live"]
        );
    }

    #[test]
    fn survives_a_round_trip() {
        let mut config = Config::default();
        config.remember("https://one.example/live");
        config.remember("https://two.example/live");
        let text = config.stations_text();

        let mut restored = Config::default();
        assert_eq!(restored.import_text(&text), 2);
        assert_eq!(restored.stations_text(), text);
    }
}
