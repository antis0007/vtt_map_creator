use crate::model::{EffectKind, MapDocument, MaterialKind};
use anyhow::{Context, Result};
use image::{ImageBuffer, Rgba};
use std::{fs, path::Path};

pub fn save_map(path: impl AsRef<Path>, doc: &MapDocument) -> Result<()> {
    let json = serde_json::to_string_pretty(doc)?;
    fs::write(path.as_ref(), json)
        .with_context(|| format!("failed to write {}", path.as_ref().display()))?;
    Ok(())
}

pub fn load_map(path: impl AsRef<Path>) -> Result<MapDocument> {
    let text = fs::read_to_string(path.as_ref())
        .with_context(|| format!("failed to read {}", path.as_ref().display()))?;
    let doc: MapDocument = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.as_ref().display()))?;
    Ok(doc)
}

pub fn export_png(path: impl AsRef<Path>, doc: &MapDocument) -> Result<()> {
    let w = doc.width * doc.tile_px;
    let h = doc.height * doc.tile_px;
    let mut img = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(w, h);

    for py in 0..h {
        for px in 0..w {
            let world = [
                px as f32 / doc.tile_px as f32,
                py as f32 / doc.tile_px as f32,
            ];
            let color = shaded_world(doc, world);
            img.put_pixel(px, py, Rgba([color[0], color[1], color[2], 255]));
        }
    }

    img.save(path.as_ref())
        .with_context(|| format!("failed to save {}", path.as_ref().display()))?;
    Ok(())
}

fn shaded_world(doc: &MapDocument, world: [f32; 2]) -> [u8; 3] {
    let tile = [world[0].floor() as i32, world[1].floor() as i32];
    let local = [world[0].fract(), world[1].fract()];
    let mut mat = doc.terrain_at_i32(tile[0], tile[1]);
    let mut eff = doc.effect_at_i32(tile[0], tile[1]);
    let blend = 0.2;

    let mut c = material_rgb(mat, world);
    c = apply_effect_cpu(c, eff, world);

    if mat.blends() {
        let l = doc.terrain_at_i32(tile[0] - 1, tile[1]);
        let r = doc.terrain_at_i32(tile[0] + 1, tile[1]);
        let t = doc.terrain_at_i32(tile[0], tile[1] - 1);
        let b = doc.terrain_at_i32(tile[0], tile[1] + 1);
        let wl = edge_weight(local[0], blend);
        let wr = edge_weight(1.0 - local[0], blend);
        let wt = edge_weight(local[1], blend);
        let wb = edge_weight(1.0 - local[1], blend);

        let mut sum = c;
        let mut wsum = 1.0_f32;
        if l != mat {
            let lc = apply_effect_cpu(
                material_rgb(l, world),
                doc.effect_at_i32(tile[0] - 1, tile[1]),
                world,
            );
            sum[0] += lc[0] * wl;
            sum[1] += lc[1] * wl;
            sum[2] += lc[2] * wl;
            wsum += wl;
        }
        if r != mat {
            let rc = apply_effect_cpu(
                material_rgb(r, world),
                doc.effect_at_i32(tile[0] + 1, tile[1]),
                world,
            );
            sum[0] += rc[0] * wr;
            sum[1] += rc[1] * wr;
            sum[2] += rc[2] * wr;
            wsum += wr;
        }
        if t != mat {
            let tc = apply_effect_cpu(
                material_rgb(t, world),
                doc.effect_at_i32(tile[0], tile[1] - 1),
                world,
            );
            sum[0] += tc[0] * wt;
            sum[1] += tc[1] * wt;
            sum[2] += tc[2] * wt;
            wsum += wt;
        }
        if b != mat {
            let bc = apply_effect_cpu(
                material_rgb(b, world),
                doc.effect_at_i32(tile[0], tile[1] + 1),
                world,
            );
            sum[0] += bc[0] * wb;
            sum[1] += bc[1] * wb;
            sum[2] += bc[2] * wb;
            wsum += wb;
        }
        c = [sum[0] / wsum, sum[1] / wsum, sum[2] / wsum];

        if mat.diagonal_blend() {
            let corner = blend * 1.35;
            if local[0] + local[1] < corner {
                let nw = doc.terrain_at_i32(tile[0] - 1, tile[1] - 1);
                if l == t && l != mat && nw == l {
                    mat = nw;
                    eff = doc.effect_at_i32(tile[0] - 1, tile[1] - 1);
                    c = apply_effect_cpu(material_rgb(mat, world), eff, world);
                }
            }
        }
    }

    [
        c[0].clamp(0.0, 255.0) as u8,
        c[1].clamp(0.0, 255.0) as u8,
        c[2].clamp(0.0, 255.0) as u8,
    ]
}

fn edge_weight(distance: f32, blend: f32) -> f32 {
    1.0 - smoothstep(0.0, blend, distance)
}

fn apply_effect_cpu(mut c: [f32; 3], effect: EffectKind, p: [f32; 2]) -> [f32; 3] {
    match effect {
        EffectKind::None => c,
        EffectKind::Wind => {
            let gust = 0.5 + 0.5 * ((p[0] * 4.0 + p[1] * 1.3).sin() * (p[1] * 3.0).cos());
            c[1] += 10.0 * gust;
            c
        }
        EffectKind::Rain => {
            let streak = ((p[0] * 12.0 + p[1] * 28.0).fract() * 2.0 - 1.0).abs();
            let mask = (1.0 - smoothstep(0.65, 0.95, streak)) * 18.0;
            c[0] += mask;
            c[1] += mask;
            c[2] += mask * 1.4;
            c
        }
    }
}

