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

const WALL_BLOCK_COLS: f32 = 3.0;
const WALL_BLOCK_ROWS: f32 = 2.0;
const WALL_SEAM_WIDTH: f32 = 0.06;
const WALL_FRAME_THICKNESS: f32 = 0.10;
const WALL_DOOR_WIDTH: f32 = 0.46;
const WALL_DOOR_HEIGHT: f32 = 0.78;
const WALL_WINDOW_WIDTH: f32 = 0.56;
const WALL_WINDOW_HEIGHT: f32 = 0.42;
const WALL_MULLION_THICKNESS: f32 = 0.05;

fn unpack_material(packed: u32) -> u32 {
    return packed & 0xffu;
}

fn unpack_effect(packed: u32) -> u32 {
    return (packed >> 8u) & 0xffu;
}

const MATERIAL_DIRT: u32 = 0u;
const MATERIAL_GRASS: u32 = 1u;
const MATERIAL_HIGH_GRASS: u32 = 2u;
const MATERIAL_BUSHES: u32 = 3u;
const MATERIAL_SAND: u32 = 4u;
const MATERIAL_WATER: u32 = 5u;
const MATERIAL_LAVA: u32 = 6u;
const MATERIAL_GRAVEL: u32 = 7u;
const MATERIAL_BRICK: u32 = 8u;
const MATERIAL_PATH: u32 = 9u;
const MATERIAL_WALL: u32 = 10u;
const MATERIAL_WALL_DOOR: u32 = 11u;
const MATERIAL_WALL_WINDOW: u32 = 12u;
const MATERIAL_FLOOR_WOOD: u32 = 13u;
const MATERIAL_SENTINEL: u32 = 255u;
const EFFECT_SENTINEL: u32 = 255u;

fn is_foliage(material: u32) -> bool {
    return material == MATERIAL_GRASS || material == MATERIAL_HIGH_GRASS || material == MATERIAL_BUSHES;
}

fn material_blends(material: u32) -> bool {
    return material <= MATERIAL_GRAVEL || material == MATERIAL_PATH || material == MATERIAL_WALL;
}

fn material_diagonal(material: u32) -> bool {
    return material == MATERIAL_PATH || material == MATERIAL_WALL;
}

fn material_structural(material: u32) -> bool {
    return material == MATERIAL_BRICK
        || material == MATERIAL_WALL
        || material == MATERIAL_WALL_DOOR
        || material == MATERIAL_WALL_WINDOW
        || material == MATERIAL_FLOOR_WOOD;
}

fn material_blend_compatible(base: u32, neighbor: u32) -> bool {
    if (base == neighbor || !material_blends(base) || !material_blends(neighbor)) {
        return false;
    }
    if ((is_foliage(base) && material_structural(neighbor))
        || (is_foliage(neighbor) && material_structural(base))) {
        return false;
    }
    return true;
}

fn in_bounds(p: vec2<i32>) -> bool {
    return p.x >= 0 && p.y >= 0 && p.x < i32(g.map_size.x) && p.y < i32(g.map_size.y);
}

fn clamp_tile(p: vec2<i32>) -> vec2<u32> {
    let max_xy = vec2<i32>(vec2<u32>(max(g.map_size, vec2<u32>(1u))) - vec2<u32>(1u));
    return vec2<u32>(clamp(p, vec2<i32>(0, 0), max_xy));
}

fn tile_at_clamped(p: vec2<i32>) -> u32 {
    let c = clamp_tile(p);
    let idx = c.y * g.map_size.x + c.x;
    return tiles[idx];
}

