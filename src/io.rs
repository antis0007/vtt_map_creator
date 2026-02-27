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
    let mat = doc.terrain_at_i32(tile[0], tile[1]);
    let eff = doc.effect_at_i32(tile[0], tile[1]);
    let blend = 0.2;

    let mut c = material_rgb(mat, world);
    c = apply_effect_cpu(c, eff, world);

    if mat.blends() {
        let l = doc.terrain_at_i32(tile[0] - 1, tile[1]);
        let r = doc.terrain_at_i32(tile[0] + 1, tile[1]);
        let t = doc.terrain_at_i32(tile[0], tile[1] - 1);
        let b = doc.terrain_at_i32(tile[0], tile[1] + 1);
        let nw = doc.terrain_at_i32(tile[0] - 1, tile[1] - 1);
        let ne = doc.terrain_at_i32(tile[0] + 1, tile[1] - 1);
        let sw = doc.terrain_at_i32(tile[0] - 1, tile[1] + 1);
        let se = doc.terrain_at_i32(tile[0] + 1, tile[1] + 1);
        let wl = edge_weight(local[0], blend);
        let wr = edge_weight(1.0 - local[0], blend);
        let wt = edge_weight(local[1], blend);
        let wb = edge_weight(1.0 - local[1], blend);
        let wnw = corner_weight(local[0], local[1], blend);
        let wne = corner_weight(1.0 - local[0], local[1], blend);
        let wsw = corner_weight(local[0], 1.0 - local[1], blend);
        let wse = corner_weight(1.0 - local[0], 1.0 - local[1], blend);

        let mut sum = c;
        let mut wsum = 1.0_f32;
        let lw = if l != mat { wl } else { 0.0 };
        let rw = if r != mat { wr } else { 0.0 };
        let tw = if t != mat { wt } else { 0.0 };
        let bw = if b != mat { wb } else { 0.0 };
        let nww = if nw != mat { wnw } else { 0.0 };
        let neww = if ne != mat { wne } else { 0.0 };
        let sww = if sw != mat { wsw } else { 0.0 };
        let sew = if se != mat { wse } else { 0.0 };

        let lc = apply_effect_cpu(
            material_rgb(l, world),
            doc.effect_at_i32(tile[0] - 1, tile[1]),
            world,
        );
        let rc = apply_effect_cpu(
            material_rgb(r, world),
            doc.effect_at_i32(tile[0] + 1, tile[1]),
            world,
        );
        let tc = apply_effect_cpu(
            material_rgb(t, world),
            doc.effect_at_i32(tile[0], tile[1] - 1),
            world,
        );
        let bc = apply_effect_cpu(
            material_rgb(b, world),
            doc.effect_at_i32(tile[0], tile[1] + 1),
            world,
        );
        let nwc = apply_effect_cpu(
            material_rgb(nw, world),
            doc.effect_at_i32(tile[0] - 1, tile[1] - 1),
            world,
        );
        let nec = apply_effect_cpu(
            material_rgb(ne, world),
            doc.effect_at_i32(tile[0] + 1, tile[1] - 1),
            world,
        );
        let swc = apply_effect_cpu(
            material_rgb(sw, world),
            doc.effect_at_i32(tile[0] - 1, tile[1] + 1),
            world,
        );
        let sec = apply_effect_cpu(
            material_rgb(se, world),
            doc.effect_at_i32(tile[0] + 1, tile[1] + 1),
            world,
        );

        sum[0] += lc[0] * lw + rc[0] * rw + tc[0] * tw + bc[0] * bw;
        sum[1] += lc[1] * lw + rc[1] * rw + tc[1] * tw + bc[1] * bw;
        sum[2] += lc[2] * lw + rc[2] * rw + tc[2] * tw + bc[2] * bw;
        sum[0] += nwc[0] * nww + nec[0] * neww + swc[0] * sww + sec[0] * sew;
        sum[1] += nwc[1] * nww + nec[1] * neww + swc[1] * sww + sec[1] * sew;
        sum[2] += nwc[2] * nww + nec[2] * neww + swc[2] * sww + sec[2] * sew;
        wsum += lw + rw + tw + bw + nww + neww + sww + sew;

        c = [sum[0] / wsum, sum[1] / wsum, sum[2] / wsum];

        if mat.diagonal_blend() {
            let corner = blend * 1.35;
            let aa = 0.02;
            let nw_mask = if l == t && l != mat && nw == l {
                1.0 - smoothstep(-aa, aa, local[0] + local[1] - corner)
            } else {
                0.0
            };
            let ne_mask = if r == t && r != mat && ne == r {
                1.0 - smoothstep(-aa, aa, (1.0 - local[0]) + local[1] - corner)
            } else {
                0.0
            };
            let sw_mask = if l == b && l != mat && sw == l {
                1.0 - smoothstep(-aa, aa, local[0] + (1.0 - local[1]) - corner)
            } else {
                0.0
            };
            let se_mask = if r == b && r != mat && se == r {
                1.0 - smoothstep(-aa, aa, (1.0 - local[0]) + (1.0 - local[1]) - corner)
            } else {
                0.0
            };

            c = mix(
                c,
                apply_effect_cpu(
                    material_rgb(l, world),
                    doc.effect_at_i32(tile[0] - 1, tile[1] - 1),
                    world,
                ),
                nw_mask,
            );
            c = mix(
                c,
                apply_effect_cpu(
                    material_rgb(r, world),
                    doc.effect_at_i32(tile[0] + 1, tile[1] - 1),
                    world,
                ),
                ne_mask,
            );
            c = mix(
                c,
                apply_effect_cpu(
                    material_rgb(l, world),
                    doc.effect_at_i32(tile[0] - 1, tile[1] + 1),
                    world,
                ),
                sw_mask,
            );
            c = mix(
                c,
                apply_effect_cpu(
                    material_rgb(r, world),
                    doc.effect_at_i32(tile[0] + 1, tile[1] + 1),
                    world,
                ),
                se_mask,
            );
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

fn corner_weight(dx: f32, dy: f32, blend: f32) -> f32 {
    let radius = (blend * std::f32::consts::SQRT_2).max(0.0001);
    1.0 - smoothstep(0.0, radius, (dx * dx + dy * dy).sqrt())
}

fn apply_effect_cpu(mut c: [f32; 3], effect: EffectKind, p: [f32; 2]) -> [f32; 3] {
    match effect {
        EffectKind::None => c,
        EffectKind::Wind => {
            let gust =
                0.5 + 0.5 * (p[0] * 4.0 + p[1] * 1.5 + fbm(p[0] * 1.5, p[1] * 1.5, 5) * 4.0).sin();
            c[0] += 7.65 * gust;
            c[1] += 20.4 * gust;
            c[2] += 5.1 * gust;
            c
        }
        EffectKind::Rain => {
            let streak = ((p[0] * 14.0 + p[1] * 32.0).fract() * 2.0 - 1.0).abs();
            let band = 1.0 - smoothstep(0.72, 0.96, streak);
            c[0] += 20.4 * band;
            c[1] += 22.95 * band;
            c[2] += 33.15 * band;
            c
        }
    }
}

fn material_rgb(mat: MaterialKind, p: [f32; 2]) -> [f32; 3] {
    let n1 = fbm(p[0] * 2.6, p[1] * 2.6, 5);
    let n2 = fbm(p[0] * 6.2 + 17.0, p[1] * 6.2 - 9.0, 5);
    match mat {
        MaterialKind::Dirt => tint([102.0, 76.5, 53.55], 0.76 + n1 * 0.18 + n2 * 0.06),
        MaterialKind::Grass => {
            let bands = 0.5 + 0.5 * (p[0] * 14.0 + fbm(p[0] * 2.5, p[1] * 2.5, 5) * 3.0).sin();
            let blades = 0.5 + 0.5 * (p[0] * 16.0 + p[1] * 3.0 + n2 * 5.0).sin();
            let mut c = tint(
                [76.5, 127.5, 58.65],
                0.72 + n1 * 0.12 + bands * 0.10 + blades * 0.08,
            );
            c[0] += 2.55;
            c[1] += 7.65;
            c
        }
        MaterialKind::Sand => {
            let ripples = 0.5 + 0.5 * (p[0] * 11.0 + p[1] * 1.2 + n1 * 4.0).sin();
            tint([198.9, 178.5, 122.4], 0.82 + n2 * 0.08 + ripples * 0.08)
        }
        MaterialKind::Water => {
            let wave =
                0.5 + 0.5 * ((p[0] + p[1]) * 2.4 + fbm(p[0] * 0.7, p[1] * 0.7, 5) * 4.0).sin();
            let caustic = 0.5 + 0.5 * (p[0] * 18.0 - p[1] * 10.0 + n2 * 8.0).sin();
            let mut c = tint([25.5, 81.6, 132.6], 0.76 + wave * 0.28);
            c[0] += 7.65 * caustic;
            c[1] += 22.95 * caustic;
            c[2] += 33.15 * caustic;
            c
        }
        MaterialKind::Lava => {
            let molten = 0.5 + 0.5 * (p[0] * 9.0 + n1 * 8.0).sin() * (p[1] * 7.0 - n2 * 6.0).cos();
            let mut c = tint([147.9, 40.8, 10.2], 0.74 + molten * 0.30);
            c[0] += 56.1 * molten;
            c[1] += 25.5 * molten;
            c
        }
        MaterialKind::Gravel => {
            let grain = hash2((p[0] * 7.0).floor() as i32, (p[1] * 7.0).floor() as i32, 9);
            let c = tint([122.4, 122.4, 117.3], 0.82 + n1 * 0.08);
            [
                c[0] + grain * 20.4,
                c[1] + grain * 20.4,
                c[2] + grain * 20.4,
            ]
        }
        MaterialKind::Brick => {
            let seam = brick_topdown_mask([p[0] + n1 * 0.02, p[1] + n2 * 0.02]);
            mix(
                tint([124.95, 56.1, 40.8], 0.86 + n1 * 0.12),
                [181.05, 168.3, 153.0],
                seam * 0.9,
            )
        }
        MaterialKind::Path => {
            let chips = hash2((p[0] * 5.0).floor() as i32, (p[1] * 5.0).floor() as i32, 21);
            tint([127.5, 109.65, 84.15], 0.86 + n1 * 0.08 + chips * 0.08)
        }
        MaterialKind::Wall => {
            let seam = stone_block_mask([p[0] + n1 * 0.04, p[1] + n2 * 0.04]);
            mix(
                tint([137.7, 145.35, 153.0], 0.84 + n1 * 0.10),
                [94.35, 96.9, 99.45],
                seam * 0.95,
            )
        }
        MaterialKind::WallDoor => {
            let seam = stone_block_mask(p);
            mix(
                tint([112.2, 76.5, 43.35], 0.9 + n1 * 0.08),
                [147.9, 153.0, 158.1],
                seam * 0.8,
            )
        }
        MaterialKind::WallWindow => {
            let seam = stone_block_mask(p);
            mix(
                tint([91.8, 127.5, 160.65], 0.9 + n1 * 0.10),
                [158.1, 165.75, 170.85],
                seam * 0.85,
            )
        }
        MaterialKind::FloorWood => {
            let seam = plank_mask([p[0] + n2 * 0.03, p[1]]);
            mix(
                tint([145.35, 102.0, 63.75], 0.86 + n1 * 0.12),
                [76.5, 53.55, 33.15],
                seam * 0.8,
            )
        }
    }
}

fn plank_mask(p: [f32; 2]) -> f32 {
    let x = (p[0] * 3.5).fract();
    1.0 - smoothstep(0.0, 0.06, x.min(1.0 - x))
}

fn stone_block_mask(p: [f32; 2]) -> f32 {
    let gx = (p[0] * 2.0).fract();
    let gy = (p[1] * 1.4).fract();
    let seam_x = 1.0 - smoothstep(0.0, 0.07, gx.min(1.0 - gx));
    let seam_y = 1.0 - smoothstep(0.0, 0.07, gy.min(1.0 - gy));
    seam_x.max(seam_y)
}

fn brick_topdown_mask(p: [f32; 2]) -> f32 {
    let lx = (p[0] * 2.0).fract();
    let ly = (p[1] * 2.0).fract();
    let seam_x = 1.0 - smoothstep(0.0, 0.06, lx.min(1.0 - lx));
    let seam_y = 1.0 - smoothstep(0.0, 0.06, ly.min(1.0 - ly));
    seam_x.max(seam_y)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MaterialKind;

    fn render_hash(doc: &MapDocument) -> u64 {
        let w = doc.width * doc.tile_px;
        let h = doc.height * doc.tile_px;
        let mut hash = 1469598103934665603u64;
        for py in 0..h {
            for px in 0..w {
                let world = [
                    px as f32 / doc.tile_px as f32,
                    py as f32 / doc.tile_px as f32,
                ];
                let rgb = shaded_world(doc, world);
                for b in rgb {
                    hash ^= b as u64;
                    hash = hash.wrapping_mul(1099511628211);
                }
            }
        }
        hash
    }

    fn set(doc: &mut MapDocument, x: u32, y: u32, mat: MaterialKind) {
        let idx = doc.idx(x, y);
        doc.terrain[idx] = mat;
    }

    fn scene_l_shape() -> MapDocument {
        let mut doc = MapDocument::new(6, 6);
        doc.tile_px = 16;
        for y in 1..5 {
            set(&mut doc, 1, y, MaterialKind::Path);
        }
        for x in 1..5 {
            set(&mut doc, x, 4, MaterialKind::Path);
        }
        doc
    }

    fn scene_checker_corners() -> MapDocument {
        let mut doc = MapDocument::new(6, 6);
        doc.tile_px = 16;
        for y in 1..5 {
            for x in 1..5 {
                if (x + y) % 2 == 0 {
                    set(&mut doc, x, y, MaterialKind::Wall);
                }
            }
        }
        doc
    }

    fn scene_t_junction() -> MapDocument {
        let mut doc = MapDocument::new(7, 7);
        doc.tile_px = 16;
        for x in 1..6 {
            set(&mut doc, x, 2, MaterialKind::Path);
        }
        for y in 2..6 {
            set(&mut doc, 3, y, MaterialKind::Path);
        }
        doc
    }

    #[test]
    fn visual_regression_scene_hashes() {
        let l_shape = render_hash(&scene_l_shape());
        let checker = render_hash(&scene_checker_corners());
        let t_junction = render_hash(&scene_t_junction());

        assert_eq!(
            (l_shape, checker, t_junction),
            (
                8947265069355207326,
                136982776539104141,
                11580239904059965943
            ),
            "visual regression hash mismatch"
        );
    }
}
