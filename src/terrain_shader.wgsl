struct Globals {
    map_size: vec2<u32>,
    _pad0: vec2<u32>,
    view_origin: vec2<f32>,
    view_size: vec2<f32>,
    time: f32,
    blend_strength: f32,
    grid_opacity: f32,
    _pad1: f32,
};

@group(0) @binding(0)
var<uniform> g: Globals;

@group(0) @binding(1)
var<storage, read> tiles: array<u32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

fn unpack_material(packed: u32) -> u32 {
    return packed & 0xffu;
}

fn unpack_effect(packed: u32) -> u32 {
    return (packed >> 8u) & 0xffu;
}

fn material_blends(material: u32) -> bool {
    return material <= 5u || material == 7u || material == 8u;
}

fn material_diagonal(material: u32) -> bool {
    return material == 7u || material == 8u;
}

fn clamp_tile(p: vec2<i32>) -> vec2<u32> {
    let max_xy = vec2<i32>(vec2<u32>(max(g.map_size, vec2<u32>(1u))) - vec2<u32>(1u));
    return vec2<u32>(clamp(p, vec2<i32>(0, 0), max_xy));
}

fn tile_at(p: vec2<i32>) -> u32 {
    let c = clamp_tile(p);
    let idx = c.y * g.map_size.x + c.x;
    return tiles[idx];
}

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453123);
}

fn noise2(x: vec2<f32>) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (vec2<f32>(3.0, 3.0) - 2.0 * f);

    let a = hash21(i + vec2<f32>(0.0, 0.0));
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));

    let ab = mix(a, b, u.x);
    let cd = mix(c, d, u.x);
    return mix(ab, cd, u.y);
}

fn fbm(p_in: vec2<f32>) -> f32 {
    var p = p_in;
    var amp = 0.5;
    var sum = 0.0;
    var norm = 0.0;
    for (var i = 0; i < 5; i = i + 1) {
        sum = sum + noise2(p) * amp;
        norm = norm + amp;
        p = p * 2.03;
        amp = amp * 0.5;
    }
    return select(0.0, sum / norm, norm > 0.0);
}

fn plank_mask(p: vec2<f32>) -> f32 {
    let x = fract(p.x * 3.5);
    let seam = 1.0 - smoothstep(0.0, 0.06, min(x, 1.0 - x));
    return seam;
}

fn stone_block_mask(p: vec2<f32>) -> f32 {
    let grid = fract(p * vec2<f32>(2.0, 1.4));
    let seam_x = 1.0 - smoothstep(0.0, 0.07, min(grid.x, 1.0 - grid.x));
    let seam_y = 1.0 - smoothstep(0.0, 0.07, min(grid.y, 1.0 - grid.y));
    return max(seam_x, seam_y);
}

fn brick_topdown_mask(p: vec2<f32>) -> f32 {
    let cell = p * vec2<f32>(2.0, 2.0);
    let local = fract(cell);
    let seam = max(
        1.0 - smoothstep(0.0, 0.06, min(local.x, 1.0 - local.x)),
        1.0 - smoothstep(0.0, 0.06, min(local.y, 1.0 - local.y)),
    );
    return seam;
}

