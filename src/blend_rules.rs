use crate::model::MaterialKind;

pub const CPU_EXPORT_BLEND_STRENGTH: f32 = 0.2;
pub const BLEND_STRENGTH_MAX: f32 = 0.48;
pub const DIAGONAL_CORNER_SCALE: f32 = 1.35;

pub fn material_blends(material: MaterialKind) -> bool {
    matches!(
        material,
        MaterialKind::Dirt
            | MaterialKind::Grass
            | MaterialKind::HighGrass
            | MaterialKind::Bushes
            | MaterialKind::Sand
            | MaterialKind::Water
            | MaterialKind::Lava
            | MaterialKind::Gravel
            | MaterialKind::Path
            | MaterialKind::Wall
    )
}

pub fn material_diagonal(material: MaterialKind) -> bool {
    matches!(material, MaterialKind::Path | MaterialKind::Wall)
}
