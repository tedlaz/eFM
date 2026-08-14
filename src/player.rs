//! The audio engine: downloads the HTTP stream, reads the ICY metadata and plays it.
//!
//! The UI talks only through [`Player`]; all the work happens on a tokio runtime
//! on separate threads and the state comes back through a shared [`SharedState`].

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use icy_metadata::{IcyHeaders, IcyMetadata, IcyMetadataReader, RequestIcyMetadata};
use stream_download::http::HttpStream;
use stream_download::http::reqwest::Client;
use stream_download::source::DecodeError;
use stream_download::storage::bounded::BoundedStorageProvider;
use stream_download::storage::memory::MemoryStorageProvider;
use stream_download::{Settings, StreamDownload};

/// Size of the ring buffer in memory. It has to be comfortably large, otherwise
/// the decoder may ask for bytes that have already been evicted.
const BUFFER_BYTES: usize = 512 * 1024;
/// How many seconds of audio we prefetch before playback starts.
const PREFETCH_SECONDS: u64 = 5;
/// Fallback prefetch when the station declares no bitrate (~128 kbps x 5s).
const DEFAULT_PREFETCH_BYTES: u64 = 128 / 8 * 1024 * PREFETCH_SECONDS;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Status {
    /// Nothing is playing.
    #[default]
    Idle,
    /// Connecting to the station / filling the buffer.
    Connecting,
    Playing,
    Paused,
    Error(String),
}

impl Status {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Connecting | Self::Playing | Self::Paused)
    }
}

/// Everything we currently know about the station and the track that is playing.
#[derive(Debug, Default, Clone)]
pub struct NowPlaying {
    // From the station's HTTP headers (constant while we stay connected).
    pub station: Option<String>,
    pub description: Option<String>,
    pub genre: Option<String>,
    pub bitrate: Option<u32>,
    pub station_url: Option<String>,
    // From the ICY metadata inside the stream (changes per track).
    pub artist: Option<String>,
    pub track: Option<String>,
    pub album: Option<String>,
    /// The raw `StreamTitle`, when it does not split into artist/title.
    pub raw_title: Option<String>,
}

impl NowPlaying {
    /// Clears only the per-track fields, keeping the station details.
    fn clear_track(&mut self) {
        self.artist = None;
        self.track = None;
        self.album = None;
        self.raw_title = None;
    }

    #[cfg(test)]
    pub fn has_track_info(&self) -> bool {
        self.artist.is_some() || self.track.is_some() || self.raw_title.is_some()
    }
}

#[derive(Debug, Default)]
pub struct SharedState {
    pub status: Status,
    pub now: NowPlaying,
}

pub struct Player {
    /// The output device; if it is dropped, the sound stops.
    _device: rodio::MixerDeviceSink,
    player: Arc<rodio::Player>,
    runtime: tokio::runtime::Runtime,
    state: Arc<Mutex<SharedState>>,
    /// Bumped on every play/stop so that slow, stale connections are ignored.
    generation: Arc<AtomicU64>,
    ctx: egui::Context,
}

impl Player {
    pub fn new(ctx: egui::Context) -> anyhow::Result<Self> {
        let device = rodio::DeviceSinkBuilder::open_default_sink()?;
        let player = rodio::Player::connect_new(device.mixer());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;

        Ok(Self {
            _device: device,
            player: Arc::new(player),
            runtime,
            state: Arc::new(Mutex::new(SharedState::default())),
            generation: Arc::new(AtomicU64::new(0)),
            ctx,
        })
    }

    pub fn state(&self) -> SharedStateSnapshot {
        let guard = self.state.lock().expect("state mutex poisoned");
        SharedStateSnapshot {
            status: guard.status.clone(),
            now: guard.now.clone(),
        }
    }

    pub fn status(&self) -> Status {
        self.state
            .lock()
            .expect("state mutex poisoned")
            .status
            .clone()
    }

    /// True when the station has dropped: we think we are playing but the queue ran dry.
    pub fn has_stalled(&self) -> bool {
        self.status() == Status::Playing && self.player.empty()
    }

    pub fn set_volume(&self, volume: f32) {
        self.player.set_volume(volume);
    }

    pub fn pause(&self) {
        self.player.pause();
        self.update_state(|s| {
            if s.status == Status::Playing {
                s.status = Status::Paused;
            }
        });
    }

