use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum MaterialKind {
    #[default]
    Dirt = 0,
    Grass = 1,
    Sand = 2,
    Water = 3,
    Lava = 4,
    Gravel = 5,
    Brick = 6,
    Path = 7,
    Wall = 8,
    WallDoor = 9,
    WallWindow = 10,
    FloorWood = 11,
}

impl MaterialKind {
    pub const ALL: [Self; 12] = [
        Self::Dirt,
        Self::Grass,
        Self::Sand,
        Self::Water,
        Self::Lava,
        Self::Gravel,
        Self::Brick,
        Self::Path,
        Self::Wall,
        Self::WallDoor,
        Self::WallWindow,
        Self::FloorWood,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dirt => "Dirt",
            Self::Grass => "Grass",
            Self::Sand => "Sand",
            Self::Water => "Water",
            Self::Lava => "Lava",
            Self::Gravel => "Gravel",
            Self::Brick => "Brick",
            Self::Path => "Path",
            Self::Wall => "Wall",
            Self::WallDoor => "Wall Door",
            Self::WallWindow => "Wall Window",
            Self::FloorWood => "Wood Floor",
        }
    }

    pub fn blends(self) -> bool {
        matches!(
            self,
            Self::Dirt
                | Self::Grass
                | Self::Sand
                | Self::Water
                | Self::Lava
                | Self::Gravel
                | Self::Path
                | Self::Wall
        )
    }

    pub fn diagonal_blend(self) -> bool {
        matches!(self, Self::Path | Self::Wall)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum EffectKind {
    #[default]
    None = 0,
    Wind = 1,
    Rain = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum SplitKind {
    #[default]
    None = 0,
    DiagNWSE = 1,
    DiagNESW = 2,
}

impl SplitKind {
    pub const ALL: [Self; 3] = [Self::None, Self::DiagNWSE, Self::DiagNESW];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::DiagNWSE => "Diag ↘ (NW-SE)",
            Self::DiagNESW => "Diag ↙ (NE-SW)",
        }
    }
}

impl EffectKind {
    pub const ALL: [Self; 3] = [Self::None, Self::Wind, Self::Rain];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Wind => "Wind",
            Self::Rain => "Rain",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditLayer {
    Base,
    Effect,
}

impl EditLayer {
    pub fn label(self) -> &'static str {
        match self {
            Self::Base => "Terrain",
            Self::Effect => "Effects",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Brush,
    Fill,
    Rect,
    Select,
    Erase,
}

impl ToolKind {
    pub const ALL: [Self; 5] = [
        Self::Brush,
        Self::Fill,
        Self::Rect,
        Self::Select,
        Self::Erase,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Brush => "Brush",
            Self::Fill => "Fill",
            Self::Rect => "Rect",
            Self::Select => "Select",
            Self::Erase => "Erase",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    pub anchor: Option<[u32; 2]>,
    pub focus: Option<[u32; 2]>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.anchor = None;
        self.focus = None;
    }

    pub fn is_active(self) -> bool {
        self.anchor.is_some() && self.focus.is_some()
    }