fn material_color(material: u32, world: vec2<f32>) -> vec3<f32> {
    let n1 = fbm(world * 2.6);
    let n2 = fbm(world * 6.2 + vec2<f32>(17.0, -9.0));

    if (material == 0u) {
        let tint = 0.76 + n1 * 0.18 + n2 * 0.06;
        return vec3<f32>(0.40, 0.30, 0.21) * tint;
    }
    if (material == 1u) {
        let bands = 0.5 + 0.5 * sin(world.x * 14.0 + fbm(world * 2.5) * 3.0 + g.time * 1.4);
        let blades = 0.5 + 0.5 * sin(world.x * 16.0 + world.y * 3.0 + n2 * 5.0 + g.time * 1.1);
        let tint = 0.72 + n1 * 0.12 + bands * 0.10 + blades * 0.08;
        return vec3<f32>(0.30, 0.50, 0.23) * tint + vec3<f32>(0.01, 0.03, 0.00);
    }
    if (material == 2u) {
        let ripples = 0.5 + 0.5 * sin(world.x * 11.0 + world.y * 1.2 + n1 * 4.0);
        let tint = 0.82 + n2 * 0.08 + ripples * 0.08;
        return vec3<f32>(0.78, 0.70, 0.48) * tint;
    }
    if (material == 3u) {
        let wave = 0.5 + 0.5 * sin((world.x + world.y) * 2.4 + fbm(world * 0.7) * 4.0 - g.time * 1.8);
        let caustic = 0.5 + 0.5 * sin(world.x * 18.0 - world.y * 10.0 + g.time * 2.0 + n2 * 8.0);
        let base = vec3<f32>(0.10, 0.32, 0.52) * (0.76 + wave * 0.28);
        return base + vec3<f32>(0.03, 0.09, 0.13) * caustic;
    }
    if (material == 4u) {
        let molten = 0.5 + 0.5 * sin(world.x * 9.0 + n1 * 8.0 - g.time * 2.2) * cos(world.y * 7.0 - n2 * 6.0 + g.time * 1.8);
        let base = vec3<f32>(0.58, 0.16, 0.04) * (0.74 + molten * 0.30);
        return base + vec3<f32>(0.22, 0.10, 0.00) * molten;
    }
    if (material == 5u) {
        let grain = hash21(floor(world * 7.0));
        let tint = 0.82 + n1 * 0.08;
        return vec3<f32>(0.48, 0.48, 0.46) * tint + vec3<f32>(0.08, 0.08, 0.08) * grain;
    }
    if (material == 6u) {
        let seam = brick_topdown_mask(world + vec2<f32>(n1 * 0.02, n2 * 0.02));
        let brick = vec3<f32>(0.49, 0.22, 0.16) * (0.86 + n1 * 0.12);
        let grout = vec3<f32>(0.71, 0.66, 0.60);
        return mix(brick, grout, seam * 0.9);
    }
    if (material == 7u) {
        let chips = hash21(floor(world * 5.0));
        return vec3<f32>(0.50, 0.43, 0.33) * (0.86 + n1 * 0.08 + chips * 0.08);
    }
    if (material == 8u) {
        let seam = stone_block_mask(world + vec2<f32>(n1 * 0.04, n2 * 0.04));
        let stone = vec3<f32>(0.54, 0.57, 0.60) * (0.84 + n1 * 0.10);
        let mortar = vec3<f32>(0.37, 0.38, 0.39);
        return mix(stone, mortar, seam * 0.95);
    }
    if (material == 9u) {
        let frame = stone_block_mask(world);
        let wood = vec3<f32>(0.44, 0.30, 0.17) * (0.9 + n1 * 0.08);
        let trim = vec3<f32>(0.58, 0.60, 0.62);
        return mix(wood, trim, frame * 0.8);
    }
    if (material == 10u) {
        let frame = stone_block_mask(world);
        let glass = vec3<f32>(0.36, 0.50, 0.63) * (0.9 + n1 * 0.10);
        let trim = vec3<f32>(0.62, 0.65, 0.67);
        return mix(glass, trim, frame * 0.85);
    }

    let seam = plank_mask(world + vec2<f32>(n2 * 0.03, 0.0));
    let wood = vec3<f32>(0.57, 0.40, 0.25) * (0.86 + n1 * 0.12);
    let gap = vec3<f32>(0.30, 0.21, 0.13);
    return mix(wood, gap, seam * 0.8);
}

