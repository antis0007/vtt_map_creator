mod app;
mod blend_rules;
mod gpu_renderer;
mod io;
mod model;

use app::AtlasForgeApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Atlas Forge - Minimal GPU Map Creator")
            .with_inner_size([1600.0, 980.0])
            .with_min_inner_size([1100.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Atlas Forge - Minimal GPU Map Creator",
        options,
        Box::new(|cc| Ok(Box::new(AtlasForgeApp::new(cc)))),
    )
}
