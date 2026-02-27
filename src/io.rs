use crate::model::{EffectKind, MapDocument, MaterialKind};
use anyhow::{Context, Result};
use image::{ImageBuffer, Rgba};
use std::{fs, path::Path};

const WALL_BLOCK_COLS: f32 = 3.0;
const WALL_BLOCK_ROWS: f32 = 2.0;
const WALL_SEAM_WIDTH: f32 = 0.06;
const WALL_FRAME_THICKNESS: f32 = 0.10;
const WALL_DOOR_WIDTH: f32 = 0.46;
const WALL_DOOR_HEIGHT: f32 = 0.78;
const WALL_WINDOW_WIDTH: f32 = 0.56;
const WALL_WINDOW_HEIGHT: f32 = 0.42;
const WALL_MULLION_THICKNESS: f32 = 0.05;

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
    let base_tile = [world[0].floor() as i32, world[1].floor() as i32];
    let local = [world[0].fract(), world[1].fract()];
    let mut mat = doc
        .terrain_at_i32_checked(base_tile[0], base_tile[1])
        .unwrap_or(MaterialKind::Dirt);
    let mut eff = doc.effect_at_i32(base_tile[0], base_tile[1]);
    let blend = 0.2;

    if mat.blends() {
        let l = doc.terrain_at_i32_checked(base_tile[0] - 1, base_tile[1]);
        let r = doc.terrain_at_i32_checked(base_tile[0] + 1, base_tile[1]);
        let t = doc.terrain_at_i32_checked(base_tile[0], base_tile[1] - 1);
        let b = doc.terrain_at_i32_checked(base_tile[0], base_tile[1] + 1);
        let wl = edge_weight(local[0], blend);
        let wr = edge_weight(1.0 - local[0], blend);
        let wt = edge_weight(local[1], blend);
        let wb = edge_weight(1.0 - local[1], blend);

        let mut wsum = 1.0_f32;
        let mut color_sum = material_rgb(mat, world);
        let mut effect_sum = effect_overlay_cpu(eff, world);
        if let Some(l_mat) = l.filter(|&other| mat.blend_compatible(other)) {
            color_sum = add_weighted(color_sum, material_rgb(l_mat, world), wl);
            effect_sum = add_weighted(
                effect_sum,
                effect_overlay_cpu(doc.effect_at_i32(base_tile[0] - 1, base_tile[1]), world),
                wl,
            );
            wsum += wl;
        }
        if let Some(r_mat) = r.filter(|&other| mat.blend_compatible(other)) {
            color_sum = add_weighted(color_sum, material_rgb(r_mat, world), wr);
            effect_sum = add_weighted(
                effect_sum,
                effect_overlay_cpu(doc.effect_at_i32(base_tile[0] + 1, base_tile[1]), world),
                wr,
            );
            wsum += wr;
        }
        if let Some(t_mat) = t.filter(|&other| mat.blend_compatible(other)) {
            color_sum = add_weighted(color_sum, material_rgb(t_mat, world), wt);
            effect_sum = add_weighted(
                effect_sum,
                effect_overlay_cpu(doc.effect_at_i32(base_tile[0], base_tile[1] - 1), world),
                wt,
            );
            wsum += wt;
        }
        if let Some(b_mat) = b.filter(|&other| mat.blend_compatible(other)) {
            color_sum = add_weighted(color_sum, material_rgb(b_mat, world), wb);
            effect_sum = add_weighted(
                effect_sum,
                effect_overlay_cpu(doc.effect_at_i32(base_tile[0], base_tile[1] + 1), world),
                wb,
            );
            wsum += wb;
        }

        let mut c = [
            (color_sum[0] + effect_sum[0]) / wsum.max(0.0001),
            (color_sum[1] + effect_sum[1]) / wsum.max(0.0001),
            (color_sum[2] + effect_sum[2]) / wsum.max(0.0001),
        ];

        if crate::blend_rules::material_diagonal(mat) {
            let corner = blend * crate::blend_rules::DIAGONAL_CORNER_SCALE;

            if local[0] + local[1] < corner {
                let nw = doc.terrain_at_i32_checked(base_tile[0] - 1, base_tile[1] - 1);
                if let (Some(l_mat), Some(t_mat), Some(nw_mat)) = (l, t, nw) {
                    if l_mat == t_mat && nw_mat == l_mat && mat.blend_compatible(l_mat) {
                        mat = nw_mat;
                        eff = doc.effect_at_i32(base_tile[0] - 1, base_tile[1] - 1);
                        c = apply_effect_cpu(material_rgb(mat, world), eff, world);
                    }
                }
            }
            if (1.0 - local[0]) + local[1] < corner {
                let ne = doc.terrain_at_i32_checked(base_tile[0] + 1, base_tile[1] - 1);
                if let (Some(r_mat), Some(t_mat), Some(ne_mat)) = (r, t, ne) {
                    if r_mat == t_mat && ne_mat == r_mat && mat.blend_compatible(r_mat) {
                        mat = ne_mat;
                        eff = doc.effect_at_i32(base_tile[0] + 1, base_tile[1] - 1);
                        c = apply_effect_cpu(material_rgb(mat, world), eff, world);
                    }
                }
            }
            if local[0] + (1.0 - local[1]) < corner {
                let sw = doc.terrain_at_i32_checked(base_tile[0] - 1, base_tile[1] + 1);
                if let (Some(l_mat), Some(b_mat), Some(sw_mat)) = (l, b, sw) {
                    if l_mat == b_mat && sw_mat == l_mat && mat.blend_compatible(l_mat) {
                        mat = sw_mat;
                        eff = doc.effect_at_i32(base_tile[0] - 1, base_tile[1] + 1);
                        c = apply_effect_cpu(material_rgb(mat, world), eff, world);
                    }
                }
            }
            if (1.0 - local[0]) + (1.0 - local[1]) < corner {
                let se = doc.terrain_at_i32_checked(base_tile[0] + 1, base_tile[1] + 1);
                if let (Some(r_mat), Some(b_mat), Some(se_mat)) = (r, b, se) {
                    if r_mat == b_mat && se_mat == r_mat && mat.blend_compatible(r_mat) {
                        mat = se_mat;
                        eff = doc.effect_at_i32(base_tile[0] + 1, base_tile[1] + 1);
                        c = apply_effect_cpu(material_rgb(mat, world), eff, world);
                    }
                }
            }
        }

        return [
            c[0].clamp(0.0, 255.0) as u8,
            c[1].clamp(0.0, 255.0) as u8,
            c[2].clamp(0.0, 255.0) as u8,
        ];
    }

    let c = add3(material_rgb(mat, world), effect_overlay_cpu(eff, world));
    [
        c[0].clamp(0.0, 255.0) as u8,
        c[1].clamp(0.0, 255.0) as u8,
        c[2].clamp(0.0, 255.0) as u8,
    ]
}

