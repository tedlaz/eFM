# eFM

[![CI](https://github.com/tedlaz/eFM/actions/workflows/ci.yml/badge.svg)](https://github.com/tedlaz/eFM/actions/workflows/ci.yml)

A minimal desktop player for internet radio streams, written in Rust.

- Enter a stream address (Icecast/Shoutcast, MP3/AAC/OGG/FLAC).
- One **Play** button, which reads **Live** in green while the stream is playing
  and turns into a red **Stop** as soon as the mouse goes over it. Stop drops the
  stream; Play connects again and picks the station up live. The volume control
  sits next to it.
- Shows the ICY metadata when the station sends it: artist, track, album (if
  present), station name, genre and bitrate.
- On startup it resumes the last stream automatically.
- If the connection drops, it reconnects on its own after 3 seconds.

## Download

Ready-made packages for every release are on the
[Releases](https://github.com/tedlaz/eFM/releases) page:

| System | File | Install |
| --- | --- | --- |
| Windows 10/11 | `…-windows-x86_64.zip` | Unzip and run `eFM.exe` |
| Linux (any distro) | `…-x86_64.AppImage` | `chmod +x eFM-*.AppImage && ./eFM-*.AppImage` |
| Linux (Flatpak) | `…-x86_64.flatpak` | `flatpak install --user eFM-*.flatpak` |
| macOS 11+ | `…-macos-universal.dmg` | Open it and drag the app to Applications |

The executables are not code-signed, so SmartScreen and Gatekeeper will complain
the first time. On macOS this clears it:

```sh
xattr -dr com.apple.quarantine /Applications/eFM.app
```

Every release also ships a `SHA256SUMS` file for verifying the downloads.

## Running from source

```sh
cargo run --release
```

On Linux the build needs a few development packages (Debian/Ubuntu names):

```sh
sudo apt-get install -y pkg-config libssl-dev libasound2-dev \
  libgl1-mesa-dev libegl1-mesa-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libx11-dev libxcursor-dev libxrandr-dev libxi-dev
```

Windows and macOS need nothing beyond a Rust toolchain.

## Settings

Stored as JSON, holding the last stream, the history and the volume:

| System | Path |
| --- | --- |
| Windows | `%APPDATA%\eFM\config\config.json` |
| Linux | `~/.config/eFM/config.json` |
| macOS | `~/Library/Application Support/eFM/config.json` |

Setting `autoplay: false` disables the automatic start.

## Tests

```sh
cargo test                      # unit tests, no network
cargo test -- --ignored         # connects to a real station
```

## Layout

| File | Role |
| --- | --- |
| `src/player.rs` | Stream download, ICY metadata parsing, playback (`stream-download` + `rodio`) |
| `src/ui.rs` | The interface, in `egui` |
| `src/theme.rs` | Colours, styling and the scrolling text |
| `src/config.rs` | Settings storage |
| `src/main.rs` | Startup, autoplay, reconnect |
| `build.rs` | Embeds `assets/icon.ico` into the `.exe` |
| `assets/` | The icon: `.ico` for the exe, `.rgba` for the window, `.png` as the source |
| `packaging/` | Packaging metadata: `.desktop` and AppStream for Linux, the Flatpak manifest, `Info.plist` for macOS |
| `.github/workflows/` | `ci.yml` (fmt, clippy, tests on three platforms) and `release.yml` (artifacts → Releases) |

## Continuous integration

`ci.yml` runs on every push and pull request, on Linux, Windows and macOS:
`cargo fmt --check`, `cargo clippy` (warnings are errors) and `cargo test`. The
`--ignored` test is left out — it needs a live station.

## Preparing a new release

`release.yml` does the building and publishing. It is triggered by pushing a tag
that starts with `v`, and it produces the four artifacts, a `SHA256SUMS` file and
a GitHub Release that holds them all.

### 1. Make sure `master` is green

Check that the latest CI run succeeded, and that you have nothing uncommitted:

```sh
git switch master
git pull
git status
```

### 2. Bump the version

Three places have to agree. Using `0.2.0` as the example:

1. `Cargo.toml` — the `version` field of `[package]`:

   ```toml
   [package]
   name = "efm"
   version = "0.2.0"
   ```

2. `Cargo.lock` — refresh it so the new version lands in the lock file. The
   release build uses `--locked` and fails if the two disagree:

   ```sh
   cargo check
   ```

3. `packaging/linux/io.github.tedlaz.eFM.metainfo.xml` — add a `<release>` entry
   at the top of `<releases>`, with today's date in `YYYY-MM-DD`:

   ```xml
   <release version="0.2.0" date="2026-09-01">
     <description>
       <p>What changed in this version.</p>
     </description>
   </release>
   ```

   Software centres (GNOME Software, KDE Discover) read this file, so an entry
   that is missing means the update shows up without a changelog.

### 3. Run the checks locally

Faster than finding out from CI:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

### 4. Commit and tag

The tag must match the `Cargo.toml` version, with a `v` in front. If they differ
the build still runs, but it logs a warning and the file names follow the tag.

```sh
git add Cargo.toml Cargo.lock packaging/linux/io.github.tedlaz.eFM.metainfo.xml
git commit -m "Release 0.2.0"
git tag -a v0.2.0 -m "eFM 0.2.0"
git push origin master --follow-tags
```

### 5. Watch the build

```sh
gh run watch
```

Or open the **Actions** tab. Five jobs run: `meta`, `windows`, `linux`,
`flatpak` (waits for `linux`, whose binary it packages) and `macos`; then
`release` collects everything. Expect roughly 10–15 minutes, most of it spent on
the macOS job, which compiles twice — once per architecture.

### 6. Check the result

The Release appears under **Releases** with five files attached:

```
eFM-0.2.0-windows-x86_64.zip
eFM-0.2.0-x86_64.AppImage
eFM-0.2.0-x86_64.flatpak
eFM-0.2.0-macos-universal.dmg
SHA256SUMS
```

The release notes are generated from the commits since the previous tag, on top
of the download table. Edit the text on GitHub if you want to say more.

### If something fails

The tag is what triggers the run, so a fix means replacing the tag. Delete the
half-finished Release first (from the GitHub UI, or `gh release delete v0.2.0`),
then:

```sh
git tag -d v0.2.0
git push origin :refs/tags/v0.2.0
# fix, commit, and tag again
git tag -a v0.2.0 -m "eFM 0.2.0"
git push origin master --follow-tags
```

To rehearse without touching a tag, run the workflow by hand: **Actions → Release
→ Run workflow**, and give it the tag name. It builds the same artifacts and
creates the tag itself at the end.

### One-time repository setup

For the release job to be able to publish, GitHub Actions needs write access to
the repository: **Settings → Actions → General → Workflow permissions →
Read and write permissions**. The workflow already asks for `contents: write`,
but that repository setting has the final say.
