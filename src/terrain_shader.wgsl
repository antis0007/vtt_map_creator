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

fn clamp_tile(p: vec2<i32>) -> vec2<u32> {
    let max_xy = vec2<i32>(vec2<u32>(max(g.map_size, vec2<u32>(1u))) - vec2<u32>(1u));
    return vec2<u32>(clamp(p, vec2<i32>(0, 0), max_xy));
}

fn tile_at(p: vec2<i32>) -> u32 {
    let c = clamp_tile(p);
    let idx = c.y * g.map_size.x + c.x;
    return tiles[idx];
}

fn hash11(n: f32) -> f32 {
    return fract(sin(n) * 43758.5453123);
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

fn pebble(p: vec2<f32>) -> f32 {
    let cell = floor(p * 4.0);
    let f = fract(p * 4.0) - 0.5;
    let jitter = vec2<f32>(hash21(cell), hash21(cell + 13.1)) - 0.5;
    let d = length(f - jitter * 0.35);
    return smoothstep(0.42, 0.08, d);
}

fn mortar_mask(p: vec2<f32>) -> f32 {
    let row = floor(p.y);
    let offset = select(0.0, 0.5, i32(row) % 2 != 0);
    let local = vec2<f32>(fract(p.x + offset), fract(p.y));
    let mx = 1.0 - smoothstep(0.03, 0.08, min(local.x, 1.0 - local.x));
    let my = 1.0 - smoothstep(0.04, 0.09, min(local.y, 1.0 - local.y));
    return max(mx, my);
}

fn water_wave(p: vec2<f32>) -> f32 {
    let edge_pull = 0.5 + 0.5 * sin((p.x + p.y) * 2.4 + fbm(p * 0.7) * 4.0 - g.time * 1.8);
    let ripple = 0.5 + 0.5 * sin(p.x * 10.0 + p.y * 6.5 - g.time * 2.4 + fbm(p * 2.0) * 6.0);
    return mix(edge_pull, ripple, 0.55);
}

fn grass_sway(p: vec2<f32>) -> f32 {
    let bands = 0.5 + 0.5 * sin(p.x * 14.0 + fbm(p * 2.5) * 3.0 + g.time * 1.4);
    let broad = 0.5 + 0.5 * sin((p.x * 1.7 + p.y * 0.5) + g.time * 0.8);
    return mix(bands, broad, 0.35);
}

fn material_color(material: u32, world: vec2<f32>) -> vec3<f32> {
    let n1 = fbm(world * 2.6);
    let n2 = fbm(world * 6.2 + vec2<f32>(17.0, -9.0));

    if (material == 0u) {
        let tint = 0.76 + n1 * 0.18 + n2 * 0.06;
        return vec3<f32>(0.40, 0.30, 0.21) * tint;
    }
    if (material == 1u) {
        let sway = grass_sway(world);
        let blades = 0.5 + 0.5 * sin(world.x * 16.0 + world.y * 3.0 + n2 * 5.0 + g.time * 1.1);
        let tint = 0.72 + n1 * 0.12 + sway * 0.10 + blades * 0.08;
        return vec3<f32>(0.30, 0.50, 0.23) * tint + vec3<f32>(0.01, 0.03, 0.00);
    }
    if (material == 2u) {
        let ripples = 0.5 + 0.5 * sin(world.x * 11.0 + world.y * 1.2 + n1 * 4.0);
        let tint = 0.82 + n2 * 0.08 + ripples * 0.08;
        return vec3<f32>(0.78, 0.70, 0.48) * tint;
    }
    if (material == 3u) {
        let wave = water_wave(world);
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
        let pebs = pebble(world);
        let tint = 0.82 + n1 * 0.08;
        return vec3<f32>(0.48, 0.48, 0.46) * tint + vec3<f32>(0.08, 0.08, 0.08) * pebs;
    }
    if (material == 6u) {
        let mortar = mortar_mask(world * vec2<f32>(1.4, 1.0));
        let brick = vec3<f32>(0.56, 0.26, 0.19) * (0.78 + n1 * 0.10);
        let mortar_c = vec3<f32>(0.72, 0.69, 0.64);
        return mix(brick, mortar_c, mortar);
    }

    let chips = pebble(world * 0.8);
    let tint = 0.80 + n1 * 0.08 + chips * 0.10;
    return vec3<f32>(0.52, 0.45, 0.35) * tint;
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

    let blend = clamp(g.blend_strength, 0.0, 0.48);
    let edge0 = 0.5 - blend;
    let edge1 = 0.5 + blend;
    let wx = smoothstep(edge0, edge1, local.x);
    let wy = smoothstep(edge0, edge1, local.y);

    let diag_axis = (local.x + local.y) - 1.0;
    let diag_mix = smoothstep(-blend, blend, diag_axis);

    var color_sum = vec3<f32>(0.0, 0.0, 0.0);
    var effect_sum = vec3<f32>(0.0, 0.0, 0.0);
    let p00 = tile_at(base_tile + vec2<i32>(0, 0));
    let p10 = tile_at(base_tile + vec2<i32>(1, 0));
    let p01 = tile_at(base_tile + vec2<i32>(0, 1));
    let p11 = tile_at(base_tile + vec2<i32>(1, 1));

    let w00 = (1.0 - wx) * (1.0 - wy);
    let w10 = wx * (1.0 - wy);
    let w01 = (1.0 - wx) * wy;
    let w11 = wx * wy;

    let d_bias = 0.16;
    let w00d = w00 + (1.0 - diag_mix) * d_bias;
    let w11d = w11 + diag_mix * d_bias;
    let norm = max(w00d + w10 + w01 + w11d, 0.0001);

    color_sum = color_sum + material_color(unpack_material(p00), world) * w00d;
    color_sum = color_sum + material_color(unpack_material(p10), world) * w10;
    color_sum = color_sum + material_color(unpack_material(p01), world) * w01;
    color_sum = color_sum + material_color(unpack_material(p11), world) * w11d;

    effect_sum = effect_sum + effect_overlay(unpack_effect(p00), world) * w00d;
    effect_sum = effect_sum + effect_overlay(unpack_effect(p10), world) * w10;
    effect_sum = effect_sum + effect_overlay(unpack_effect(p01), world) * w01;
    effect_sum = effect_sum + effect_overlay(unpack_effect(p11), world) * w11d;

    var rgb = (color_sum + effect_sum) / norm;

    let tile_local = fract(world);
    let line = max(
        1.0 - smoothstep(0.0, 0.02, min(tile_local.x, 1.0 - tile_local.x)),
        1.0 - smoothstep(0.0, 0.02, min(tile_local.y, 1.0 - tile_local.y)),
    );
    rgb = mix(rgb, rgb * 0.82, line * clamp(g.grid_opacity, 0.0, 0.35));

    return vec4<f32>(rgb, 1.0);
}
