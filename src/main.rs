#![allow(non_snake_case, dead_code)]

mod app;
mod image_loader;
mod pdf;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Image2SplitPDF — Convert Images to PDF")
            .with_inner_size([1150.0, 750.0])
            .with_min_inner_size([750.0, 500.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    // Check if user passed an image argument on CLI
    let cli_arg_path = std::env::args().nth(1).map(PathBuf::from);

    eframe::run_native(
        "Image2SplitPDF",
        options,
        Box::new(move |cc| {
            let mut app = app::Image2PdfApp::new(cc);
            if let Some(path) = cli_arg_path {
                if path.exists() {
                    app.load_image_path(path);
                }
            }
            Ok(Box::new(app))
        }),
    )
}
