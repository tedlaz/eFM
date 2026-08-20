//! Shared drawing toolkit for the eFM icon work: signed distance fields painted
//! at 4x and box-filtered down, plus the Aurora Night colours and the bits that
//! write out PNG, raw RGBA and .ico files.

pub const S: f32 = 256.0;
pub const SS: u32 = 4;

pub const TILE_HI: [f32; 3] = [0x17 as f32, 0x21 as f32, 0x3B as f32];
pub const TILE_LO: [f32; 3] = [0x08 as f32, 0x0C as f32, 0x16 as f32];
pub const BACKDROP: [f32; 3] = [0x0F as f32, 0x14 as f32, 0x24 as f32];
pub const BLUE_HI: [f32; 3] = [0x7D as f32, 0xD3 as f32, 0xFC as f32];
pub const BLUE_MID: [f32; 3] = [0x38 as f32, 0xBD as f32, 0xF8 as f32];
pub const BLUE_LO: [f32; 3] = [0x1E as f32, 0x56 as f32, 0x87 as f32];
pub const GOLD: [f32; 3] = [0xFB as f32, 0xBF as f32, 0x24 as f32];
pub const GOLD_HI: [f32; 3] = [0xFD as f32, 0xD9 as f32, 0x6A as f32];

pub fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}
pub fn len(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

pub fn sd_round_rect(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let r = r.min(hx).min(hy);
    let qx = (px - cx).abs() - (hx - r);
    let qy = (py - cy).abs() - (hy - r);
    len(qx.max(0.0), qy.max(0.0)) + qx.max(qy).min(0.0) - r
}
pub fn sd_circle(px: f32, py: f32, cx: f32, cy: f32, rad: f32) -> f32 {
    len(px - cx, py - cy) - rad
}
pub fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let (wx, wy) = (px - ax, py - ay);
    let t = ((wx * vx + wy * vy) / (vx * vx + vy * vy).max(1e-6)).clamp(0.0, 1.0);
    len(wx - vx * t, wy - vy * t)
}
/// An arc centred on `(cx,cy)` opening toward `facing` degrees, `span` wide.
pub fn sd_arc(
    px: f32,
    py: f32,
    cx: f32,
    cy: f32,
    rad: f32,
    half: f32,
    facing: f32,
    span: f32,
) -> f32 {
    let (dx, dy) = (px - cx, py - cy);
    let mut off = (dy.atan2(dx).to_degrees() - facing).abs() % 360.0;
    if off > 180.0 {
        off = 360.0 - off;
    }
    if off <= span {
        (len(dx, dy) - rad).abs() - half
    } else {
        let a = (facing + span).to_radians();
        let b = (facing - span).to_radians();
        (len(dx - rad * a.cos(), dy - rad * a.sin()) - half)
            .min(len(dx - rad * b.cos(), dy - rad * b.sin()) - half)
    }
}

pub struct Canvas {
    pub n: u32,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    pub fn new(n: u32) -> Self {
        Self { n, px: vec![[0.0; 4]; (n * n) as usize] }
    }

    /// Composites a shape. `sdf` and `paint` both work in 256-design space.
    /// `paint` returns a colour and an alpha.
    pub fn draw<F, P>(&mut self, sdf: F, paint: P)
    where
        F: Fn(f32, f32) -> f32,
        P: Fn(f32, f32) -> ([f32; 3], f32),
    {
        let scale = S / self.n as f32;
        for y in 0..self.n {
            for x in 0..self.n {
                let (dx, dy) = ((x as f32 + 0.5) * scale, (y as f32 + 0.5) * scale);
                let cov = (0.5 - sdf(dx, dy) / scale).clamp(0.0, 1.0);
                if cov <= 0.0 {
                    continue;
                }
                let (rgb, alpha) = paint(dx, dy);
                let a = cov * alpha;
                if a <= 0.0 {
                    continue;
                }
                let dst = &mut self.px[(y * self.n + x) as usize];
                for i in 0..3 {
                    dst[i] = rgb[i] * a + dst[i] * (1.0 - a);
                }
                dst[3] = a * 255.0 + dst[3] * (1.0 - a);
            }
        }
    }

