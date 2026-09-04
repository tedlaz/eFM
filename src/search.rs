//! Station search on the radio-browser directory.
//!
//! Whatever the user types that is not a stream address goes through here: the
//! request runs on a thread of its own and the answer lands in a shared slot, so
//! the interface never blocks while the network takes its time.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use stream_download::http::reqwest::{Client, Url};

/// The directory endpoint. It searches by name and gives back the most voted first.
/// Without a trailing slash: the query is pushed on as the next path segment.
const API: &str = "https://de1.api.radio-browser.info/json/stations/byname";
/// `hidebroken` leaves out the streams the directory has already found to be dead.
const QUERY: &str = "limit=100&order=votes&reverse=true&hidebroken=true";
const TIMEOUT: Duration = Duration::from_secs(10);
/// How many tags we show next to a station before it gets noisy.
const MAX_TAGS: usize = 3;

/// One station the directory sent back, along with its tick box.
#[derive(Debug, Clone)]
pub struct Found {
    pub name: String,
    pub url: String,
    /// Country, codec, bitrate and tags, joined for the second line of the row.
    pub meta: String,
    pub checked: bool,
}

/// What the worker leaves behind: the stations, or why it could not get them.
type Answer = Arc<Mutex<Option<Result<Vec<Found>, String>>>>;

/// Where the search has got to.
pub enum State {
    Searching,
    Ready(Vec<Found>),
    Failed(String),
}

/// The search window: the text that was asked for and its result.
pub struct Search {
    pub query: String,
    pub state: State,
    /// The worker thread leaves its answer here and asks for a repaint.
    slot: Answer,
}

impl Search {
    /// Fires the request off and returns straight away; the window opens showing
    /// "Searching…".
    pub fn start(query: &str, ctx: &egui::Context) -> Self {
        let query = query.trim().to_owned();
        let slot = Arc::new(Mutex::new(None));

        {
            let query = query.clone();
            let slot = Arc::clone(&slot);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                // A runtime of its own: the search must work even when there is no
                // audio device, i.e. when the player — and its runtime — is missing.
                let outcome = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(fetch(&query)).map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
                *slot.lock().expect("search mutex poisoned") = Some(outcome);
                ctx.request_repaint();
            });
        }

        Self {
            query,
            state: State::Searching,
            slot,
        }
    }

    /// Moves the answer of the worker into the window. Called once per frame.
    pub fn poll(&mut self) {
        let Some(outcome) = self.slot.lock().expect("search mutex poisoned").take() else {
            return;
        };
        self.state = match outcome {
            Ok(found) => State::Ready(found),
            Err(e) => State::Failed(e),
        };
    }
}

/// What we read out of the directory's JSON. Everything is optional: the fields
/// do come back as `null` on stations nobody has filled in properly.
#[derive(Debug, Deserialize)]
struct Raw {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    url: Option<String>,
    /// The address after the redirects — the one that actually plays.
    #[serde(default)]
    url_resolved: Option<String>,
    #[serde(default)]
    codec: Option<String>,
    #[serde(default)]
    bitrate: Option<u32>,
    #[serde(default)]
    countrycode: Option<String>,
    /// Comma separated, as the directory keeps them.
    #[serde(default)]
    tags: Option<String>,
}

async fn fetch(query: &str) -> anyhow::Result<Vec<Found>> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    // The directory asks every client to name itself.
    let client = Client::builder()
        .user_agent(concat!("eFM/", env!("CARGO_PKG_VERSION")))
        .timeout(TIMEOUT)
        .build()?;
    let body = client
        .get(endpoint(query)?)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let raw: Vec<Raw> = serde_json::from_str(&body)?;
    Ok(collect(raw))
}

/// The address of the search. `path_segments_mut` percent-encodes the text for
/// us, so anything the user typed — spaces, Greek, a slash — travels safely.
fn endpoint(query: &str) -> anyhow::Result<Url> {
    let mut url = Url::parse(API)?;
    url.path_segments_mut()
        .map_err(|()| anyhow::anyhow!("Invalid endpoint"))?
        // A path that ends in "/" has an empty last segment, and pushing onto it
        // would give the directory a "byname//jazz" it answers with a 404.
        .pop_if_empty()
        .push(query);
    url.set_query(Some(QUERY));
    Ok(url)
}

/// Keeps the entries that can actually be played and drops the duplicates —
/// the same stream is often listed several times under different names.
fn collect(raw: Vec<Raw>) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    for entry in raw {
        let Some(station) = convert(entry) else {
            continue;
        };
        if found.iter().any(|f| f.url == station.url) {
            continue;
        }
        found.push(station);
    }
    found
}

