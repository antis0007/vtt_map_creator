# Atlas Forge (Minimal GPU-Backed Rust VTT Map Creator MVP)

A compact, modular Rust project that demonstrates the architecture for a modern 2D virtual tabletop map editor:

- **egui editor UI**
- **custom GPU render backend** in `src/gpu_renderer.rs`
- **procedural shader-based materials** (no full-frame CPU texture rebuilds)
- **terrain layer + effect layer**
- **brush / fill / rect / select / erase**
- **middle-mouse pan + mouse-wheel zoom**
- **save/load map JSON**
- **export PNG preview**

## Why this architecture

This MVP is built around a single full-screen GPU pass over an offscreen render target:

- the map is stored as compact tile data (`u32` per tile)
- tiles are uploaded to a GPU storage buffer **only when the map changes**
- each frame, the GPU shader renders the visible map directly from that tile buffer
- materials and effects are generated procedurally in the shader and blended across tile boundaries
- no per-frame CPU-side RGBA rebuild and no per-frame full texture upload

That makes it a much better base for scaling toward chunking, asset packs, live shader swapping, and richer layered tools later.

## Current scope

This is a **clean MVP foundation**, not a full commercial-grade editor yet.

Included now:
- terrain paint layer
- effect paint layer (Wind / Rain / None)
- deleting effect regions (`Clear Selected FX`, Erase on FX layer, or paint `None`)
- modular render backend file
- modular document model and IO

Deliberately left simple for easy expansion:
- only two logical layers (terrain + effect)
- JSON path entry instead of native file dialogs
- PNG export uses a deterministic CPU preview renderer (runtime editor rendering remains GPU-backed)
- no asset pack importer yet

## Build + run

```bash
cargo run --release
```

## Suggested next upgrades

1. Add chunked dirty-range uploads instead of whole-buffer uploads on edits.
2. Add stacked layer arrays (terrain, decals, props, lighting, FX, masks).
3. Add brush falloff curves and alpha/weight painting.
4. Add asset pack loading and bindless / atlas-based material textures.
5. Add undo/redo command stack.
6. Add tile metadata (collision, difficult terrain, walls, LOS, tags).
7. Add shader hot-reload for moddable effects.
8. Replace CPU PNG exporter with GPU render-to-file output.

## File layout

- `src/main.rs` - small launcher
- `src/app.rs` - egui editor shell and input handling
- `src/model.rs` - map document + editing operations
- `src/io.rs` - save/load/export
- `src/gpu_renderer.rs` - GPU renderer + offscreen render target
- `src/terrain_shader.wgsl` - procedural materials and effects shader
