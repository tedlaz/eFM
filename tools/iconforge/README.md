# iconforge

The eFM icon is generated, not drawn by hand. This crate is its source: the
three files in `assets/` are all output, and editing them directly means the
next run overwrites the change.

```sh
cd tools/iconforge
cargo run --release --bin forge     # writes ../../assets/
cargo run --release --bin verify    # reads them back and checks them
```

`forge` writes all three assets:

| File | Used by |
| --- | --- |
| `assets/icon.ico` | the taskbar and Explorer, embedded into the .exe by `build.rs` |
| `assets/icon-64.rgba` | the window icon, embedded by `src/main.rs` |
| `assets/icon-256.png` | the Linux and macOS packages |

`verify` decodes every entry of the `.ico`, checks each is the size it declares,
and writes a contact sheet to `out/icon-sizes.png` showing all eight sizes at
their true scale.

## How it draws

Shapes are signed distance fields, painted at 4x and box-filtered down
(`src/lib.rs`). Nothing is ever resampled from the 256 master: **each size is
drawn at its own resolution**, and the drawing is retuned per size. Five bars
are elegant at 256 and a grey smear at 16, so below 28px the meter drops to
three thicker bars and the disc grows to fill more of the frame. Those three
tiers are the `Tier` constants at the top of `src/bin/forge.rs`.

## Kept in step with the title bar

`app_logo` in `src/ui.rs` draws this same mark inside the window, and the two are
meant to be identical: same four bars, same `HEIGHTS`, same gold on the third
one. **Change the proportions here and change them there too**, or the title bar
and the taskbar drift apart.

Two things about the shape are load-bearing, and worth knowing before adjusting
them:

- **Four bars, never three.** Three bars — two short ones flanking a tall one —
  read as a pause symbol, which is the one thing a player must not say about
  itself. The small tiers therefore widen the bars rather than drop one.
- **Rising then falling.** A symmetric arrangement reads as pause for the same
  reason; the uneven run is what makes it a meter.

There is no plate or disc behind the bars, so the icon is mostly transparent and
sits on whatever the desktop provides. It was checked against white, light grey
and dark taskbar grounds.

Colours come from `src/lib.rs` and mirror `src/theme.rs`. They are duplicated
rather than shared, because this crate is deliberately not part of the app's
build — it is not a workspace member, so `cargo build` at the repo root never
compiles it.