    /// Resumes after a pause. Returns `false` if there is nothing left to resume
    /// (e.g. the buffer drained meanwhile) and a fresh connection is needed.
    pub fn resume(&self) -> bool {
        if self.player.empty() {
            return false;
        }
        self.player.play();
        self.update_state(|s| {
            if s.status == Status::Paused {
                s.status = Status::Playing;
            }
        });
        true
    }

    /// Starts (or switches to) a station. Returns immediately; the connection is
    /// made in the background.
    pub fn play_url(&self, url: &str) {
        let url = url.trim().to_owned();
        if url.is_empty() {
            self.update_state(|s| s.status = Status::Error("Enter a stream URL.".into()));
            return;
        }

        // Cancels whatever was running: clear() drops the decoder, which cancels
        // the download.
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.player.clear();
        self.update_state(|s| {
            s.status = Status::Connecting;
            s.now = NowPlaying::default();
        });

        let state = Arc::clone(&self.state);
        let player = Arc::clone(&self.player);
        let current = Arc::clone(&self.generation);
        let ctx = self.ctx.clone();

        self.runtime.spawn(async move {
            let result = connect(&url, Arc::clone(&state), ctx.clone()).await;

            // In the meantime the user may have switched stations.
            if current.load(Ordering::SeqCst) != generation {
                return;
            }

            let mut guard = state.lock().expect("state mutex poisoned");
            match result {
                Ok(source) => {
                    player.append(source);
                    player.play();
                    guard.status = Status::Playing;
                }
                Err(e) => {
                    guard.status = Status::Error(e.to_string());
                    guard.now = NowPlaying::default();
                }
            }
            drop(guard);
            ctx.request_repaint();
        });

        self.ctx.request_repaint();
    }

    fn update_state(&self, f: impl FnOnce(&mut SharedState)) {
        let mut guard = self.state.lock().expect("state mutex poisoned");
        f(&mut guard);
        drop(guard);
        self.ctx.request_repaint();
    }
}

/// Snapshot of the state, so the UI does not hold the mutex while it paints.
pub struct SharedStateSnapshot {
    pub status: Status,
    pub now: NowPlaying,
}

type RadioSource = rodio::Decoder<
    IcyMetadataReader<StreamDownload<BoundedStorageProvider<MemoryStorageProvider>>>,
>;

