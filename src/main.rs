#![allow(non_snake_case, dead_code)]

mod app;
mod cli;
mod image_loader;
mod pdf;

use clap::Parser;
use eframe::egui;

fn main() -> eframe::Result {
    let args = cli::CliArgs::parse();

    if args.should_run_cli() {
        if let Err(err) = cli::run_cli(&args) {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Image2SplitPDF — Convert Images to PDF")
            .with_inner_size([1150.0, 750.0])
            .with_min_inner_size([750.0, 500.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    let initial_images = args.images;

    eframe::run_native(
        "Image2SplitPDF",
        options,
        Box::new(move |cc| {
            let mut app = app::Image2PdfApp::new(cc);
            for path in initial_images {
                if path.exists() {
                    app.load_image_path(path);
                }
            }
            Ok(Box::new(app))
        }),
    )
}
