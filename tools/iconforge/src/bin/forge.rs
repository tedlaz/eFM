//! Builds the eFM icon set: the level meter, on its own, in the Aurora Night
//! palette. Writes straight into `assets/`.
//!
//! This is the same mark `app_logo` draws in the title bar — four bars rising
//! then falling, the tall one in gold — with no disc or plate behind it. The
//! shape carries the icon on its own and the desktop shows through.
//!
//! Every size is drawn from the vector description at its own resolution rather
//! than shrunk from the 256 master, and the description is retuned per tier:
//! bars that are elegantly slim at 256 fall below a pixel and a half at 16, so
//! the small tiers widen the bars and tighten the gaps. The bar count never
//! changes — four bars *are* the mark, and three of them read as a pause symbol.

use iconforge::*;

/// `assets/`, found relative to this crate rather than to the shell's cwd.
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");

/// Bar heights as fractions of the tallest one, matching `app_logo`.
const HEIGHTS: [f32; 4] = [0.45, 0.75, 1.00, 0.60];
/// Which bar carries the gold.
const ACCENT_BAR: usize = 2;

/// How the drawing is tuned for a given output size.
struct Tier {
    /// The tallest bar, in the 256 design space.
    height: f32,
    /// Bar width and the gap between two, as fractions of `height`.
    bar: f32,
    gap: f32,
}

/// The title bar's own proportions, used wherever there is room for them.
const LARGE: Tier = Tier { height: 176.0, bar: 0.200, gap: 0.125 };
const MEDIUM: Tier = Tier { height: 182.0, bar: 0.225, gap: 0.115 };
const SMALL: Tier = Tier { height: 186.0, bar: 0.255, gap: 0.095 };

fn tier_for(n: u32) -> &'static Tier {
    match n {
        0..=28 => &SMALL,
        29..=55 => &MEDIUM,
        _ => &LARGE,
    }
}

fn render(n: u32) -> image::RgbaImage {
    let t = tier_for(n);
    let mut c = Canvas::new(n * SS);

    let (w, gap) = (t.height * t.bar, t.height * t.gap);
    let span = 4.0 * w + 3.0 * gap;
    let x0 = 128.0 - span * 0.5 + w * 0.5;

    for (i, factor) in HEIGHTS.iter().enumerate() {
        let cx = x0 + i as f32 * (w + gap);
        let h = t.height * factor;
        let colour = if i == ACCENT_BAR { GOLD } else { BLUE_MID };
        c.fill(
            move |x, y| sd_round_rect(x, y, cx, 128.0, w * 0.5, h * 0.5, w * 0.5),
            colour,
        );
    }

    c.resolve(n)
}

fn main() {
    let master = render(256);
    std::fs::write(format!("{ASSETS}/icon-256.png"), encode_png(&master)).expect("write png");
    println!("icon-256.png   drawn at 256");

    let window = render(64);
    let raw: Vec<u8> = window.pixels().flat_map(|p| p.0).collect();
    // `src/main.rs` asserts this length when it embeds the buffer.
    assert_eq!(raw.len(), 64 * 64 * 4, "the window icon must stay 64x64 RGBA");
    std::fs::write(format!("{ASSETS}/icon-64.rgba"), &raw).expect("write rgba");
    println!("icon-64.rgba   drawn at 64 ({} bytes)", raw.len());

    let sizes = [16u32, 20, 24, 32, 48, 64, 128, 256];
    let entries: Vec<(u32, Vec<u8>)> = sizes
        .iter()
        .map(|&n| {
            let img = if n == 256 { master.clone() } else { render(n) };
            (n, encode_png(&img))
        })
        .collect();
    std::fs::write(format!("{ASSETS}/icon.ico"), write_ico(&entries)).expect("write ico");
    println!(
        "icon.ico       drawn at [{}]",
        sizes.map(|n| n.to_string()).join(", ")
    );
}
