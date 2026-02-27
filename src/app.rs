use crate::{
    gpu_renderer::GpuMapRenderer,
    io,
    model::{EditLayer, EffectKind, MapDocument, MaterialKind, Selection, ToolKind},
};
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind};
use std::time::Instant;

pub struct AtlasForgeApp {
    doc: MapDocument,
    renderer: GpuMapRenderer,
    layer: EditLayer,
    tool: ToolKind,
    material: MaterialKind,
    effect: EffectKind,
    brush_radius: u32,
    selection: Selection,
    drag_anchor: Option<[u32; 2]>,
    drag_current: Option<[u32; 2]>,
    primary_drag_active: bool,
    view_origin: [f32; 2],
    zoom: f32,
    gpu_dirty: bool,
    save_path: String,
    export_path: String,
    new_w: u32,
    new_h: u32,
    status: String,
    started_at: Instant,
}

impl AtlasForgeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let doc = MapDocument::new(64, 64);
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("Run with eframe WGPU backend enabled");
        let renderer = GpuMapRenderer::new(render_state, &doc).expect("GPU renderer init failed");
        Self {
            doc,
            renderer,
            layer: EditLayer::Base,
            tool: ToolKind::Brush,
            material: MaterialKind::Grass,
            effect: EffectKind::Wind,
            brush_radius: 2,
            selection: Selection::default(),
            drag_anchor: None,
            drag_current: None,
            primary_drag_active: false,
            view_origin: [0.0, 0.0],
            zoom: 1.0,
            gpu_dirty: true,
            save_path: "autosave.vttmap.json".into(),
            export_path: "export.png".into(),
            new_w: 64,
            new_h: 64,
            status: "Ready".into(),
            started_at: Instant::now(),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Tool:");
            for tool in ToolKind::ALL {
                ui.selectable_value(&mut self.tool, tool, tool.label());
            }
            ui.separator();
            ui.label("Layer:");
            ui.selectable_value(&mut self.layer, EditLayer::Base, EditLayer::Base.label());
            ui.selectable_value(&mut self.layer, EditLayer::Effect, EditLayer::Effect.label());
            ui.separator();
            ui.label("Brush:");
            ui.add(egui::Slider::new(&mut self.brush_radius, 1..=8));
            ui.separator();
            ui.label("Zoom:");
            ui.add(egui::Slider::new(&mut self.zoom, 1.0..=12.0).logarithmic(true));
            ui.separator();
            if ui.button("Reset View").clicked() {
                self.view_origin = [0.0, 0.0];
                self.zoom = 1.0;
            }
        });
    }

    fn side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("controls")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.heading("Materials");
                egui::ComboBox::from_id_salt("material_combo")
                    .selected_text(self.material.label())
                    .show_ui(ui, |ui| {
                        for material in MaterialKind::ALL {
                            ui.selectable_value(&mut self.material, material, material.label());
                        }
                    });

                ui.separator();
                ui.heading("Effect Brush");
                egui::ComboBox::from_id_salt("effect_combo")
                    .selected_text(self.effect.label())
                    .show_ui(ui, |ui| {
                        for effect in EffectKind::ALL {
                            ui.selectable_value(&mut self.effect, effect, effect.label());
                        }
                    });
                ui.label("Tip: set effect to None or use Erase on the Effects layer to remove FX.");

                ui.separator();
                ui.heading("Selection");
                if let Some((a, b)) = self.preview_or_selection_bounds() {
                    ui.label(format!(
                        "Active rect: ({}, {}) → ({}, {})",
                        a[0], a[1], b[0], b[1]
                    ));
                } else {
                    ui.label("No selection");
                }
                ui.horizontal(|ui| {
                    if ui.button("Clear Selection").clicked() {
                        self.selection.clear();
                    }
                    if ui.button("Clear Selected FX").clicked() {
                        self.doc.clear_effects_in_selection(self.selection);
                        self.gpu_dirty = true;
                    }
                });

                ui.separator();
                ui.heading("Map IO");
                ui.label("Map JSON path");
                ui.text_edit_singleline(&mut self.save_path);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        match io::save_map(&self.save_path, &self.doc) {
                            Ok(()) => self.status = format!("Saved {}", self.save_path),
                            Err(err) => self.status = format!("Save failed: {err}"),
                        }
                    }
                    if ui.button("Load").clicked() {
                        match io::load_map(&self.save_path) {
                            Ok(doc) => {
                                self.doc = doc;
                                self.new_w = self.doc.width;
                                self.new_h = self.doc.height;
                                self.selection.clear();
                                self.view_origin = [0.0, 0.0];
                                self.gpu_dirty = true;
                                self.status = format!("Loaded {}", self.save_path);
                            }
                            Err(err) => self.status = format!("Load failed: {err}"),
                        }
                    }
                });
                ui.separator();
                ui.label("Export PNG path");
                ui.text_edit_singleline(&mut self.export_path);
                if ui.button("Export PNG").clicked() {
                    match io::export_png(&self.export_path, &self.doc) {
                        Ok(()) => self.status = format!("Exported {}", self.export_path),
                        Err(err) => self.status = format!("Export failed: {err}"),
                    }
                }

                ui.separator();
                ui.heading("New Map");
                ui.horizontal(|ui| {
                    ui.label("W");
                    ui.add(egui::DragValue::new(&mut self.new_w).range(8..=512));
                    ui.label("H");
                    ui.add(egui::DragValue::new(&mut self.new_h).range(8..=512));
                });
                if ui.button("Create Fresh Map").clicked() {
                    self.doc = MapDocument::new(self.new_w.max(8), self.new_h.max(8));
                    self.view_origin = [0.0, 0.0];
                    self.zoom = 1.0;
                    self.selection.clear();
                    self.gpu_dirty = true;
                    self.status = format!("Created {}x{} map", self.doc.width, self.doc.height);
                }

                ui.separator();
                ui.heading("Controls");
                ui.label("• Mouse wheel: zoom to cursor");
                ui.label("• Middle drag: pan");
                ui.label("• Brush/Erase: paint live while dragging");
                ui.label("• Rect/Select: drag to define region");
                ui.label("• Fill: click once");
            });
    }

    fn preview_or_selection_bounds(&self) -> Option<([u32; 2], [u32; 2])> {
        match (self.drag_anchor, self.drag_current) {
            (Some(a), Some(b)) if matches!(self.tool, ToolKind::Rect | ToolKind::Select) => {
                Some(([a[0].min(b[0]), a[1].min(b[1])], [a[0].max(b[0]), a[1].max(b[1])]))
            }
            _ => self.selection.bounds(),
        }
    }

    fn view_size_tiles(&self) -> [f32; 2] {
        [
            self.doc.width as f32 / self.zoom.max(1.0),
            self.doc.height as f32 / self.zoom.max(1.0),
        ]
    }

    fn clamp_view(&mut self) {
        let view = self.view_size_tiles();
        self.view_origin[0] = self
            .view_origin[0]
            .clamp(0.0, (self.doc.width as f32 - view[0]).max(0.0));
        self.view_origin[1] = self
            .view_origin[1]
            .clamp(0.0, (self.doc.height as f32 - view[1]).max(0.0));
    }

    fn tile_from_pos(&self, image_rect: Rect, pos: Pos2) -> Option<[u32; 2]> {
        if !image_rect.contains(pos) {
            return None;
        }
        let uv = (pos - image_rect.min) / image_rect.size();
        let view = self.view_size_tiles();
        let world = [
            self.view_origin[0] + uv.x * view[0],
            self.view_origin[1] + uv.y * view[1],
        ];
        let tx = world[0].floor() as i32;
        let ty = world[1].floor() as i32;
        if self.doc.contains_i32(tx, ty) {
            Some([tx as u32, ty as u32])
        } else {
            None
        }
    }

    fn world_to_screen(&self, image_rect: Rect, world: [f32; 2]) -> Pos2 {
        let view = self.view_size_tiles();
        let uv = [
            (world[0] - self.view_origin[0]) / view[0].max(0.0001),
            (world[1] - self.view_origin[1]) / view[1].max(0.0001),
        ];
        pos2(
            image_rect.left() + image_rect.width() * uv[0],
            image_rect.top() + image_rect.height() * uv[1],
        )
    }

    fn fit_aspect(&self, outer: Rect, target_px: [u32; 2]) -> Rect {
        let aspect = target_px[0] as f32 / target_px[1] as f32;
        let outer_aspect = outer.width() / outer.height().max(1.0);
        if outer_aspect > aspect {
            let w = outer.height() * aspect;
            let x = outer.center().x - w * 0.5;
            Rect::from_min_size(pos2(x, outer.top()), vec2(w, outer.height()))
        } else {
            let h = outer.width() / aspect;
            let y = outer.center().y - h * 0.5;
            Rect::from_min_size(pos2(outer.left(), y), vec2(outer.width(), h))
        }
    }

    fn apply_brush(&mut self, tile: [u32; 2], erase: bool) {
        if erase {
            let r = self.brush_radius as i32;
            let [cx, cy] = tile;
            let rr = (self.brush_radius * self.brush_radius) as i32;
            for y in (cy as i32 - r).max(0)..=(cy as i32 + r).min(self.doc.height as i32 - 1) {
                for x in (cx as i32 - r).max(0)..=(cx as i32 + r).min(self.doc.width as i32 - 1) {
                    let dx = x - cx as i32;
                    let dy = y - cy as i32;
                    if dx * dx + dy * dy <= rr {
                        self.doc.erase_one(x as u32, y as u32, self.layer);
                    }
                }
            }
        } else {
            self.doc.paint_disc(tile, self.brush_radius, self.layer, self.material, self.effect);
        }
        self.gpu_dirty = true;
    }

    fn begin_primary_action(&mut self, tile: Option<[u32; 2]>) {
        let Some(tile) = tile else {
            return;
        };
        self.primary_drag_active = true;
        self.drag_anchor = Some(tile);
        self.drag_current = Some(tile);
        match self.tool {
            ToolKind::Brush => self.apply_brush(tile, false),
            ToolKind::Erase => self.apply_brush(tile, true),
            ToolKind::Fill => {
                self.doc.fill(tile, self.layer, self.material, self.effect);
                self.gpu_dirty = true;
                self.primary_drag_active = false;
                self.drag_anchor = None;
                self.drag_current = None;
            }
            ToolKind::Rect => {}
            ToolKind::Select => {
                self.selection.anchor = Some(tile);
                self.selection.focus = Some(tile);
            }
        }
    }

    fn update_primary_drag(&mut self, tile: Option<[u32; 2]>) {
        if !self.primary_drag_active {
            return;
        }
        let Some(tile) = tile else {
            return;
        };
        self.drag_current = Some(tile);
        match self.tool {
            ToolKind::Brush => self.apply_brush(tile, false),
            ToolKind::Erase => self.apply_brush(tile, true),
            ToolKind::Select => {
                self.selection.focus = Some(tile);
            }
            ToolKind::Fill | ToolKind::Rect => {}
        }
    }

    fn end_primary_action(&mut self) {
        if !self.primary_drag_active {
            return;
        }
        if let (Some(a), Some(b)) = (self.drag_anchor, self.drag_current) {
            match self.tool {
                ToolKind::Rect => {
                    self.doc.paint_rect(a, b, self.layer, self.material, self.effect);
                    self.gpu_dirty = true;
                }
                ToolKind::Select => {
                    self.selection.anchor = Some(a);
                    self.selection.focus = Some(b);
                }
                ToolKind::Brush | ToolKind::Fill | ToolKind::Erase => {}
            }
        }
        self.primary_drag_active = false;
        self.drag_anchor = None;
        self.drag_current = None;
    }

    fn zoom_to_cursor(&mut self, image_rect: Rect, cursor: Pos2, scroll: f32) {
        let focus_uv = (cursor - image_rect.min) / image_rect.size();
        let before_view = self.view_size_tiles();
        let focus_world = [
            self.view_origin[0] + before_view[0] * focus_uv.x,
            self.view_origin[1] + before_view[1] * focus_uv.y,
        ];
        let scale = if scroll > 0.0 { 1.12 } else { 1.0 / 1.12 };
        self.zoom = (self.zoom * scale).clamp(1.0, 12.0);
        let after_view = self.view_size_tiles();
        self.view_origin = [
            focus_world[0] - after_view[0] * focus_uv.x,
            focus_world[1] - after_view[1] * focus_uv.y,
        ];
        self.clamp_view();
    }

    fn canvas(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let (response, painter) = ui.allocate_painter(avail, Sense::click_and_drag());
        let image_rect = self.fit_aspect(response.rect, self.renderer.target_px());

        if response.hovered() {
            let scroll = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                if let Some(pos) = ctx.pointer_hover_pos() {
                    if image_rect.contains(pos) {
                        self.zoom_to_cursor(image_rect, pos, scroll);
                    }
                }
            }
        }

        if response.hovered() && ctx.input(|i| i.pointer.middle_down()) {
            let delta = ctx.input(|i| i.pointer.delta());
            let view = self.view_size_tiles();
            if delta != egui::Vec2::ZERO {
                self.view_origin[0] -= delta.x / image_rect.width().max(1.0) * view[0];
                self.view_origin[1] -= delta.y / image_rect.height().max(1.0) * view[1];
                self.clamp_view();
            }
        }

        let hover_tile = ctx
            .pointer_hover_pos()
            .and_then(|pos| self.tile_from_pos(image_rect, pos));

        let primary_pressed = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let primary_released = ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));

        if primary_pressed && response.hovered() {
            self.begin_primary_action(hover_tile);
        }
        if primary_down && response.hovered() {
            self.update_primary_drag(hover_tile);
        }
        if primary_released {
            self.end_primary_action();
        }

        self.renderer.render(
            &self.doc,
            self.gpu_dirty,
            self.view_origin,
            self.view_size_tiles(),
            self.started_at.elapsed().as_secs_f32(),
        );
        self.gpu_dirty = false;

        painter.rect_filled(response.rect, 6.0, Color32::from_rgb(18, 21, 27));
        painter.image(
            self.renderer.texture_id(),
            image_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.rect_stroke(
            image_rect,
            6.0,
            Stroke::new(1.0, Color32::from_rgb(58, 64, 76)),
            StrokeKind::Middle,
        );

        if let Some((a, b)) = self.preview_or_selection_bounds() {
            let min = self.world_to_screen(image_rect, [a[0] as f32, a[1] as f32]);
            let max = self.world_to_screen(image_rect, [b[0] as f32 + 1.0, b[1] as f32 + 1.0]);
            let rect = Rect::from_min_max(min, max);
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(2.0, Color32::from_rgb(120, 190, 255)),
                StrokeKind::Inside,
            );
        }

        if let Some(tile) = hover_tile {
            let min = self.world_to_screen(image_rect, [tile[0] as f32, tile[1] as f32]);
            let max = self.world_to_screen(image_rect, [tile[0] as f32 + 1.0, tile[1] as f32 + 1.0]);
            painter.rect_stroke(
                Rect::from_min_max(min, max),
                0.0,
                Stroke::new(1.0, Color32::from_white_alpha(160)),
                StrokeKind::Inside,
            );
            self.status = format!(
                "Tile {}, {} | Tool: {} | Layer: {}",
                tile[0],
                tile[1],
                self.tool.label(),
                self.layer.label()
            );
        }
    }
}

impl eframe::App for AtlasForgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.clamp_view();
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
        self.side_panel(ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });
        egui::CentralPanel::default().show(ctx, |ui| self.canvas(ctx, ui));
        ctx.request_repaint();
    }
}
