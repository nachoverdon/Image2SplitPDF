use clap::{Parser, ValueEnum};
use image::DynamicImage;
use std::path::PathBuf;

use crate::image_loader::LoadedImage;
use crate::pdf::{
    save_pdf_from_refs_to_file, FitMode, MarginPreset, Orientation, PageRotation, PageSizePreset,
    PdfConfig,
};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrientationArg {
    Auto,
    Portrait,
    Landscape,
}

impl From<OrientationArg> for Orientation {
    fn from(arg: OrientationArg) -> Self {
        match arg {
            OrientationArg::Auto => Orientation::Auto,
            OrientationArg::Portrait => Orientation::Portrait,
            OrientationArg::Landscape => Orientation::Landscape,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotationArg {
    #[value(name = "0")]
    Deg0,
    #[value(name = "90")]
    Deg90,
    #[value(name = "180")]
    Deg180,
    #[value(name = "270")]
    Deg270,
}

impl From<RotationArg> for PageRotation {
    fn from(arg: RotationArg) -> Self {
        match arg {
            RotationArg::Deg0 => PageRotation::Deg0,
            RotationArg::Deg90 => PageRotation::Deg90,
            RotationArg::Deg180 => PageRotation::Deg180,
            RotationArg::Deg270 => PageRotation::Deg270,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitModeArg {
    Fit,
    Fill,
    Stretch,
    Original,
}

impl From<FitModeArg> for FitMode {
    fn from(arg: FitModeArg) -> Self {
        match arg {
            FitModeArg::Fit => FitMode::Fit,
            FitModeArg::Fill => FitMode::Fill,
            FitModeArg::Stretch => FitMode::Stretch,
            FitModeArg::Original => FitMode::Original,
        }
    }
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "Image2SplitPDF",
    about = "Convert images to split multi-page PDF documents for printing and viewing",
    version,
    after_help = "EXAMPLES:
  # Launch GUI (default when no CLI flags provided)
  image2splitpdf

  # Open GUI with an image pre-loaded
  image2splitpdf photo.jpg

  # Convert a single image to PDF
  image2splitpdf photo.jpg -o photo.pdf

  # Split poster across 2 columns x 3 rows of A4 pages
  image2splitpdf poster.png --split --cols 2 --rows 3 -o poster.pdf

  # Automatically calculate optimal grid pages at 300 DPI
  image2splitpdf artwork.png --auto-grid -o artwork.pdf

  # Auto-grid with custom DPI and overlap margin
  image2splitpdf banner.png --auto-grid 150 --overlap 10 -o banner.pdf

  # Convert multiple images into a single multi-page PDF
  image2splitpdf page1.jpg page2.png page3.webp -o combined.pdf
"
)]
pub struct CliArgs {
    /// Input image file path(s) (JPG, PNG, WebP, BMP, GIF, TIFF, etc.)
    #[arg(value_name = "IMAGES")]
    pub images: Vec<PathBuf>,

    /// Output PDF file path (triggers CLI mode if specified)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Force CLI mode (converts input images without opening GUI)
    #[arg(short, long)]
    pub cli: bool,

    /// Force GUI mode even if other options are specified
    #[arg(short, long)]
    pub gui: bool,

    /// Target page size: A4, A3, A5, Letter, Legal, or custom WIDTHxHEIGHT in mm (e.g. 200x300)
    #[arg(short = 's', long, default_value = "a4", value_name = "SIZE")]
    pub page_size: String,

    /// Page orientation
    #[arg(short = 'r', long, value_enum, default_value_t = OrientationArg::Auto)]
    pub orientation: OrientationArg,

    /// Page rotation degrees in PDF (0, 90, 180, 270)
    #[arg(long, value_enum, default_value_t = RotationArg::Deg0)]
    pub page_rotation: RotationArg,

    /// Rotate input image(s) clockwise before placement (0, 90, 180, 270)
    #[arg(long, value_enum, default_value_t = RotationArg::Deg0)]
    pub image_rotation: RotationArg,

    /// Page margin: none, small (5mm), normal (10mm), large (20mm), or custom mm number (e.g. 15 or 12.5)
    #[arg(short = 'm', long, default_value = "normal", value_name = "MARGIN")]
    pub margin: String,

    /// Image fit mode inside margins
    #[arg(short = 'f', long, value_enum, default_value_t = FitModeArg::Fit)]
    pub fit: FitModeArg,

    /// Enable multi-page split / poster tiling mode
    #[arg(long)]
    pub split: bool,

    /// Number of split grid columns (horizontal pages)
    #[arg(long, default_value_t = 2)]
    pub cols: usize,

    /// Number of split grid rows (vertical pages)
    #[arg(long, default_value_t = 2)]
    pub rows: usize,

    /// Automatically calculate optimal split grid based on target DPI (default: 300)
    #[arg(long, num_args = 0..=1, default_missing_value = "300.0", value_name = "DPI")]
    pub auto_grid: Option<f32>,

    /// Overlap margin in mm between split tiles for cutting/gluing
    #[arg(long, default_value_t = 5.0, value_name = "MM")]
    pub overlap: f32,

    /// Verbose progress output
    #[arg(short, long)]
    pub verbose: bool,
}

impl CliArgs {
    /// Determines whether the program should run in headless CLI mode.
    pub fn should_run_cli(&self) -> bool {
        if self.gui {
            return false;
        }
        self.cli || self.output.is_some() || self.split || self.auto_grid.is_some()
    }

    /// Parse page size argument into preset and custom dimensions
    pub fn parse_page_size(&self) -> Result<(PageSizePreset, f32, f32), String> {
        let s = self.page_size.trim().to_lowercase();
        match s.as_str() {
            "a4" => Ok((PageSizePreset::A4, 210.0, 297.0)),
            "a3" => Ok((PageSizePreset::A3, 297.0, 420.0)),
            "a5" => Ok((PageSizePreset::A5, 148.0, 210.0)),
            "letter" => Ok((PageSizePreset::Letter, 215.9, 279.4)),
            "legal" => Ok((PageSizePreset::Legal, 215.9, 355.6)),
            custom => {
                let parts: Vec<&str> = if custom.contains('x') {
                    custom.split('x').collect()
                } else if custom.contains(',') {
                    custom.split(',').collect()
                } else {
                    return Err(format!(
                        "Invalid page size '{}'. Supported presets: A4, A3, A5, Letter, Legal, or custom WIDTHxHEIGHT in mm (e.g. 210x297)",
                        self.page_size
                    ));
                };

                if parts.len() != 2 {
                    return Err(format!(
                        "Invalid custom dimensions '{}'. Expected format: WIDTHxHEIGHT (e.g. 200x300)",
                        self.page_size
                    ));
                }

                let w: f32 = parts[0].trim().parse().map_err(|_| {
                    format!("Invalid custom width '{}' in page size", parts[0])
                })?;
                let h: f32 = parts[1].trim().parse().map_err(|_| {
                    format!("Invalid custom height '{}' in page size", parts[1])
                })?;

                if w <= 0.0 || h <= 0.0 {
                    return Err("Page dimensions must be greater than 0 mm".to_string());
                }

                Ok((PageSizePreset::Custom, w, h))
            }
        }
    }

    /// Parse margin argument into preset and custom value
    pub fn parse_margin(&self) -> Result<(MarginPreset, f32), String> {
        let s = self.margin.trim().to_lowercase();
        match s.as_str() {
            "none" | "0" | "0mm" => Ok((MarginPreset::None, 0.0)),
            "small" | "5" | "5mm" => Ok((MarginPreset::Small, 5.0)),
            "normal" | "10" | "10mm" => Ok((MarginPreset::Normal, 10.0)),
            "large" | "20" | "20mm" => Ok((MarginPreset::Large, 20.0)),
            num_str => {
                let val_str = num_str.trim_end_matches("mm").trim();
                let mm: f32 = val_str.parse().map_err(|_| {
                    format!(
                        "Invalid margin '{}'. Supported presets: none, small, normal, large, or number in mm (e.g. 15.0)",
                        self.margin
                    )
                })?;
                if mm < 0.0 {
                    return Err("Margin must be non-negative".to_string());
                }
                Ok((MarginPreset::Custom, mm))
            }
        }
    }

    /// Resolve effective destination output path
    pub fn resolve_output_path(&self) -> Result<PathBuf, String> {
        if let Some(ref out) = self.output {
            return Ok(out.clone());
        }

        if let Some(first) = self.images.first() {
            let stem = first.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
            let mut out = first.with_file_name(format!("{}.pdf", stem));
            // Avoid overwriting input if input is somehow named .pdf
            if out == *first {
                out = first.with_file_name(format!("{}_converted.pdf", stem));
            }
            Ok(out)
        } else {
            Ok(PathBuf::from("output.pdf"))
        }
    }

    /// Build `PdfConfig` based on the CLI arguments and first image dimensions
    pub fn build_pdf_config(&self, first_image_dims: Option<(u32, u32)>) -> Result<PdfConfig, String> {
        let (size_preset, custom_w, custom_h) = self.parse_page_size()?;
        let (margin_preset, custom_m) = self.parse_margin()?;

        let mut config = PdfConfig {
            size_preset,
            custom_width_mm: custom_w,
            custom_height_mm: custom_h,
            orientation: self.orientation.into(),
            rotation: self.page_rotation.into(),
            fit_mode: self.fit.into(),
            margin_preset,
            custom_margin_mm: custom_m,
            split_enabled: false,
            split_cols: self.cols.max(1),
            split_rows: self.rows.max(1),
            overlap_mm: self.overlap.max(0.0),
        };

        if let Some(dpi) = self.auto_grid {
            let dpi = if dpi <= 0.0 { 300.0 } else { dpi };
            if let Some((w, h)) = first_image_dims {
                let (cols, rows) = config.calculate_auto_grid(w, h, dpi);
                config.split_enabled = true;
                config.split_cols = cols;
                config.split_rows = rows;
                if self.verbose {
                    eprintln!(
                        "⚡ Auto-grid calculated: {} cols × {} rows ({} pages) for image {}×{} px at {:.0} DPI",
                        cols,
                        rows,
                        cols * rows,
                        w,
                        h,
                        dpi
                    );
                }
            } else {
                config.split_enabled = true;
            }
        } else if self.split {
            config.split_enabled = true;
            config.split_cols = self.cols.max(1);
            config.split_rows = self.rows.max(1);
        }

        Ok(config)
    }
}

/// Execute CLI conversion process
pub fn run_cli(args: &CliArgs) -> Result<(), String> {
    if args.images.is_empty() {
        return Err("No input images specified.\nUsage: image2splitpdf [OPTIONS] <IMAGES>...\nRun with --help for details.".to_string());
    }

    let output_path = args.resolve_output_path()?;

    if args.verbose {
        eprintln!("==> Loading {} input image(s)...", args.images.len());
    }

    let mut loaded_images: Vec<DynamicImage> = Vec::new();

    for path in &args.images {
        if !path.exists() {
            return Err(format!("File not found: '{}'", path.display()));
        }

        if args.verbose {
            eprintln!("    • Reading '{}'...", path.display());
        }

        let loaded = LoadedImage::from_file(path)
            .map_err(|e| format!("Failed to read image '{}': {}", path.display(), e))?;

        let mut img = loaded.working_image;

        // Apply CLI image rotation if specified
        match args.image_rotation {
            RotationArg::Deg0 => {}
            RotationArg::Deg90 => img = img.rotate90(),
            RotationArg::Deg180 => img = img.rotate180(),
            RotationArg::Deg270 => img = img.rotate270(),
        }

        loaded_images.push(img);
    }

    if loaded_images.is_empty() {
        return Err("No valid images could be loaded.".to_string());
    }

    let first_dims = loaded_images.first().map(|img| (img.width(), img.height()));
    let config = args.build_pdf_config(first_dims)?;

    if args.verbose {
        eprintln!(
            "==> Config: Page Size: {:?}, Orientation: {:?}, Margins: {:.1} mm, Fit: {:?}",
            config.size_preset,
            config.orientation,
            config.margin_mm(),
            config.fit_mode
        );
        if config.split_enabled {
            eprintln!(
                "    Split mode: {} cols × {} rows (overlap: {:.1} mm)",
                config.split_cols, config.split_rows, config.overlap_mm
            );
        }
        eprintln!("==> Generating PDF to '{}'...", output_path.display());
    }

    let dyn_refs: Vec<&DynamicImage> = loaded_images.iter().collect();
    let bytes_written = save_pdf_from_refs_to_file(&dyn_refs, &config, &output_path)
        .map_err(|e| format!("Failed to create PDF '{}': {}", output_path.display(), e))?;

    let total_pages = if config.split_enabled {
        config.split_cols * config.split_rows * loaded_images.len()
    } else {
        loaded_images.len()
    };

    println!(
        "✅ Generated '{}' ({:.2} MB, {} page{})",
        output_path.display(),
        bytes_written as f64 / (1024.0 * 1024.0),
        total_pages,
        if total_pages == 1 { "" } else { "s" }
    );

    Ok(())
}