fn edge_weight(distance: f32, blend: f32) -> f32 {
    1.0 - smoothstep(0.0, blend, distance)
}

fn add_weighted(base: [f32; 3], value: [f32; 3], weight: f32) -> [f32; 3] {
    [
        base[0] + value[0] * weight,
        base[1] + value[1] * weight,
        base[2] + value[2] * weight,
    ]
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn apply_effect_cpu(base: [f32; 3], effect: EffectKind, p: [f32; 2]) -> [f32; 3] {
    add3(base, effect_overlay_cpu(effect, p))
}

fn effect_overlay_cpu(effect: EffectKind, p: [f32; 2]) -> [f32; 3] {
    match effect {
        EffectKind::None => [0.0, 0.0, 0.0],
        EffectKind::Wind => {
            let gust = 0.5 + 0.5 * ((p[0] * 4.0 + p[1] * 1.3).sin() * (p[1] * 3.0).cos());
            [0.0, 10.0 * gust, 0.0]
        }
        EffectKind::Rain => {
            let streak = ((p[0] * 12.0 + p[1] * 28.0).fract() * 2.0 - 1.0).abs();
            let mask = (1.0 - smoothstep(0.65, 0.95, streak)) * 18.0;
            [mask, mask, mask * 1.4]
        }
    }
}

fn material_rgb(mat: MaterialKind, p: [f32; 2]) -> [f32; 3] {
    let tile_local = [p[0].fract(), p[1].fract()];
    let block_grid = [
        tile_local[0] * WALL_BLOCK_COLS,
        tile_local[1] * WALL_BLOCK_ROWS,
    ];
    let block_id = [block_grid[0].floor(), block_grid[1].floor()];
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
            let seam = wall_block_mask(tile_local);
            let block_seed = hash2(
                (p[0].floor() * 11.0 + block_id[0] * 3.0) as i32,
                (p[1].floor() * 11.0 + block_id[1] * 3.0) as i32,
                33,
            );
            mix(
                tint(
                    [136.0, 143.0, 150.0],
                    0.82 + n1 * 0.10 + (block_seed - 0.5) * 0.10,
                ),
                [95.0, 99.0, 102.0],
                seam * 0.95,
            )
        }
        MaterialKind::WallDoor => {
            let seam = wall_block_mask(tile_local);
            let stone = mix(
                tint([135.0, 143.0, 153.0], 0.82 + n1 * 0.09),
                [92.0, 95.0, 99.0],
                seam * 0.9,
            );

            let opening_min = [
                0.5 - WALL_DOOR_WIDTH * 0.5,
                1.0 - WALL_DOOR_HEIGHT + WALL_FRAME_THICKNESS,
            ];
            let opening_max = [0.5 + WALL_DOOR_WIDTH * 0.5, 1.0 - WALL_FRAME_THICKNESS];
            let frame_min = [
                opening_min[0] - WALL_FRAME_THICKNESS,
                opening_min[1] - WALL_FRAME_THICKNESS,
            ];
            let frame_max = [opening_max[0] + WALL_FRAME_THICKNESS, 1.0];

            let opening = rect_mask(tile_local, opening_min, opening_max);
            let frame = (rect_mask(tile_local, frame_min, frame_max) - opening).max(0.0);
            let with_frame = mix(stone, [160.0, 167.0, 173.0], frame);
            mix(
                with_frame,
                tint([64.0, 38.0, 21.0], 0.78 + n2 * 0.12),
                opening,
            )
        }
        MaterialKind::WallWindow => {
            let seam = wall_block_mask(tile_local);
            let wall = mix(
                tint([135.0, 143.0, 153.0], 0.83 + n1 * 0.09),
                [92.0, 95.0, 99.0],
                seam * 0.9,
            );

            let outer_min = [
                0.5 - WALL_WINDOW_WIDTH * 0.5,
                0.36 - WALL_WINDOW_HEIGHT * 0.5,
            ];
            let outer_max = [
                0.5 + WALL_WINDOW_WIDTH * 0.5,
                0.36 + WALL_WINDOW_HEIGHT * 0.5,
            ];
            let inner_min = [
                outer_min[0] + WALL_FRAME_THICKNESS,
                outer_min[1] + WALL_FRAME_THICKNESS,
            ];
            let inner_max = [
                outer_max[0] - WALL_FRAME_THICKNESS,
                outer_max[1] - WALL_FRAME_THICKNESS,
            ];
            let inner_size = [inner_max[0] - inner_min[0], inner_max[1] - inner_min[1]];

            let window_outer = rect_mask(tile_local, outer_min, outer_max);
            let glass_area = rect_mask(tile_local, inner_min, inner_max);
            let frame = (window_outer - glass_area).max(0.0);

            let v_mid = inner_min[0] + inner_size[0] * 0.5;
            let h_mid = inner_min[1] + inner_size[1] * 0.5;
            let v_bar = rect_mask(
                tile_local,
                [v_mid - WALL_MULLION_THICKNESS, inner_min[1]],
                [v_mid + WALL_MULLION_THICKNESS, inner_max[1]],
            );
            let h_bar = rect_mask(
                tile_local,
                [inner_min[0], h_mid - WALL_MULLION_THICKNESS],
                [inner_max[0], h_mid + WALL_MULLION_THICKNESS],
            );
            let mullion = v_bar.max(h_bar) * glass_area;

            let pane_seed = hash2(
                (p[0].floor() * 7.0 + block_id[0]) as i32,
                (p[1].floor() * 7.0 + block_id[1]) as i32,
                71,
            );
            let glass = tint([84.0, 128.0, 168.0], 0.84 + pane_seed * 0.16);
            let highlight = rect_mask(
                tile_local,
                [inner_min[0] + 0.03, inner_min[1] + 0.03],
                [
                    inner_min[0] + inner_size[0] * 0.35,
                    inner_min[1] + inner_size[1] * 0.26,
                ],
            ) * glass_area;

            let mut color = mix(wall, [162.0, 168.0, 174.0], frame);
            color = mix(color, glass, glass_area);
            color = mix(color, [156.0, 162.0, 168.0], mullion);
            [
                color[0] + 30.0 * highlight,
                color[1] + 36.0 * highlight,
                color[2] + 42.0 * highlight,
            ]
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

fn rect_mask(p: [f32; 2], min_p: [f32; 2], max_p: [f32; 2]) -> f32 {
    if p[0] >= min_p[0] && p[0] <= max_p[0] && p[1] >= min_p[1] && p[1] <= max_p[1] {
        1.0
    } else {
        0.0
    }
}

fn wall_block_mask(tile_local: [f32; 2]) -> f32 {
    let fx = (tile_local[0] * WALL_BLOCK_COLS).fract();
    let fy = (tile_local[1] * WALL_BLOCK_ROWS).fract();
    let gx = 1.0 - smoothstep(0.0, WALL_SEAM_WIDTH, fx.min(1.0 - fx));
    let gy = 1.0 - smoothstep(0.0, WALL_SEAM_WIDTH, fy.min(1.0 - fy));
    gx.max(gy)
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

#[cfg(test)]
mod tests {
    use super::shaded_world;
    use crate::model::{EffectKind, MapDocument, MaterialKind};

    fn doc_from_rows(rows: &[[MaterialKind; 3]; 3]) -> MapDocument {
        let mut doc = MapDocument::new(3, 3);
        doc.tile_px = 32;
        for (y, row) in rows.iter().enumerate() {
            for (x, material) in row.iter().enumerate() {
                let idx = doc.idx(x as u32, y as u32);
                doc.terrain[idx] = *material;
                doc.effects[idx] = EffectKind::None;
            }
        }
        doc
    }

    fn checksum(doc: &MapDocument) -> u64 {
        let mut acc = 0u64;
        for py in 0..(doc.height * doc.tile_px) {
            for px in 0..(doc.width * doc.tile_px) {
                let world = [
                    px as f32 / doc.tile_px as f32,
                    py as f32 / doc.tile_px as f32,
                ];
                let c = shaded_world(doc, world);
                acc = acc
                    .wrapping_mul(1_099_511_628_211)
                    .wrapping_add(u64::from(c[0]))
                    .wrapping_add(u64::from(c[1]) << 8)
                    .wrapping_add(u64::from(c[2]) << 16);
            }
        }
        acc
    }
    #[test]
    fn diagonal_corner_fixtures_match_snapshot() {
        let path = MaterialKind::Path;
        let dirt = MaterialKind::Dirt;

        let nw_doc = doc_from_rows(&[[dirt, dirt, path], [dirt, path, path], [path, path, path]]);
        assert_eq!(shaded_world(&nw_doc, [1.02, 1.02]), [90, 68, 47]);

        let ne_doc = doc_from_rows(&[[path, dirt, dirt], [path, path, dirt], [path, path, path]]);
        assert_eq!(shaded_world(&ne_doc, [1.98, 1.02]), [88, 66, 46]);

        let sw_doc = doc_from_rows(&[[path, path, path], [dirt, path, path], [dirt, dirt, path]]);
        assert_eq!(shaded_world(&sw_doc, [1.02, 1.98]), [87, 66, 45]);

        let se_doc = doc_from_rows(&[[path, path, path], [path, path, dirt], [path, dirt, dirt]]);
        assert_eq!(shaded_world(&se_doc, [1.98, 1.98]), [90, 68, 47]);
    }

    #[test]
    fn export_parity_checksum_fixture() {
        let g = MaterialKind::Grass;
        let p = MaterialKind::Path;
        let w = MaterialKind::Wall;
        let s = MaterialKind::Sand;
        let doc = doc_from_rows(&[[g, p, g], [w, p, s], [g, w, g]]);
        assert_eq!(checksum(&doc), 17_084_880_885_835_417_417);
    }
}
