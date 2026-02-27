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
            let tile = [world[0].floor() as i32, world[1].floor() as i32];
            let color = shaded_world(doc, world, tile);
            img.put_pixel(px, py, Rgba([color[0], color[1], color[2], 255]));
        }
    }

    img.save(path.as_ref())
        .with_context(|| format!("failed to save {}", path.as_ref().display()))?;
    Ok(())
}

fn shaded_world(doc: &MapDocument, world: [f32; 2], tile: [i32; 2]) -> [u8; 3] {
    let mut sum = [0.0; 3];
    let mut wsum = 0.0;

    for oy in -1..=1 {
        for ox in -1..=1 {
            let sx = tile[0] + ox;
            let sy = tile[1] + oy;
            let mat = doc.terrain_at_i32(sx, sy);
            let eff = doc.effect_at_i32(sx, sy);
            let center = [
                sx as f32 + 0.5 + (hash2(sx, sy, 17) - 0.5) * 0.18,
                sy as f32 + 0.5 + (hash2(sx, sy, 29) - 0.5) * 0.18,
            ];
            let dx = world[0] - center[0];
            let dy = world[1] - center[1];
            let weight = (-3.6 * (dx * dx + dy * dy)).exp();
            let mut c = material_rgb(mat, world);
            c = apply_effect_cpu(c, eff, world);
            sum[0] += c[0] * weight;
            sum[1] += c[1] * weight;
            sum[2] += c[2] * weight;
            wsum += weight;
        }
    }

    let inv = if wsum > 0.0 { 1.0 / wsum } else { 1.0 };
    [
        (sum[0] * inv).clamp(0.0, 255.0) as u8,
        (sum[1] * inv).clamp(0.0, 255.0) as u8,
        (sum[2] * inv).clamp(0.0, 255.0) as u8,
    ]
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
        MaterialKind::Dirt => {
            let base = [103.0, 78.0, 54.0];
            tint(base, 0.75 + n1 * 0.18 + n2 * 0.07)
        }
        MaterialKind::Grass => {
            let blade = (p[0] * 11.0 + p[1] * 2.0 + n2 * 2.0).sin().abs();
            let base = [79.0, 129.0, 60.0];
            tint(base, 0.78 + n1 * 0.14 + blade * 0.10)
        }
        MaterialKind::Sand => {
            let ripple = 0.5 + 0.5 * (p[0] * 9.0 + p[1] * 1.1 + n1 * 2.0).sin();
            let base = [194.0, 176.0, 121.0];
            tint(base, 0.82 + n1 * 0.10 + ripple * 0.08)
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
            [c[0] + pebble * 18.0, c[1] + pebble * 18.0, c[2] + pebble * 18.0]
        }
        MaterialKind::Brick => {
            let mortar = brick_mortar(p);
            let brick = tint([145.0, 71.0, 52.0], 0.78 + n1 * 0.10);
            mix(brick, [191.0, 183.0, 172.0], mortar)
        }
        MaterialKind::Path => {
            let chip = (hash2((p[0] * 4.0) as i32, (p[1] * 4.0) as i32, 21) * 0.25) + n1 * 0.10;
            tint([126.0, 112.0, 91.0], 0.82 + chip)
        }
    }
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
    if norm > 0.0 { sum / norm } else { 0.0 }
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
    let mut n = x.wrapping_mul(374_761_393)
        ^ y.wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(362_437);
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

fn brick_mortar(p: [f32; 2]) -> f32 {
    let row = p[1].floor();
    let offset = if (row as i32) & 1 == 0 { 0.0 } else { 0.5 };
    let local_x = (p[0] + offset).fract();
    let local_y = p[1].fract();
    let mortar = (local_x.min(1.0 - local_x) < 0.06) || (local_y.min(1.0 - local_y) < 0.08);
    if mortar { 1.0 } else { 0.0 }
}