/// Opens the stream, fills the buffer and returns a ready audio source.
async fn connect(
    url: &str,
    state: Arc<Mutex<SharedState>>,
    ctx: egui::Context,
) -> anyhow::Result<RadioSource> {
    let url = url
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid URL: {url}"))?;

    // `Icy-MetaData: 1` tells the Icecast/Shoutcast server to interleave the
    // metadata into the stream itself.
    let client = Client::builder().request_icy_metadata().build()?;
    let stream = HttpStream::new(client, url).await?;

    let headers = IcyHeaders::parse_from_headers(stream.headers());
    let mime = stream
        .content_type()
        .as_ref()
        .map(|ct| format!("{}/{}", ct.r#type, ct.subtype));
    {
        let mut guard = state.lock().expect("state mutex poisoned");
        guard.now.station = headers.name().map(str::to_owned);
        guard.now.description = headers.description().map(str::to_owned);
        guard.now.station_url = headers.station_url().map(str::to_owned);
        guard.now.bitrate = headers.bitrate();
        let genre = headers.genre().join(", ");
        guard.now.genre = (!genre.is_empty()).then_some(genre);
    }
    ctx.request_repaint();

    let prefetch = headers.bitrate().map_or(DEFAULT_PREFETCH_BYTES, |kbps| {
        u64::from(kbps) / 8 * 1024 * PREFETCH_SECONDS
    });

    let metadata_interval = headers.metadata_interval();
    let buffer_size = NonZeroUsize::new(BUFFER_BYTES).expect("BUFFER_BYTES != 0");

    let reader = match StreamDownload::from_stream(
        stream,
        // Bounded storage in memory: a live stream never ends, so we do not want
        // the buffer to grow without limit.
        BoundedStorageProvider::new(MemoryStorageProvider, buffer_size),
        Settings::default().prefetch_bytes(prefetch),
    )
    .await
    {
        Ok(reader) => reader,
        Err(e) => return Err(anyhow::anyhow!("{}", e.decode_error().await)),
    };

    let reader = IcyMetadataReader::new(reader, metadata_interval, {
        let state = Arc::clone(&state);
        let ctx = ctx.clone();
        move |metadata| {
            if let Ok(metadata) = metadata {
                apply_metadata(&state, &metadata);
                ctx.request_repaint();
            }
        }
    });

    // The decoder reads (blocking) in order to recognise the audio format, so it
    // must not run on the async runtime.
    tokio::task::spawn_blocking(move || {
        let mut builder = rodio::Decoder::builder()
            .with_data(reader)
            // Live radio has no end, hence no seeking either.
            .with_seekable(false);
        if let Some(mime) = &mime {
            builder = builder.with_mime_type(mime);
        }
        builder.build()
    })
    .await?
    .map_err(|e| anyhow::anyhow!("Unrecognised audio format: {e}"))
}

/// Passes a new `StreamTitle` into the shared state.
fn apply_metadata(state: &Mutex<SharedState>, metadata: &IcyMetadata) {
    let mut guard = state.lock().expect("state mutex poisoned");
    let now = &mut guard.now;

    // Many stations send an empty StreamTitle between tracks. We keep what we knew
    // until the next track: if we cleared it, the card title briefly fell back to
    // "Playing" and the texts flickered on every track change.
    let Some(title) = metadata
        .stream_title()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    else {
        return;
    };

    now.clear_track();

    match split_artist_title(title) {
        Some((artist, track)) => {
            now.artist = Some(artist);
            now.track = Some(track);
        }
        None => now.raw_title = Some(title.to_owned()),
    }

    // The album is not part of the ICY standard, but some stations send it as a
    // custom field of their own.
    for (key, value) in metadata.custom_fields() {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.to_ascii_lowercase().as_str() {
            k if k.contains("album") => now.album = Some(value.to_owned()),
            k if k.contains("artist") => now.artist = Some(value.to_owned()),
            _ => {}
        }
    }
}

/// Splits the "Artist - Title" form that most stations send.
fn split_artist_title(title: &str) -> Option<(String, String)> {
    let (artist, track) = title.split_once(" - ")?;
    let (artist, track) = (artist.trim(), track.trim());
    (!artist.is_empty() && !track.is_empty()).then(|| (artist.to_owned(), track.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_usual_artist_title_form() {
        assert_eq!(
            split_artist_title("Nina Simone - Feeling Good"),
            Some(("Nina Simone".into(), "Feeling Good".into()))
        );
    }

    #[test]
    fn keeps_titles_that_do_not_split_cleanly() {
        // A dash without spaces, or an empty half: not a separator.
        assert_eq!(split_artist_title("Rock-A-Bye Baby"), None);
        assert_eq!(split_artist_title(" - Feeling Good"), None);
        assert_eq!(split_artist_title("news bulletin"), None);
    }

    /// Hits a real station, which is why it only runs by hand:
    /// `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore = "needs network"]
    fn connects_to_a_live_station() {
        let state = Arc::new(Mutex::new(SharedState::default()));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let source = runtime
            .block_on(connect(
                "https://ice6.somafm.com/groovesalad-128-mp3",
                Arc::clone(&state),
                egui::Context::default(),
            ))
            .expect("the connection failed");

        // The decoder recognised the audio and hands out samples.
        let mut source = source;
        assert!(
            rodio::Source::sample_rate(&source).get() > 0,
            "the decoder found no sample rate"
        );
        assert!(
            Iterator::next(&mut source).is_some(),
            "the decoder produced no audio"
        );

        // ICY metadata arrives every ~16 KB, so we pull a few seconds of audio to
        // trigger the callback.
        for _ in 0..(rodio::Source::sample_rate(&source).get() * 4) {
            if Iterator::next(&mut source).is_none() {
                break;
            }
        }

        let now = state.lock().unwrap().now.clone();
        assert!(now.station.is_some(), "the station name is missing");
        assert!(now.bitrate.is_some(), "the bitrate is missing");
        assert!(now.has_track_info(), "no track metadata arrived");
        println!("{now:#?}");
    }

    #[test]
    fn splits_only_on_the_first_separator() {
        assert_eq!(
            split_artist_title("AC/DC - Rock - Roll"),
            Some(("AC/DC".into(), "Rock - Roll".into()))
        );
    }
}
