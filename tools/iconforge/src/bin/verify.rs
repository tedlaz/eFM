//! Reads the written assets back and checks each one is what it claims to be.
//! Also dumps a contact sheet of every .ico entry into `out/`, so the small
//! sizes can be eyeballed rather than taken on trust.

const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");
const OUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/out");

fn main() {
    let master = image::open(format!("{ASSETS}/icon-256.png")).expect("open icon-256.png");
    println!("icon-256.png  {}x{}", master.width(), master.height());
    assert_eq!((master.width(), master.height()), (256, 256));

    let raw = std::fs::read(format!("{ASSETS}/icon-64.rgba")).expect("read rgba");
    println!("icon-64.rgba  {} bytes", raw.len());
    assert_eq!(raw.len(), 64 * 64 * 4, "src/main.rs asserts this length");

    let ico = std::fs::read(format!("{ASSETS}/icon.ico")).expect("read ico");
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    println!("icon.ico      {} entries, {} bytes", count, ico.len());

    let mut decoded = Vec::new();
    for i in 0..count {
        let e = 6 + i * 16;
        // A width byte of zero means 256; that is how the format encodes it.
        let declared = if ico[e] == 0 { 256 } else { ico[e] as u32 };
        let len = u32::from_le_bytes(ico[e + 8..e + 12].try_into().unwrap()) as usize;
        let off = u32::from_le_bytes(ico[e + 12..e + 16].try_into().unwrap()) as usize;
        assert!(off + len <= ico.len(), "entry {i} runs past the end of the file");

        let img = image::load_from_memory_with_format(&ico[off..off + len], image::ImageFormat::Png)
            .unwrap_or_else(|e| panic!("entry {i} ({declared}px) did not decode: {e}"));
        assert_eq!(
            (img.width(), img.height()),
            (declared, declared),
            "entry {i} is not the size it declares"
        );
        println!("  {declared:>3}px  {len:>6} bytes  ok");
        decoded.push(img.to_rgba8());
    }

    // The contact sheet: every size at its true scale, on one strip.
    let pad = 8u32;
    let width: u32 = decoded.iter().map(|d| d.width() + pad).sum::<u32>() + pad;
    let mut sheet =
        image::RgbaImage::from_pixel(width, 256 + pad * 2, image::Rgba([24, 27, 35, 255]));
    let mut x = pad;
    for d in &decoded {
        image::imageops::overlay(&mut sheet, d, x as i64, (pad + (256 - d.height()) / 2) as i64);
        x += d.width() + pad;
    }
    std::fs::create_dir_all(OUT).expect("create out dir");
    sheet.save(format!("{OUT}/icon-sizes.png")).expect("save sheet");
    println!("\ncontact sheet -> out/icon-sizes.png");
}
