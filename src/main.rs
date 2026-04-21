mod app;
mod core;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("pfiles")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "pfiles",
        native_options,
        Box::new(|cc| Ok(Box::new(app::PFilesApp::new(cc)))),
    )
}