fn material_rgb(mat: MaterialKind, p: [f32; 2]) -> [f32; 3] {
    let n1 = fbm(p[0] * 2.7, p[1] * 2.7, 4);
    let n2 = fbm(p[0] * 6.4 + 11.0, p[1] * 6.4 - 3.0, 3);
    match mat {
        MaterialKind::Dirt => tint([103.0, 78.0, 54.0], 0.75 + n1 * 0.18 + n2 * 0.07),
        MaterialKind::Grass => {
            let blade = (p[0] * 11.0 + p[1] * 2.0 + n2 * 2.0).sin().abs();
            tint([79.0, 129.0, 60.0], 0.78 + n1 * 0.14 + blade * 0.10)
        }
        MaterialKind::Sand => {
            let ripple = 0.5 + 0.5 * (p[0] * 9.0 + p[1] * 1.1 + n1 * 2.0).sin();
            tint([194.0, 176.0, 121.0], 0.82 + n1 * 0.10 + ripple * 0.08)
        }
        MaterialKind::Water => {
            let wave = 0.5 + 0.5 * ((p[0] * 7.5 + n1 * 3.5).sin() + (p[1] * 5.0).cos()) * 0.5;
            let c = tint([35.0, 94.0, 148.0], 0.78 + wave * 0.25);
            [c[0], c[1] + 12.0, c[2] + 18.0]
        }
        MaterialKind::Lava => {
            let molten = (p[0] * 8.0 + n2 * 4.0).sin() * (p[1] * 7.0 - n1 * 2.0).cos();
            let glow = 0.5 + 0.5 * molten;
            let c = tint([165.0, 55.0, 18.0], 0.72 + glow * 0.35);
            [c[0] + 30.0 * glow, c[1] + 18.0 * glow, c[2]]
        }
        MaterialKind::Gravel => {
            let pebble = hash2((p[0] * 5.0) as i32, (p[1] * 5.0) as i32, 9);
            let c = tint([118.0, 118.0, 114.0], 0.82 + n1 * 0.08);
            [
                c[0] + pebble * 18.0,
                c[1] + pebble * 18.0,
                c[2] + pebble * 18.0,
            ]
        }
        MaterialKind::Brick => {
            let seam = topdown_grid(p, 2.0, 2.0, 0.06);
            mix(
                tint([138.0, 62.0, 48.0], 0.82 + n1 * 0.10),
                [181.0, 172.0, 159.0],
                seam,
            )
        }
        MaterialKind::Path => {
            let chip = hash2((p[0] * 4.0) as i32, (p[1] * 4.0) as i32, 21) * 0.20 + n1 * 0.10;
            tint([126.0, 112.0, 91.0], 0.82 + chip)
        }
        MaterialKind::Wall => {
            let seam = topdown_grid(p, 2.0, 1.4, 0.07);
            mix(
                tint([136.0, 143.0, 150.0], 0.82 + n1 * 0.10),
                [95.0, 99.0, 102.0],
                seam,
            )
        }
        MaterialKind::WallDoor => {
            let seam = topdown_grid(p, 2.0, 1.4, 0.07);
            mix(
                tint([126.0, 88.0, 55.0], 0.86 + n1 * 0.10),
                [150.0, 156.0, 160.0],
                seam,
            )
        }
        MaterialKind::WallWindow => {
            let seam = topdown_grid(p, 2.0, 1.4, 0.07);
            mix(
                tint([96.0, 123.0, 152.0], 0.86 + n1 * 0.10),
                [156.0, 162.0, 166.0],
                seam,
            )
        }
        MaterialKind::FloorWood => {
            let seam = topdown_grid(p, 3.4, 1.0, 0.06);
            mix(
                tint([152.0, 112.0, 72.0], 0.84 + n1 * 0.10),
                [85.0, 61.0, 39.0],
                seam,
            )
        }
    }
}

fn topdown_grid(p: [f32; 2], sx: f32, sy: f32, width: f32) -> f32 {
    let fx = (p[0] * sx).fract();
    let fy = (p[1] * sy).fract();
    let gx: f32 = if fx.min(1.0 - fx) < width { 1.0 } else { 0.0 };
    let gy: f32 = if fy.min(1.0 - fy) < width { 1.0 } else { 0.0 };
    gx.max(gy)
}

fn fbm(mut x: f32, mut y: f32, octaves: usize) -> f32 {
    let mut amp = 0.5;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for _ in 0..octaves {
        sum += amp * noise2(x, y);
        norm += amp;
        x *= 2.03;
        y *= 2.03;
        amp *= 0.5;
    }
    if norm > 0.0 {
        sum / norm
    } else {
        0.0
    }
}

fn noise2(x: f32, y: f32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x.fract();
    let yf = y.fract();
    let u = xf * xf * (3.0 - 2.0 * xf);
    let v = yf * yf * (3.0 - 2.0 * yf);

    let a = hash2(xi, yi, 1);
    let b = hash2(xi + 1, yi, 1);
    let c = hash2(xi, yi + 1, 1);
    let d = hash2(xi + 1, yi + 1, 1);

    let ab = a + (b - a) * u;
    let cd = c + (d - c) * u;
    ab + (cd - ab) * v
}

fn hash2(x: i32, y: i32, seed: i32) -> f32 {
    let mut n =
        x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed.wrapping_mul(362_437);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) as u32 & 0xffff) as f32 / 65535.0
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn tint(base: [f32; 3], factor: f32) -> [f32; 3] {
    [base[0] * factor, base[1] * factor, base[2] * factor]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