    /// Convenience for the common case of a flat, fully opaque fill.
    pub fn fill<F>(&mut self, sdf: F, rgb: [f32; 3])
    where
        F: Fn(f32, f32) -> f32,
    {
        self.draw(sdf, move |_, _| (rgb, 1.0));
    }

    pub fn resolve(&self, out: u32) -> image::RgbaImage {
        let k = self.n / out;
        let mut img = image::RgbaImage::new(out, out);
        for y in 0..out {
            for x in 0..out {
                let mut acc = [0.0f32; 4];
                for j in 0..k {
                    for i in 0..k {
                        let s = self.px[((y * k + j) * self.n + (x * k + i)) as usize];
                        let a = s[3] / 255.0;
                        for c in 0..3 {
                            acc[c] += s[c] * a; // premultiply before averaging
                        }
                        acc[3] += s[3];
                    }
                }
                let n = (k * k) as f32;
                let a = acc[3] / n;
                let unp = if a > 0.5 { 255.0 / a } else { 0.0 };
                img.put_pixel(
                    x,
                    y,
                    image::Rgba([
                        ((acc[0] / n) * unp).round().clamp(0.0, 255.0) as u8,
                        ((acc[1] / n) * unp).round().clamp(0.0, 255.0) as u8,
                        ((acc[2] / n) * unp).round().clamp(0.0, 255.0) as u8,
                        a.round().clamp(0.0, 255.0) as u8,
                    ]),
                );
            }
        }
        img
    }
}

/// The rounded-square plate most of the concepts sit on.
pub fn tile_sdf(x: f32, y: f32, half: f32, round: f32) -> f32 {
    sd_round_rect(x, y, 128.0, 128.0, half, half, round)
}

pub fn premul_resize(src: &image::RgbaImage, n: u32) -> image::RgbaImage {
    let mut pm = src.clone();
    for p in pm.pixels_mut() {
        let a = p.0[3] as f32 / 255.0;
        for i in 0..3 {
            p.0[i] = (p.0[i] as f32 * a).round() as u8;
        }
    }
    let mut out = image::imageops::resize(&pm, n, n, image::imageops::FilterType::Lanczos3);
    for p in out.pixels_mut() {
        let a = p.0[3] as f32 / 255.0;
        if a > 0.004 {
            for i in 0..3 {
                p.0[i] = (p.0[i] as f32 / a).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

pub fn encode_png(img: &image::RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode png");
    out
}

/// Builds an .ico out of PNG payloads, 32-bit, as Windows expects.
pub fn write_ico(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + entries.len() * 16;
    for (n, png) in entries {
        let dim = if *n == 256 { 0u8 } else { *n as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in entries {
        out.extend_from_slice(png);
    }
    out
}

/// Lays concepts out in a column each: the 256 render on top, the sizes that
/// actually matter underneath.
pub fn contact_sheet(bigs: &[image::RgbaImage], ground: [u8; 3]) -> image::RgbaImage {
    let small = [64u32, 48, 32, 24, 16];
    let pad = 26u32;
    let col = 256 + pad;
    let w = pad + bigs.len() as u32 * col;
    let h = pad + 256 + 28 + 64 + pad;
    let mut sheet =
        image::RgbaImage::from_pixel(w, h, image::Rgba([ground[0], ground[1], ground[2], 255]));

    for (i, big) in bigs.iter().enumerate() {
        let x = pad + i as u32 * col;
        image::imageops::overlay(&mut sheet, big, x as i64, pad as i64);
        let mut sx = x;
        let base = pad + 256 + 28;
        for n in small {
            let s = premul_resize(big, n);
            image::imageops::overlay(&mut sheet, &s, sx as i64, (base + (64 - n)) as i64);
            sx += n + 10;
        }
    }
    sheet
}