    pub fn bounds(self) -> Option<([u32; 2], [u32; 2])> {
        let a = self.anchor?;
        let b = self.focus?;
        Some((
            [a[0].min(b[0]), a[1].min(b[1])],
            [a[0].max(b[0]), a[1].max(b[1])],
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapDocument {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub tile_px: u32,
    pub terrain: Vec<MaterialKind>,
    pub effects: Vec<EffectKind>,
    #[serde(default)]
    pub split: Vec<SplitKind>,
    #[serde(default)]
    pub secondary_terrain: Vec<MaterialKind>,
}

impl MapDocument {
    pub fn new(width: u32, height: u32) -> Self {
        let len = (width * height) as usize;
        Self {
            name: "untitled".to_owned(),
            width,
            height,
            tile_px: 32,
            terrain: vec![MaterialKind::Grass; len],
            effects: vec![EffectKind::None; len],
            split: vec![SplitKind::None; len],
            secondary_terrain: vec![MaterialKind::Grass; len],
        }
    }

    pub fn normalize_layers(&mut self) {
        let len = self.len();
        self.terrain.resize(len, MaterialKind::Grass);
        self.effects.resize(len, EffectKind::None);
        self.split.resize(len, SplitKind::None);
        self.secondary_terrain.resize(len, MaterialKind::Grass);
    }

    pub fn len(&self) -> usize {
        (self.width * self.height) as usize
    }

    pub fn dims(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    pub fn contains_i32(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }

    pub fn idx(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    pub fn terrain_at_i32(&self, x: i32, y: i32) -> MaterialKind {
        if self.contains_i32(x, y) {
            self.terrain[self.idx(x as u32, y as u32)]
        } else {
            MaterialKind::Dirt
        }
    }

    pub fn effect_at_i32(&self, x: i32, y: i32) -> EffectKind {
        if self.contains_i32(x, y) {
            self.effects[self.idx(x as u32, y as u32)]
        } else {
            EffectKind::None
        }
    }

    pub fn split_at_i32(&self, x: i32, y: i32) -> SplitKind {
        if self.contains_i32(x, y) {
            self.split[self.idx(x as u32, y as u32)]
        } else {
            SplitKind::None
        }
    }

    pub fn secondary_at_i32(&self, x: i32, y: i32) -> MaterialKind {
        if self.contains_i32(x, y) {
            self.secondary_terrain[self.idx(x as u32, y as u32)]
        } else {
            MaterialKind::Dirt
        }
    }

    pub fn packed_tiles(&self) -> Vec<u32> {
        self.terrain
            .iter()
            .zip(&self.effects)
            .zip(&self.split)
            .zip(&self.secondary_terrain)
            .map(|(((&m, &e), &s), &secondary)| {
                (m as u32) | ((e as u32) << 8) | ((s as u32) << 16) | ((secondary as u32) << 18)
            })
            .collect()
    }

    pub fn split_allowed(material: MaterialKind) -> bool {
        material.diagonal_blend()
    }

    fn apply_one(
        &mut self,
        x: u32,
        y: u32,
        layer: EditLayer,
        material: MaterialKind,
        effect: EffectKind,
        split: SplitKind,
        secondary_material: MaterialKind,
    ) {
        let idx = self.idx(x, y);
        match layer {
            EditLayer::Base => {
                self.terrain[idx] = material;
                if split != SplitKind::None
                    && Self::split_allowed(material)
                    && Self::split_allowed(secondary_material)
                {
                    self.split[idx] = split;
                    self.secondary_terrain[idx] = secondary_material;
                } else {
                    self.split[idx] = SplitKind::None;
                    self.secondary_terrain[idx] = material;
                }
            }
            EditLayer::Effect => self.effects[idx] = effect,
        }
    }

    pub fn erase_one(&mut self, x: u32, y: u32, layer: EditLayer) {
        let idx = self.idx(x, y);
        match layer {
            EditLayer::Base => {
                self.terrain[idx] = MaterialKind::Dirt;
                self.split[idx] = SplitKind::None;
                self.secondary_terrain[idx] = MaterialKind::Dirt;
            }
            EditLayer::Effect => self.effects[idx] = EffectKind::None,
        }
    }

    pub fn paint_disc(
        &mut self,
        center: [u32; 2],
        radius: u32,
        layer: EditLayer,
        material: MaterialKind,
        effect: EffectKind,
        split: SplitKind,
        secondary_material: MaterialKind,
    ) {
        let [cx, cy] = center;
        let r = radius as i32;
        let rr = (radius * radius) as i32;
        for y in (cy as i32 - r).max(0)..=(cy as i32 + r).min(self.height as i32 - 1) {
            for x in (cx as i32 - r).max(0)..=(cx as i32 + r).min(self.width as i32 - 1) {
                let dx = x - cx as i32;
                let dy = y - cy as i32;
                if dx * dx + dy * dy <= rr {
                    self.apply_one(
                        x as u32,
                        y as u32,
                        layer,
                        material,
                        effect,
                        split,
                        secondary_material,
                    );
                }
            }
        }
    }

    pub fn paint_rect(
        &mut self,
        a: [u32; 2],
        b: [u32; 2],
        layer: EditLayer,
        material: MaterialKind,
        effect: EffectKind,
        split: SplitKind,
        secondary_material: MaterialKind,
    ) {
        let min_x = a[0].min(b[0]);
        let max_x = a[0].max(b[0]);
        let min_y = a[1].min(b[1]);
        let max_y = a[1].max(b[1]);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.apply_one(x, y, layer, material, effect, split, secondary_material);
            }
        }
    }

    pub fn erase_rect(&mut self, a: [u32; 2], b: [u32; 2], layer: EditLayer) {
        let min_x = a[0].min(b[0]);
        let max_x = a[0].max(b[0]);
        let min_y = a[1].min(b[1]);
        let max_y = a[1].max(b[1]);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.erase_one(x, y, layer);
            }
        }
    }

    pub fn fill(
        &mut self,
        start: [u32; 2],
        layer: EditLayer,
        material: MaterialKind,
        effect: EffectKind,
        split: SplitKind,
        secondary_material: MaterialKind,
    ) {
        let [sx, sy] = start;
        let start_idx = self.idx(sx, sy);
        match layer {
            EditLayer::Base => {
                let target = self.terrain[start_idx];
                if target == material && split == SplitKind::None {
                    return;
                }
                let mut queue = VecDeque::from([(sx, sy)]);
                while let Some((x, y)) = queue.pop_front() {
                    let idx = self.idx(x, y);
                    if self.terrain[idx] != target {
                        continue;
                    }
                    self.apply_one(x, y, layer, material, effect, split, secondary_material);
                    if x > 0 {
                        queue.push_back((x - 1, y));
                    }
                    if x + 1 < self.width {
                        queue.push_back((x + 1, y));
                    }
                    if y > 0 {
                        queue.push_back((x, y - 1));
                    }
                    if y + 1 < self.height {
                        queue.push_back((x, y + 1));
                    }
                }
            }
            EditLayer::Effect => {
                let target = self.effects[start_idx];
                if target == effect {
                    return;
                }
                let mut queue = VecDeque::from([(sx, sy)]);
                while let Some((x, y)) = queue.pop_front() {
                    let idx = self.idx(x, y);
                    if self.effects[idx] != target {
                        continue;
                    }
                    self.effects[idx] = effect;
                    if x > 0 {
                        queue.push_back((x - 1, y));
                    }
                    if x + 1 < self.width {
                        queue.push_back((x + 1, y));
                    }
                    if y > 0 {
                        queue.push_back((x, y - 1));
                    }
                    if y + 1 < self.height {
                        queue.push_back((x, y + 1));
                    }
                }
            }
        }
    }

    pub fn clear_effects_in_selection(&mut self, selection: Selection) {
        if let Some((a, b)) = selection.bounds() {
            for y in a[1]..=b[1] {
                for x in a[0]..=b[0] {
                    let idx = self.idx(x, y);
                    self.effects[idx] = EffectKind::None;
                }
            }
        }
    }
}