fn effect_overlay(effect: u32, world: vec2<f32>) -> vec3<f32> {
    if (effect == 1u) {
        let gust = 0.5 + 0.5 * sin(world.x * 4.0 + world.y * 1.5 + g.time * 1.7 + fbm(world * 1.5) * 4.0);
        return vec3<f32>(0.03, 0.08, 0.02) * gust;
    }
    if (effect == 2u) {
        let streak = abs(fract(world.x * 14.0 + world.y * 32.0 - g.time * 8.0) * 2.0 - 1.0);
        let band = 1.0 - smoothstep(0.72, 0.96, streak);
        return vec3<f32>(0.08, 0.09, 0.13) * band;
    }
    return vec3<f32>(0.0, 0.0, 0.0);
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );

    var out: VsOut;
    let p = pos[vertex_index];
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = p * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(clamp(in.uv.x, 0.0, 1.0), 1.0 - clamp(in.uv.y, 0.0, 1.0));
    let world = g.view_origin + uv * g.view_size;
    let base_tile = vec2<i32>(floor(world));
    let local = fract(world);

    let p00 = tile_at(base_tile);
    let p_w = tile_at(base_tile + vec2<i32>(-1, 0));
    let p_e = tile_at(base_tile + vec2<i32>(1, 0));
    let p_n = tile_at(base_tile + vec2<i32>(0, -1));
    let p_s = tile_at(base_tile + vec2<i32>(0, 1));
    let p_nw = tile_at(base_tile + vec2<i32>(-1, -1));
    let p_ne = tile_at(base_tile + vec2<i32>(1, -1));
    let p_sw = tile_at(base_tile + vec2<i32>(-1, 1));
    let p_se = tile_at(base_tile + vec2<i32>(1, 1));

    var mat = unpack_material(p00);
    var eff = unpack_effect(p00);

    let blend = clamp(g.blend_strength, 0.0, 0.48);
    if (blend > 0.0 && material_blends(mat)) {
        let edge_l = 1.0 - smoothstep(0.0, blend, local.x);
        let edge_r = 1.0 - smoothstep(0.0, blend, 1.0 - local.x);
        let edge_t = 1.0 - smoothstep(0.0, blend, local.y);
        let edge_b = 1.0 - smoothstep(0.0, blend, 1.0 - local.y);

        let w = unpack_material(p_w);
        let e = unpack_material(p_e);
        let n = unpack_material(p_n);
        let s = unpack_material(p_s);

        var wsum = 1.0;
        var color_sum = material_color(mat, world);
        var effect_sum = effect_overlay(eff, world);

        if (w != mat) {
            color_sum = color_sum + material_color(w, world) * edge_l;
            effect_sum = effect_sum + effect_overlay(unpack_effect(p_w), world) * edge_l;
            wsum = wsum + edge_l;
        }
        if (e != mat) {
            color_sum = color_sum + material_color(e, world) * edge_r;
            effect_sum = effect_sum + effect_overlay(unpack_effect(p_e), world) * edge_r;
            wsum = wsum + edge_r;
        }
        if (n != mat) {
            color_sum = color_sum + material_color(n, world) * edge_t;
            effect_sum = effect_sum + effect_overlay(unpack_effect(p_n), world) * edge_t;
            wsum = wsum + edge_t;
        }
        if (s != mat) {
            color_sum = color_sum + material_color(s, world) * edge_b;
            effect_sum = effect_sum + effect_overlay(unpack_effect(p_s), world) * edge_b;
            wsum = wsum + edge_b;
        }

        var rgb = (color_sum + effect_sum) / max(wsum, 0.0001);

        if (material_diagonal(mat)) {
            let corner = blend * 1.35;
            if (local.x + local.y < corner) {
                let nw = unpack_material(p_nw);
                if (w == n && w != mat && nw == w) {
                    rgb = material_color(w, world) + effect_overlay(unpack_effect(p_nw), world);
                }
            }
            if ((1.0 - local.x) + local.y < corner) {
                let ne = unpack_material(p_ne);
                if (e == n && e != mat && ne == e) {
                    rgb = material_color(e, world) + effect_overlay(unpack_effect(p_ne), world);
                }
            }
            if (local.x + (1.0 - local.y) < corner) {
                let sw = unpack_material(p_sw);
                if (w == s && w != mat && sw == w) {
                    rgb = material_color(w, world) + effect_overlay(unpack_effect(p_sw), world);
                }
            }
            if ((1.0 - local.x) + (1.0 - local.y) < corner) {
                let se = unpack_material(p_se);
                if (e == s && e != mat && se == e) {
                    rgb = material_color(e, world) + effect_overlay(unpack_effect(p_se), world);
                }
            }
        }

        let line = max(
            1.0 - smoothstep(0.0, 0.02, min(local.x, 1.0 - local.x)),
            1.0 - smoothstep(0.0, 0.02, min(local.y, 1.0 - local.y)),
        );
        rgb = mix(rgb, rgb * 0.82, line * clamp(g.grid_opacity, 0.0, 0.35));
        return vec4<f32>(rgb, 1.0);
    }

    var rgb = material_color(mat, world) + effect_overlay(eff, world);
    let line = max(
        1.0 - smoothstep(0.0, 0.02, min(local.x, 1.0 - local.x)),
        1.0 - smoothstep(0.0, 0.02, min(local.y, 1.0 - local.y)),
    );
    rgb = mix(rgb, rgb * 0.82, line * clamp(g.grid_opacity, 0.0, 0.35));
    return vec4<f32>(rgb, 1.0);
}