fn convert(raw: Raw) -> Option<Found> {
    let url = [raw.url_resolved, raw.url]
        .into_iter()
        .flatten()
        .map(|u| u.trim().to_owned())
        .find(|u| crate::config::is_stream_url(u))?;

    let name = raw.name.unwrap_or_default().trim().to_owned();
    let name = if name.is_empty() {
        url.trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_owned()
    } else {
        name
    };

    Some(Found {
        name,
        meta: meta_line(
            raw.countrycode.as_deref(),
            raw.codec.as_deref(),
            raw.bitrate,
            raw.tags.as_deref(),
        ),
        url,
        checked: false,
    })
}

/// The second line of a result: country, codec, bitrate and a few tags.
fn meta_line(
    country: Option<&str>,
    codec: Option<&str>,
    bitrate: Option<u32>,
    tags: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(country) = country.map(str::trim).filter(|c| !c.is_empty()) {
        parts.push(country.to_uppercase());
    }

    let codec = codec
        .map(str::trim)
        .filter(|c| !c.is_empty() && *c != "UNKNOWN");
    match (codec, bitrate.filter(|b| *b > 0)) {
        (Some(codec), Some(bitrate)) => parts.push(format!("{codec} {bitrate} kbps")),
        (Some(codec), None) => parts.push(codec.to_owned()),
        (None, Some(bitrate)) => parts.push(format!("{bitrate} kbps")),
        (None, None) => {}
    }

    let tags: Vec<&str> = tags
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .take(MAX_TAGS)
        .collect();
    if !tags.is_empty() {
        parts.push(tags.join(", "));
    }

    parts.join("  •  ")
}

/// Whether the text is a stream address, or something to search for.
pub fn looks_like_address(text: &str) -> bool {
    let text = text.trim();
    text.contains("://") || text.starts_with("www.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(name: &str, url: &str) -> Raw {
        Raw {
            name: Some(name.to_owned()),
            url: Some(url.to_owned()),
            url_resolved: None,
            codec: None,
            bitrate: None,
            countrycode: None,
            tags: None,
        }
    }

    #[test]
    fn tells_an_address_from_a_search() {
        assert!(looks_like_address("https://example.com/live"));
        assert!(looks_like_address("  http://example.com:8000  "));
        assert!(looks_like_address("www.example.com/live"));
        assert!(!looks_like_address("jazz"));
        assert!(!looks_like_address("Σκάι 100.3"));
    }

    #[test]
    fn puts_the_query_straight_after_byname() {
        assert_eq!(
            endpoint("radio paradise").unwrap().as_str(),
            "https://de1.api.radio-browser.info/json/stations/byname/radio%20paradise\
             ?limit=100&order=votes&reverse=true&hidebroken=true"
        );
    }

    #[test]
    fn survives_a_trailing_slash_on_the_endpoint() {
        let mut url = Url::parse(&format!("{API}/")).unwrap();
        url.path_segments_mut().unwrap().pop_if_empty().push("jazz");
        assert!(url.as_str().ends_with("/byname/jazz"), "{url}");
    }

    #[test]
    fn prefers_the_resolved_address() {
        let mut entry = raw("One", "http://one.example/live");
        entry.url_resolved = Some("https://cdn.one.example/live".to_owned());
        assert_eq!(convert(entry).unwrap().url, "https://cdn.one.example/live");
    }

    #[test]
    fn drops_entries_without_a_playable_address() {
        assert!(convert(raw("Broken", "")).is_none());
        assert!(convert(raw("Odd", "rtsp://one.example/live")).is_none());
    }

    #[test]
    fn names_a_nameless_station_after_its_address() {
        assert_eq!(
            convert(raw("  ", "https://one.example/live")).unwrap().name,
            "one.example/live"
        );
    }

    #[test]
    fn keeps_the_same_stream_only_once() {
        let found = collect(vec![
            raw("One", "https://one.example/live"),
            raw("One HQ", "https://one.example/live"),
            raw("Two", "https://two.example/live"),
        ]);
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["One", "Two"]);
    }

    #[test]
    fn joins_only_what_the_directory_knows() {
        assert_eq!(
            meta_line(
                Some("gr"),
                Some("MP3"),
                Some(128),
                Some("jazz, blues, ,soul, funk")
            ),
            "GR  •  MP3 128 kbps  •  jazz, blues, soul"
        );
        assert_eq!(meta_line(None, Some("UNKNOWN"), Some(0), Some("")), "");
        assert_eq!(meta_line(None, None, Some(64), None), "64 kbps");
    }

    /// Hits the real directory, hence by hand only:
    /// `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore = "needs network"]
    fn finds_real_stations() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let found = runtime.block_on(fetch("jazz")).expect("the search failed");
        assert!(!found.is_empty(), "the directory sent nothing back");
        println!("{:#?}", &found[..found.len().min(5)]);
    }
}