fn tile_at_or_sentinel(p: vec2<i32>) -> u32 {
    if (in_bounds(p)) {
        let c = vec2<u32>(u32(p.x), u32(p.y));
        let idx = c.y * g.map_size.x + c.x;
        return tiles[idx];
    }
    return MATERIAL_SENTINEL | (EFFECT_SENTINEL << 8u);
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

fn vec_noise2(p: vec2<f32>, time: f32) -> vec2<f32> {
    let a = fbm(p + vec2<f32>(time * 0.21, -time * 0.17));
    let b = fbm(p * 1.07 + vec2<f32>(-13.4 - time * 0.16, 9.1 + time * 0.20));
    return vec2<f32>(a * 2.0 - 1.0, b * 2.0 - 1.0);
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

fn rect_mask(p: vec2<f32>, min_p: vec2<f32>, max_p: vec2<f32>) -> f32 {
    let left = step(min_p.x, p.x);
    let right = step(p.x, max_p.x);
    let top = step(min_p.y, p.y);
    let bottom = step(p.y, max_p.y);
    return left * right * top * bottom;
}

fn wall_block_mask(tile_local: vec2<f32>) -> f32 {
    let grid = tile_local * vec2<f32>(WALL_BLOCK_COLS, WALL_BLOCK_ROWS);
    let cell_local = fract(grid);
    let seam_x = 1.0 - smoothstep(0.0, WALL_SEAM_WIDTH, min(cell_local.x, 1.0 - cell_local.x));
    let seam_y = 1.0 - smoothstep(0.0, WALL_SEAM_WIDTH, min(cell_local.y, 1.0 - cell_local.y));
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
    let tile_local = fract(world);
    let block_grid = tile_local * vec2<f32>(WALL_BLOCK_COLS, WALL_BLOCK_ROWS);
    let block_id = floor(block_grid);
    let n1 = fbm(world * 2.6);
    let n2 = fbm(world * 6.2 + vec2<f32>(17.0, -9.0));

    if (material == MATERIAL_DIRT) {
        let tint = 0.76 + n1 * 0.18 + n2 * 0.06;
        return vec3<f32>(0.40, 0.30, 0.21) * tint;
    }
    if (is_foliage(material)) {
        let type_scale = select(select(1.0, 1.25, material == MATERIAL_HIGH_GRASS), 0.7, material == MATERIAL_BUSHES);
        let shade_base = select(select(0.72, 0.66, material == MATERIAL_HIGH_GRASS), 0.62, material == MATERIAL_BUSHES);
        let band_amp = select(select(0.10, 0.13, material == MATERIAL_HIGH_GRASS), 0.18, material == MATERIAL_BUSHES);
        let cluster_amp = select(select(0.08, 0.11, material == MATERIAL_HIGH_GRASS), 0.14, material == MATERIAL_BUSHES);
        let wind_vec = vec_noise2(world * 0.48 + vec2<f32>(2.0, -5.0), g.time);
        let orient = world.x * (12.0 + type_scale * 2.6) + world.y * (2.0 + type_scale) + wind_vec.x * 3.4 + wind_vec.y * 2.8;
        let blades = 0.5 + 0.5 * sin(orient + n2 * (4.2 + type_scale));
        let cluster = fbm(world * (3.4 + type_scale));
        let detail = fbm(world * 11.0 + vec2<f32>(4.0, -3.0));
        let tint = shade_base + n1 * 0.14 + blades * band_amp + cluster * cluster_amp;
        let base = select(
            select(vec3<f32>(0.30, 0.50, 0.23), vec3<f32>(0.26, 0.45, 0.21), material == MATERIAL_HIGH_GRASS),
            vec3<f32>(0.21, 0.39, 0.18),
            material == MATERIAL_BUSHES,
        ) * tint;
        let directional = vec3<f32>(base.x * 0.88, base.y * 1.08, base.z * 0.90);
        return mix(base, directional, (detail * 0.5 + 0.5) * 0.25);
    }
    if (material == MATERIAL_SAND) {
        let ripples = 0.5 + 0.5 * sin(world.x * 11.0 + world.y * 1.2 + n1 * 4.0);
        let tint = 0.82 + n2 * 0.08 + ripples * 0.08;
        return vec3<f32>(0.78, 0.70, 0.48) * tint;
    }
    if (material == MATERIAL_WATER) {
        let wave = 0.5 + 0.5 * sin((world.x + world.y) * 2.4 + fbm(world * 0.7) * 4.0 - g.time * 1.8);
        let caustic = 0.5 + 0.5 * sin(world.x * 18.0 - world.y * 10.0 + g.time * 2.0 + n2 * 8.0);
        let base = vec3<f32>(0.10, 0.32, 0.52) * (0.76 + wave * 0.28);
        return base + vec3<f32>(0.03, 0.09, 0.13) * caustic;
    }
    if (material == MATERIAL_LAVA) {
        let molten = 0.5 + 0.5 * sin(world.x * 9.0 + n1 * 8.0 - g.time * 2.2) * cos(world.y * 7.0 - n2 * 6.0 + g.time * 1.8);
        let base = vec3<f32>(0.58, 0.16, 0.04) * (0.74 + molten * 0.30);
        return base + vec3<f32>(0.22, 0.10, 0.00) * molten;
    }
    if (material == MATERIAL_GRAVEL) {
        let grain = hash21(floor(world * 7.0));
        let tint = 0.82 + n1 * 0.08;
        return vec3<f32>(0.48, 0.48, 0.46) * tint + vec3<f32>(0.08, 0.08, 0.08) * grain;
    }
    if (material == MATERIAL_BRICK) {
        let seam = brick_topdown_mask(world + vec2<f32>(n1 * 0.02, n2 * 0.02));
        let brick = vec3<f32>(0.49, 0.22, 0.16) * (0.86 + n1 * 0.12);
        let grout = vec3<f32>(0.71, 0.66, 0.60);
        return mix(brick, grout, seam * 0.9);
    }
    if (material == MATERIAL_PATH) {
        let chips = hash21(floor(world * 5.0));
        return vec3<f32>(0.50, 0.43, 0.33) * (0.86 + n1 * 0.08 + chips * 0.08);
    }
    if (material == MATERIAL_WALL) {
        let seam = wall_block_mask(tile_local);
        let block_seed = hash21(floor(world) * 11.0 + block_id * 3.0);
        let block_light = 0.82 + n1 * 0.10 + (block_seed - 0.5) * 0.10;
        let stone = vec3<f32>(0.54, 0.57, 0.60) * block_light;
        let mortar = vec3<f32>(0.37, 0.38, 0.39);
        return mix(stone, mortar, seam * 0.95);
    }
    if (material == MATERIAL_WALL_DOOR) {
        let seam = wall_block_mask(tile_local);
        let stone = vec3<f32>(0.53, 0.56, 0.60) * (0.82 + n1 * 0.09);
        let mortar = vec3<f32>(0.36, 0.37, 0.39);
        let wall = mix(stone, mortar, seam * 0.9);

        let opening_min = vec2<f32>(0.5 - WALL_DOOR_WIDTH * 0.5, 1.0 - WALL_DOOR_HEIGHT + WALL_FRAME_THICKNESS);
        let opening_max = vec2<f32>(0.5 + WALL_DOOR_WIDTH * 0.5, 1.0 - WALL_FRAME_THICKNESS);
        let frame_min = vec2<f32>(opening_min.x - WALL_FRAME_THICKNESS, opening_min.y - WALL_FRAME_THICKNESS);
        let frame_max = vec2<f32>(opening_max.x + WALL_FRAME_THICKNESS, 1.0);

        let opening = rect_mask(tile_local, opening_min, opening_max);
        let frame = max(0.0, rect_mask(tile_local, frame_min, frame_max) - opening);
        let door_wood = vec3<f32>(0.25, 0.15, 0.08) * (0.78 + n2 * 0.12);
        let trim = vec3<f32>(0.63, 0.66, 0.69);
        let with_frame = mix(wall, trim, frame);
        return mix(with_frame, door_wood, opening);
    }
    if (material == MATERIAL_WALL_WINDOW) {
        let seam = wall_block_mask(tile_local);
        let stone = vec3<f32>(0.53, 0.56, 0.60) * (0.84 + n1 * 0.08);
        let mortar = vec3<f32>(0.36, 0.37, 0.39);
        let wall = mix(stone, mortar, seam * 0.9);

        let outer_min = vec2<f32>(0.5 - WALL_WINDOW_WIDTH * 0.5, 0.36 - WALL_WINDOW_HEIGHT * 0.5);
        let outer_max = vec2<f32>(0.5 + WALL_WINDOW_WIDTH * 0.5, 0.36 + WALL_WINDOW_HEIGHT * 0.5);
        let inner_min = outer_min + vec2<f32>(WALL_FRAME_THICKNESS, WALL_FRAME_THICKNESS);
        let inner_max = outer_max - vec2<f32>(WALL_FRAME_THICKNESS, WALL_FRAME_THICKNESS);
        let window_outer = rect_mask(tile_local, outer_min, outer_max);
        let glass_area = rect_mask(tile_local, inner_min, inner_max);
        let frame = max(0.0, window_outer - glass_area);

        let inner_size = inner_max - inner_min;
        let v_mid = inner_min.x + inner_size.x * 0.5;
        let h_mid = inner_min.y + inner_size.y * 0.5;
        let v_bar = rect_mask(tile_local, vec2<f32>(v_mid - WALL_MULLION_THICKNESS, inner_min.y), vec2<f32>(v_mid + WALL_MULLION_THICKNESS, inner_max.y));
        let h_bar = rect_mask(tile_local, vec2<f32>(inner_min.x, h_mid - WALL_MULLION_THICKNESS), vec2<f32>(inner_max.x, h_mid + WALL_MULLION_THICKNESS));
        let mullion = max(v_bar, h_bar) * glass_area;

        let pane_variation = hash21(floor(world) * 7.0 + block_id);
        let glass = vec3<f32>(0.33, 0.50, 0.66) * (0.84 + pane_variation * 0.16);
        let highlight = rect_mask(tile_local, inner_min + vec2<f32>(0.03, 0.03), inner_min + vec2<f32>(inner_size.x * 0.35, inner_size.y * 0.26)) * glass_area;
        let trim = vec3<f32>(0.66, 0.69, 0.72);
        var color = mix(wall, trim, frame);
        color = mix(color, glass, glass_area);
        color = mix(color, trim * 0.95, mullion);
        return color + vec3<f32>(0.12, 0.14, 0.16) * highlight;
    }

    let seam = plank_mask(world + vec2<f32>(n2 * 0.03, 0.0));
    let wood = vec3<f32>(0.57, 0.40, 0.25) * (0.86 + n1 * 0.12);
    let gap = vec3<f32>(0.30, 0.21, 0.13);
    return mix(wood, gap, seam * 0.8);
}

fn rain_streak(world: vec2<f32>, dir: vec2<f32>, phase: f32, freq: f32, width: f32, speed: f32) -> f32 {
    let dir_n = normalize(dir);
    let orth = vec2<f32>(-dir_n.y, dir_n.x);
    let u = dot(world, orth);
    let v = dot(world, dir_n);
    let jitter = fbm(world * 2.8 + vec2<f32>(phase, -phase)) * 0.35;
    let lane = fract((u + phase + jitter) * freq - g.time * speed);
    let core = 1.0 - smoothstep(0.5 - width, 0.5 + width, abs(lane - 0.5));
    let length = 0.5 + 0.5 * sin(v * 9.0 + phase * 3.0 - g.time * speed * 0.7);
    return core * (0.45 + length * 0.55);
}

fn rain_drops(world: vec2<f32>, grid: f32, threshold: f32, speed: f32) -> f32 {
    let cell = floor(world * grid + vec2<f32>(0.0, g.time * speed));
    let seed = hash21(cell + vec2<f32>(17.0, -9.0));
    if (seed < 1.0 - threshold) {
        return 0.0;
    }
    let local = fract(world * grid) - vec2<f32>(0.5, 0.5);
    let r = length(local);
    let spot = 1.0 - smoothstep(0.12, 0.42, r);
    return spot * ((seed - (1.0 - threshold)) / threshold);
}

fn effect_overlay(effect: u32, material: u32, world: vec2<f32>, strength: f32) -> vec3<f32> {
    let s = clamp(strength, 0.0, 2.0);
    if (effect == 1u) {
        if (!is_foliage(material)) {
            return vec3<f32>(0.0);
        }
        let flow = vec_noise2(world * 0.35 + vec2<f32>(3.1, -2.4), g.time);
        let turbulence = fbm(world * 1.4 + vec2<f32>(11.0 - g.time * 0.3, -7.0 + g.time * 0.25));
        let gust = clamp(flow.x * 0.75 + flow.y * 0.45 + turbulence * 0.35, 0.0, 1.0);
        let lift = gust * 0.12 * s;
        return vec3<f32>(-0.002, 0.01, -0.002) + vec3<f32>(0.02, 0.03, 0.015) * lift;
    }
    if (effect == 2u) {
        let streak1 = rain_streak(world, vec2<f32>(0.52, -1.35), 8.0, 22.0, 0.18, 1.3);
        let streak2 = rain_streak(world, vec2<f32>(0.16, -1.0), -4.0, 34.0, 0.12, 2.0);
        let streak3 = rain_streak(world, vec2<f32>(0.73, -1.7), 13.0, 48.0, 0.09, 2.8);
        let drops = rain_drops(world, 9.5, 0.22, 2.2) * 0.35 + rain_drops(world + vec2<f32>(5.7, -1.3), 17.0, 0.13, 3.0) * 0.20;
        let wet = clamp(streak1 * 0.50 + streak2 * 0.32 + streak3 * 0.22 + drops, 0.0, 1.0) * s;
        return vec3<f32>(-0.06, -0.06, -0.05) * wet + vec3<f32>(0.0, 0.0, 0.03) * wet;
    }
    return vec3<f32>(0.0, 0.0, 0.0);
}

fn edge_weight(distance: f32, blend: f32) -> f32 {
    return 1.0 - smoothstep(0.0, blend, distance);
}

fn corner_weight(dx: f32, dy: f32, blend: f32) -> f32 {
    let radius = max(blend * 1.4142135, 0.0001);
    let d = length(vec2<f32>(dx, dy));
    return 1.0 - smoothstep(0.0, radius, d);
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

    let p00 = tile_at_clamped(base_tile);
    let p_w = tile_at_or_sentinel(base_tile + vec2<i32>(-1, 0));
    let p_e = tile_at_or_sentinel(base_tile + vec2<i32>(1, 0));
    let p_n = tile_at_or_sentinel(base_tile + vec2<i32>(0, -1));
    let p_s = tile_at_or_sentinel(base_tile + vec2<i32>(0, 1));
    let p_nw = tile_at_or_sentinel(base_tile + vec2<i32>(-1, -1));
    let p_ne = tile_at_or_sentinel(base_tile + vec2<i32>(1, -1));
    let p_sw = tile_at_or_sentinel(base_tile + vec2<i32>(-1, 1));
    let p_se = tile_at_or_sentinel(base_tile + vec2<i32>(1, 1));

    let mat = unpack_material(p00);
    let eff = unpack_effect(p00);

    let blend = clamp(g.blend_strength, 0.0, 0.48);
    if (blend > 0.0 && material_blends(mat)) {
        let edge_l = edge_weight(local.x, blend);
        let edge_r = edge_weight(1.0 - local.x, blend);
        let edge_t = edge_weight(local.y, blend);
        let edge_b = edge_weight(1.0 - local.y, blend);
        let corner_nw = corner_weight(local.x, local.y, blend);
        let corner_ne = corner_weight(1.0 - local.x, local.y, blend);
        let corner_sw = corner_weight(local.x, 1.0 - local.y, blend);
        let corner_se = corner_weight(1.0 - local.x, 1.0 - local.y, blend);

        let w = unpack_material(p_w);
        let e = unpack_material(p_e);
        let n = unpack_material(p_n);
        let s = unpack_material(p_s);
        let nw = unpack_material(p_nw);
        let ne = unpack_material(p_ne);
        let sw = unpack_material(p_sw);
        let se = unpack_material(p_se);

        var wsum = 1.0;
        var color_sum = material_color(mat, world);
        var effect_sum = effect_overlay(eff, mat, world, 1.0);
        let ww = edge_l * select(0.0, 1.0, w != mat);
        let we = edge_r * select(0.0, 1.0, e != mat);
        let wn = edge_t * select(0.0, 1.0, n != mat);
        let ws = edge_b * select(0.0, 1.0, s != mat);
        let wnw = corner_nw * select(0.0, 1.0, nw != mat);
        let wne = corner_ne * select(0.0, 1.0, ne != mat);
        let wsw = corner_sw * select(0.0, 1.0, sw != mat);
        let wse = corner_se * select(0.0, 1.0, se != mat);

        color_sum = color_sum + material_color(w, world) * ww;
        effect_sum = effect_sum + effect_overlay(unpack_effect(p_w), w, world, 1.0) * ww;
        color_sum = color_sum + material_color(e, world) * we;
        effect_sum = effect_sum + effect_overlay(unpack_effect(p_e), e, world, 1.0) * we;
        color_sum = color_sum + material_color(n, world) * wn;
        effect_sum = effect_sum + effect_overlay(unpack_effect(p_n), n, world, 1.0) * wn;
        color_sum = color_sum + material_color(s, world) * ws;
        effect_sum = effect_sum + effect_overlay(unpack_effect(p_s), s, world, 1.0) * ws;
        color_sum = color_sum + material_color(nw, world) * wnw;
        effect_sum = effect_sum + effect_overlay(unpack_effect(p_nw), nw, world, 1.0) * wnw;
        color_sum = color_sum + material_color(ne, world) * wne;
        effect_sum = effect_sum + effect_overlay(unpack_effect(p_ne), ne, world, 1.0) * wne;
        color_sum = color_sum + material_color(sw, world) * wsw;
        effect_sum = effect_sum + effect_overlay(unpack_effect(p_sw), sw, world, 1.0) * wsw;
        color_sum = color_sum + material_color(se, world) * wse;
        effect_sum = effect_sum + effect_overlay(unpack_effect(p_se), se, world, 1.0) * wse;
        wsum = wsum + ww + we + wn + ws + wnw + wne + wsw + wse;

        var rgb = (color_sum + effect_sum) / max(wsum, 0.0001);

        if (material_diagonal(mat)) {
            let corner = blend * 1.35;
            let aa = max(0.01, fwidth(local.x + local.y) * 1.5);

            let valid_nw = select(0.0, 1.0, w == n && w != mat && nw == w);
            let valid_ne = select(0.0, 1.0, e == n && e != mat && ne == e);
            let valid_sw = select(0.0, 1.0, w == s && w != mat && sw == w);
            let valid_se = select(0.0, 1.0, e == s && e != mat && se == e);

            let mask_nw = (1.0 - smoothstep(-aa, aa, local.x + local.y - corner)) * valid_nw;
            let mask_ne = (1.0 - smoothstep(-aa, aa, (1.0 - local.x) + local.y - corner)) * valid_ne;
            let mask_sw = (1.0 - smoothstep(-aa, aa, local.x + (1.0 - local.y) - corner)) * valid_sw;
            let mask_se = (1.0 - smoothstep(-aa, aa, (1.0 - local.x) + (1.0 - local.y) - corner)) * valid_se;

            let rgb_nw = material_color(w, world) + effect_overlay(unpack_effect(p_nw), w, world, 1.0);
            let rgb_ne = material_color(e, world) + effect_overlay(unpack_effect(p_ne), e, world, 1.0);
            let rgb_sw = material_color(w, world) + effect_overlay(unpack_effect(p_sw), w, world, 1.0);
            let rgb_se = material_color(e, world) + effect_overlay(unpack_effect(p_se), e, world, 1.0);

            rgb = mix(rgb, rgb_nw, clamp(mask_nw, 0.0, 1.0));
            rgb = mix(rgb, rgb_ne, clamp(mask_ne, 0.0, 1.0));
            rgb = mix(rgb, rgb_sw, clamp(mask_sw, 0.0, 1.0));
            rgb = mix(rgb, rgb_se, clamp(mask_se, 0.0, 1.0));
        }

        let line = max(
            1.0 - smoothstep(0.0, 0.02, min(local.x, 1.0 - local.x)),
            1.0 - smoothstep(0.0, 0.02, min(local.y, 1.0 - local.y)),
        );
        rgb = mix(rgb, rgb * 0.82, line * clamp(g.grid_opacity, 0.0, 0.35));
        return vec4<f32>(rgb, 1.0);
    }

    var rgb = material_color(mat, world) + effect_overlay(eff, mat, world, 1.0);
    let line = max(
        1.0 - smoothstep(0.0, 0.02, min(local.x, 1.0 - local.x)),
        1.0 - smoothstep(0.0, 0.02, min(local.y, 1.0 - local.y)),
    );
    rgb = mix(rgb, rgb * 0.82, line * clamp(g.grid_opacity, 0.0, 0.35));
    return vec4<f32>(rgb, 1.0);
}
