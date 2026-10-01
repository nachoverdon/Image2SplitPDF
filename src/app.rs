use crate::image_loader::LoadedImage;
use crate::pdf::{
    save_pdf_from_refs_to_file, FitMode, MarginPreset, Orientation, PageRotation, PageSizePreset,
    PdfConfig,
};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontFamily, FontId, Layout, Painter, Rect,
    Sense, Shape, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui,
};
use std::path::PathBuf;

const MM_TO_PT: f32 = 72.0 / 25.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitPreviewMode {
    PosterGrid,
    SingleTile,
}

pub struct Image2PdfApp {
    // Loaded images
    pub images: Vec<LoadedImage>,
    pub selected_image_idx: usize,
    pub texture_cache: Option<(usize, u32, TextureHandle)>, // (img_idx, rotation, handle)
    pub tile_texture_cache: Option<(usize, usize, usize, TextureHandle)>, // (img_idx, col, row, handle)

    // Configuration
    pub config: PdfConfig,
    pub convert_all_images: bool, // false = selected image only, true = all loaded images
    pub auto_grid_dpi: f32,       // Target DPI for auto-adjusting page grid based on image size

    // Preview state
    pub zoom: f32,
    pub show_margins: bool,
    pub split_preview_mode: SplitPreviewMode,
    pub selected_tile_idx: usize,

    // Status / Notification
    pub status_message: Option<(String, bool)>, // (text, is_error)
    pub last_saved_path: Option<PathBuf>,
}

impl Default for Image2PdfApp {
    fn default() -> Self {
        Self {
            images: Vec::new(),
            selected_image_idx: 0,
            texture_cache: None,
            tile_texture_cache: None,
            config: PdfConfig::default(),
            convert_all_images: false,
            auto_grid_dpi: 300.0,
            zoom: 1.0,
            show_margins: true,
            split_preview_mode: SplitPreviewMode::PosterGrid,
            selected_tile_idx: 0,
            status_message: None,
            last_saved_path: None,
        }
    }
}

impl Image2PdfApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    pub fn load_image_path(&mut self, path: PathBuf) {
        match LoadedImage::from_file(&path) {
            Ok(img) => {
                self.images.push(img);
                self.selected_image_idx = self.images.len() - 1;
                self.texture_cache = None;
                self.tile_texture_cache = None;
                self.status_message = Some((
                    format!(
                        "Loaded image: {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    false,
                ));
            }
            Err(e) => {
                self.status_message = Some((format!("Error loading image: {}", e), true));
            }
        }
    }

    pub fn load_image_bytes(&mut self, name: &str, bytes: &[u8]) {
        match LoadedImage::from_memory(bytes, name) {
            Ok(img) => {
                self.images.push(img);
                self.selected_image_idx = self.images.len() - 1;
                self.texture_cache = None;
                self.tile_texture_cache = None;
                self.status_message = Some((format!("Loaded image: {}", name), false));
            }
            Err(e) => {
                self.status_message = Some((format!("Error decoding image: {}", e), true));
            }
        }
    }

    pub fn current_image(&self) -> Option<&LoadedImage> {
        self.images.get(self.selected_image_idx)
    }

    pub fn current_image_mut(&mut self) -> Option<&mut LoadedImage> {
        self.images.get_mut(self.selected_image_idx)
    }

    /// Automatically adjust the grid of pages (columns & rows) required to print
    /// the currently selected image at `auto_grid_dpi` (defaults to 300 DPI)
    /// across split pages.
    pub fn auto_adjust_grid(&mut self) {
        let (img_w, img_h) = match self.current_image() {
            Some(img) => (img.width(), img.height()),
            None => {
                self.status_message = Some((
                    "Cannot adjust grid: please load an image first.".to_string(),
                    true,
                ));
                return;
            }
        };

        let (cols, rows) = self
            .config
            .calculate_auto_grid(img_w, img_h, self.auto_grid_dpi);
        self.config.split_cols = cols;
        self.config.split_rows = rows;
        self.config.split_enabled = true;
        self.tile_texture_cache = None;

        let total_pages = self.config.total_pages();
        if self.selected_tile_idx >= total_pages {
            self.selected_tile_idx = total_pages.saturating_sub(1);
        }

        self.status_message = Some((
            format!(
                "Auto-adjusted grid to {} cols × {} rows ({} {} total) for {} × {} px image at {:.0} DPI.",
                cols,
                rows,
                total_pages,
                if total_pages == 1 { "page" } else { "pages" },
                img_w,
                img_h,
                self.auto_grid_dpi,
            ),
            false,
        ));
    }

    fn ensure_texture(&mut self, ctx: &egui::Context) -> Option<&TextureHandle> {
        let (idx, img) = match self.images.get(self.selected_image_idx) {
            Some(img) => (self.selected_image_idx, img),
            None => return None,
        };

        let rot = img.image_rotation;
        let needs_reload = match &self.texture_cache {
            Some((c_idx, c_rot, _)) => *c_idx != idx || *c_rot != rot,
            None => true,
        };

        if needs_reload {
            let color_image = img.to_egui_color_image();
            let handle = ctx.load_texture(
                format!("img_{}_{}", idx, rot),
                color_image,
                TextureOptions::LINEAR,
            );
            self.texture_cache = Some((idx, rot, handle));
        }

        self.texture_cache.as_ref().map(|(_, _, h)| h)
    }

    fn open_file_dialog(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Open Images")
            .add_filter(
                "Image Files (*.jpg, *.png, *.bmp, *.webp, *.gif, *.tiff, *.ico)",
                &[
                    "jpg", "jpeg", "png", "bmp", "webp", "gif", "tiff", "tif", "ico", "avif",
                    "qoi",
                ],
            )
            .pick_files()
        {
            for file in files {
                self.load_image_path(file);
            }
        }
    }

    fn export_pdf_dialog(&mut self) {
        if self.images.is_empty() {
            self.status_message = Some(("Please load at least one image first.".to_string(), true));
            return;
        }

        let default_name = if self.images.len() == 1 || !self.convert_all_images {
            let base = self
                .current_image()
                .map(|img| {
                    PathBuf::from(&img.filename)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "document".to_string())
                })
                .unwrap_or_else(|| "document".to_string());
            format!("{}.pdf", base)
        } else {
            "converted_images.pdf".to_string()
        };

        if let Some(dest) = rfd::FileDialog::new()
            .set_title("Export PDF Document")
            .set_file_name(&default_name)
            .add_filter("PDF Document (*.pdf)", &["pdf"])
            .save_file()
        {
            let imgs_to_convert: Vec<&image::DynamicImage> = if self.convert_all_images {
                self.images.iter().map(|i| &i.working_image).collect()
            } else if let Some(cur) = self.current_image() {
                vec![&cur.working_image]
            } else {
                vec![]
            };

            match save_pdf_from_refs_to_file(&imgs_to_convert, &self.config, &dest) {
                Ok(bytes_written) => {
                    let size_kb = bytes_written as f64 / 1024.0;
                    let total_pages = self.config.total_pages() * imgs_to_convert.len();
                    self.status_message = Some((
                        format!(
                            "Successfully saved PDF to {} ({:.1} KB, {} page{})",
                            dest.display(),
                            size_kb,
                            total_pages,
                            if total_pages == 1 { "" } else { "s" }
                        ),
                        false,
                    ));
                    self.last_saved_path = Some(dest);
                }
                Err(e) => {
                    self.status_message = Some((format!("Failed to export PDF: {}", e), true));
                }
            }
        }
    }

    fn open_saved_pdf(&self) {
        if let Some(path) = &self.last_saved_path {
            let _ = std::process::Command::new("xdg-open").arg(path).spawn();
        }
    }

    fn handle_drag_and_drop(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            let path = file.path();
            if path.exists() {
                self.load_image_path(path.to_path_buf());
            } else if let Ok(bytes) = file.bytes() {
                let name = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| "dropped_image".to_string());
                self.load_image_bytes(&name, &bytes);
            }
        }
    }

    // UI rendering helpers
    fn render_top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("📄 Image2SplitPDF");
            ui.add_space(16.0);

            if ui
                .button("📂 Open Image(s)...")
                .on_hover_text("Open images from disk (JPG, PNG, WebP, BMP, etc.)")
                .clicked()
            {
                self.open_file_dialog();
            }

            if !self.images.is_empty() {
                if ui
                    .button("🗑 Clear All")
                    .on_hover_text("Remove all loaded images")
                    .clicked()
                {
                    self.images.clear();
                    self.selected_image_idx = 0;
                    self.texture_cache = None;
                    self.tile_texture_cache = None;
                    self.status_message = None;
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let export_btn = egui::Button::new("💾 Export PDF...")
                    .fill(Color32::from_rgb(46, 125, 50)); // rich green accent
                if ui
                    .add_enabled(!self.images.is_empty(), export_btn)
                    .on_hover_text("Convert and save configured pages as PDF")
                    .clicked()
                {
                    self.export_pdf_dialog();
                }

                if self.images.len() > 1 {
                    ui.checkbox(
                        &mut self.convert_all_images,
                        format!("Convert All ({})", self.images.len()),
                    );
                }
            });
        });
    }

    fn render_sidebar(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_width(320.0);

            // SECTION 1: Selected Image Details
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong("🖼 Selected Image");
                    if let Some(img) = self.current_image() {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(&img.format_name)
                                    .small()
                                    .color(Color32::from_gray(160)),
                            );
                        });
                    }
                });

                if self.images.is_empty() {
                    ui.label(
                        egui::RichText::new("No image loaded.\nDrop a file or click Open.")
                            .italics()
                            .color(Color32::from_gray(140)),
                    );
                } else {
                    if self.images.len() > 1 {
                        ui.horizontal(|ui| {
                            ui.label("Image:");
                            egui::ComboBox::from_id_salt("image_selector")
                                .selected_text(&self.images[self.selected_image_idx].filename)
                                .show_ui(ui, |ui| {
                                    for (i, img) in self.images.iter().enumerate() {
                                        if ui
                                            .selectable_value(
                                                &mut self.selected_image_idx,
                                                i,
                                                format!("{}. {}", i + 1, img.filename),
                                            )
                                            .clicked()
                                        {
                                            self.texture_cache = None;
                                            self.tile_texture_cache = None;
                                        }
                                    }
                                });
                        });
                    }

                    if let Some(img) = self.current_image() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Dimensions: {} × {} px", img.width(), img.height()));
                        });
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "Ratio: {:.2}:1 • Size: {}",
                                img.aspect_ratio(),
                                img.formatted_size()
                            ));
                        });

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Image Rotation:");
                            if ui.button("⟳ Rotate 90°").clicked() {
                                if let Some(img) = self.current_image_mut() {
                                    img.rotate_cw();
                                    self.texture_cache = None;
                                    self.tile_texture_cache = None;
                                }
                            }
                            if let Some(img) = self.current_image() {
                                ui.label(format!("{}°", img.image_rotation));
                            }
                        });
                    }
                }
            });

            ui.add_space(8.0);

            // SECTION 2: Page Size
            ui.group(|ui| {
                ui.strong("📐 Page Size");
                ui.add_space(2.0);

                egui::ComboBox::from_id_salt("page_size_preset")
                    .selected_text(self.config.size_preset.name())
                    .show_ui(ui, |ui| {
                        for preset in PageSizePreset::ALL {
                            ui.selectable_value(
                                &mut self.config.size_preset,
                                preset,
                                preset.name(),
                            );
                        }
                    });

                if self.config.size_preset == PageSizePreset::Custom {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Width (mm):");
                        ui.add(
                            egui::DragValue::new(&mut self.config.custom_width_mm)
                                .range(20.0..=2000.0)
                                .speed(1.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Height (mm):");
                        ui.add(
                            egui::DragValue::new(&mut self.config.custom_height_mm)
                                .range(20.0..=2000.0)
                                .speed(1.0),
                        );
                    });
                }
            });

            ui.add_space(8.0);

            // SECTION 3: Orientation & Page Rotation
            ui.group(|ui| {
                ui.strong("🔄 Page Orientation & Rotation");
                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    ui.label("Orientation:");
                    for orient in Orientation::ALL {
                        ui.selectable_value(
                            &mut self.config.orientation,
                            orient,
                            match orient {
                                Orientation::Auto => "Auto",
                                Orientation::Portrait => "Portrait",
                                Orientation::Landscape => "Landscape",
                            },
                        );
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Page Rotation:");
                    for rot in PageRotation::ALL {
                        ui.selectable_value(
                            &mut self.config.rotation,
                            rot,
                            format!("{}°", rot.degrees()),
                        );
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("⟳ Turn Page 90°").clicked() {
                        self.config.rotation = self.config.rotation.rotate_cw();
                    }
                });
            });

            ui.add_space(8.0);

            // SECTION 4: Margins & Fit Mode
            ui.group(|ui| {
                ui.strong("📏 Margins & Scaling");
                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    ui.label("Fit Mode:");
                    egui::ComboBox::from_id_salt("fit_mode_selector")
                        .selected_text(self.config.fit_mode.name())
                        .show_ui(ui, |ui| {
                            for mode in FitMode::ALL {
                                ui.selectable_value(&mut self.config.fit_mode, mode, mode.name());
                            }
                        });
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Margin:");
                    egui::ComboBox::from_id_salt("margin_preset_selector")
                        .selected_text(self.config.margin_preset.name())
                        .show_ui(ui, |ui| {
                            for preset in MarginPreset::ALL {
                                ui.selectable_value(
                                    &mut self.config.margin_preset,
                                    preset,
                                    preset.name(),
                                );
                            }
                        });
                });

                if self.config.margin_preset == MarginPreset::Custom {
                    ui.horizontal(|ui| {
                        ui.label("Custom Margin (mm):");
                        ui.add(
                            egui::DragValue::new(&mut self.config.custom_margin_mm)
                                .range(0.0..=100.0)
                                .speed(0.5),
                        );
                    });
                }

                ui.checkbox(&mut self.show_margins, "Show margin guidelines");
            });

            ui.add_space(8.0);

            // SECTION 5: Split Image / Poster Printing
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut self.config.split_enabled,
                        egui::RichText::new("✂ Split Image (Poster Tiling)").strong(),
                    );
                    if !self.config.split_enabled {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let auto_btn = egui::Button::new("⚡ Auto Grid")
                                .fill(Color32::from_rgb(30, 136, 229));
                            if ui
                                .add_enabled(self.current_image().is_some(), auto_btn)
                                .on_hover_text(
                                    "Enable poster splitting and automatically adjust grid based on image size",
                                )
                                .clicked()
                            {
                                self.auto_adjust_grid();
                            }
                        });
                    }
                });

                if self.config.split_enabled {
                    ui.add_space(4.0);

                    // Auto-adjust button and target DPI controls
                    ui.horizontal(|ui| {
                        let auto_btn = egui::Button::new("⚡ Auto-Adjust Grid to Image Size")
                            .fill(Color32::from_rgb(30, 136, 229));
                        if ui
                            .add_enabled(self.current_image().is_some(), auto_btn)
                            .on_hover_text(
                                "Automatically calculate required columns and rows based on image dimensions, printable page area, and target DPI",
                            )
                            .clicked()
                        {
                            self.auto_adjust_grid();
                        }
                    });

                    if let Some(img) = self.current_image() {
                        let (s_cols, s_rows) =
                            self.config.calculate_auto_grid(img.width(), img.height(), self.auto_grid_dpi);
                        let s_pages = s_cols * s_rows;
                        ui.label(
                            egui::RichText::new(format!(
                                "Calculated for {} × {} px: {} cols × {} rows ({} {})",
                                img.width(),
                                img.height(),
                                s_cols,
                                s_rows,
                                s_pages,
                                if s_pages == 1 { "page" } else { "pages" }
                            ))
                            .small()
                            .color(Color32::from_gray(160)),
                        );
                    }

                    ui.horizontal(|ui| {
                        ui.label("Target DPI:");
                        ui.add(
                            egui::DragValue::new(&mut self.auto_grid_dpi)
                                .range(30.0..=600.0)
                                .speed(5.0)
                                .suffix(" DPI"),
                        )
                        .on_hover_text("Target resolution for grid calculation (300 DPI = high quality print, 150 DPI = standard poster)");

                        if ui.small_button("300 DPI").on_hover_text("300 DPI (Standard print quality)").clicked() {
                            self.auto_grid_dpi = 300.0;
                            self.auto_adjust_grid();
                        }
                        if ui.small_button("150 DPI").on_hover_text("150 DPI (Poster quality)").clicked() {
                            self.auto_grid_dpi = 150.0;
                            self.auto_adjust_grid();
                        }
                    });

                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label("Grid Columns:");
                        ui.add(
                            egui::DragValue::new(&mut self.config.split_cols)
                                .range(1..=25)
                                .speed(0.1),
                        );
                        ui.label("Rows:");
                        ui.add(
                            egui::DragValue::new(&mut self.config.split_rows)
                                .range(1..=25)
                                .speed(0.1),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.label("Presets:");
                        if ui.button("1×2").clicked() {
                            self.config.split_cols = 1;
                            self.config.split_rows = 2;
                            self.tile_texture_cache = None;
                        }
                        if ui.button("2×1").clicked() {
                            self.config.split_cols = 2;
                            self.config.split_rows = 1;
                            self.tile_texture_cache = None;
                        }
                        if ui.button("2×2").clicked() {
                            self.config.split_cols = 2;
                            self.config.split_rows = 2;
                            self.tile_texture_cache = None;
                        }
                        if ui.button("3×3").clicked() {
                            self.config.split_cols = 3;
                            self.config.split_rows = 3;
                            self.tile_texture_cache = None;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Overlap (mm):");
                        ui.add(
                            egui::DragValue::new(&mut self.config.overlap_mm)
                                .range(0.0..=50.0)
                                .speed(0.5),
                        )
                        .on_hover_text("Overlap margin between tiles for trimming & gluing");
                    });

                    let total_tiles = self.config.total_pages();
                    ui.label(
                        egui::RichText::new(format!(
                            "Total Output: {} pages ({} cols × {} rows)",
                            total_tiles, self.config.split_cols, self.config.split_rows
                        ))
                        .color(Color32::from_rgb(100, 180, 246)),
                    );

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Preview View:");
                        ui.selectable_value(
                            &mut self.split_preview_mode,
                            SplitPreviewMode::PosterGrid,
                            "Poster Grid",
                        );
                        ui.selectable_value(
                            &mut self.split_preview_mode,
                            SplitPreviewMode::SingleTile,
                            "Single Page",
                        );
                    });

                    if self.split_preview_mode == SplitPreviewMode::SingleTile {
                        let total = self.config.total_pages();
                        ui.horizontal(|ui| {
                            if ui.button("◀").clicked() && self.selected_tile_idx > 0 {
                                self.selected_tile_idx -= 1;
                            }
                            ui.label(format!("Page {} of {}", self.selected_tile_idx + 1, total));
                            if ui.button("▶").clicked() && self.selected_tile_idx + 1 < total {
                                self.selected_tile_idx += 1;
                            }
                        });
                    }
                }
            });

            // Notification / Status
            if let Some((msg, is_error)) = &self.status_message {
                ui.add_space(8.0);
                let color = if *is_error {
                    Color32::from_rgb(239, 83, 80)
                } else {
                    Color32::from_rgb(76, 175, 80)
                };
                ui.group(|ui| {
                    ui.label(egui::RichText::new(msg).color(color));
                    if self.last_saved_path.is_some() && !*is_error {
                        ui.horizontal(|ui| {
                            if ui.button("🔍 Open PDF").clicked() {
                                self.open_saved_pdf();
                            }
                        });
                    }
                });
            }
        });
    }

    fn render_preview_canvas(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let (rect, _response) =
            ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let painter = ui.painter_at(rect);

        // Draw neutral canvas background
        let bg_color = if ui.visuals().dark_mode {
            Color32::from_rgb(28, 30, 34)
        } else {
            Color32::from_rgb(230, 233, 238)
        };
        painter.rect_filled(rect, 0.0, bg_color);

        let img = match self.current_image() {
            Some(img) => img,
            None => {
                // Empty state: Draw nice drop zone prompt
                let center = rect.center();
                let card_rect = Rect::from_center_size(center, vec2(420.0, 240.0));
                let card_bg = if ui.visuals().dark_mode {
                    Color32::from_rgb(38, 41, 46)
                } else {
                    Color32::WHITE
                };
                painter.rect_filled(card_rect, 10.0, card_bg);
                painter.rect_stroke(
                    card_rect,
                    10.0,
                    Stroke::new(
                        2.0,
                        if ui.visuals().dark_mode {
                            Color32::from_rgb(60, 64, 72)
                        } else {
                            Color32::from_rgb(200, 205, 212)
                        },
                    ),
                    StrokeKind::Inside,
                );

                painter.text(
                    pos2(center.x, center.y - 45.0),
                    Align2::CENTER_CENTER,
                    "📥 Drag & Drop Image Here",
                    FontId::new(20.0, FontFamily::Proportional),
                    if ui.visuals().dark_mode {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(40, 44, 52)
                    },
                );

                painter.text(
                    pos2(center.x, center.y - 10.0),
                    Align2::CENTER_CENTER,
                    "Supports JPG, PNG, WebP, BMP, GIF, TIFF, and more",
                    FontId::new(13.0, FontFamily::Proportional),
                    Color32::from_gray(140),
                );

                painter.text(
                    pos2(center.x, center.y + 35.0),
                    Align2::CENTER_CENTER,
                    "— or click 'Open Image(s)...' in the top bar —",
                    FontId::new(12.0, FontFamily::Proportional),
                    Color32::from_rgb(66, 133, 244),
                );

                return;
            }
        };

        let (img_w, img_h) = (img.width(), img.height());
        let (page_w_pt, page_h_pt) = self.config.resolved_page_size_pt(img_w, img_h);

        // Calculate sheet visual size on screen based on page rotation
        // If rotated 90 or 270, visual aspect ratio is inverted
        let is_visual_transposed = match self.config.rotation {
            PageRotation::Deg90 | PageRotation::Deg270 => true,
            _ => false,
        };

        let (visual_w_pt, visual_h_pt) = if is_visual_transposed {
            (page_h_pt, page_w_pt)
        } else {
            (page_w_pt, page_h_pt)
        };

        // Scale to fit available canvas space with 40px margin
        let pad = 50.0;
        let scale_w = (rect.width() - pad) / visual_w_pt;
        let scale_h = (rect.height() - pad) / visual_h_pt;
        let scale = scale_w.min(scale_h).max(0.05) * self.zoom;

        let screen_w = visual_w_pt * scale;
        let screen_h = visual_h_pt * scale;
        let sheet_rect = Rect::from_center_size(rect.center(), vec2(screen_w, screen_h));

        // Draw drop shadow
        let shadow_rect = sheet_rect.translate(vec2(5.0, 7.0));
        painter.rect_filled(shadow_rect, 4.0, Color32::from_black_alpha(45));

        // Draw white paper sheet
        painter.rect_filled(sheet_rect, 2.0, Color32::WHITE);
        painter.rect_stroke(
            sheet_rect,
            2.0,
            Stroke::new(1.0, Color32::from_rgb(180, 185, 190)),
            StrokeKind::Inside,
        );

        // Printable margin guideline
        let margin_pt = self.config.margin_mm() * MM_TO_PT;
        let visual_margin_x = margin_pt * scale;
        let visual_margin_y = margin_pt * scale;
        let printable_rect = sheet_rect.shrink2(vec2(visual_margin_x, visual_margin_y));

        if self.show_margins && margin_pt > 0.5 {
            painter.rect_stroke(
                printable_rect,
                0.0,
                Stroke::new(
                    1.0,
                    Color32::from_rgba_unmultiplied(66, 133, 244, 130),
                ),
                StrokeKind::Inside,
            );
        }

        // Draw Image Content
        let texture = match self.ensure_texture(ctx) {
            Some(t) => t,
            None => return,
        };

        let texture_id = texture.id();

        if self.config.split_enabled && self.split_preview_mode == SplitPreviewMode::PosterGrid {
            // Render full poster on page with grid cutlines overlaid
            let (draw_x_pt, draw_y_pt, draw_w_pt, draw_h_pt) = self
                .config
                .calculate_image_placement(page_w_pt, page_h_pt, img_w, img_h);

            let (img_screen_rect, uv_rect) = self.compute_screen_rect_and_uv(
                sheet_rect,
                scale,
                draw_x_pt,
                draw_y_pt,
                draw_w_pt,
                draw_h_pt,
                page_h_pt,
                page_w_pt,
                is_visual_transposed,
            );

            // Clip to printable area if Fill mode
            let clip_rect = if self.config.fit_mode == FitMode::Fill {
                printable_rect
            } else {
                sheet_rect
            };

            let clipped_painter = painter.with_clip_rect(clip_rect);
            self.draw_textured_quad(&clipped_painter, texture_id, img_screen_rect, uv_rect);

            // Draw grid cut lines across the image
            let cols = self.config.split_cols.max(1);
            let rows = self.config.split_rows.max(1);
            let grid_stroke = Stroke::new(2.0, Color32::from_rgb(230, 81, 0)); // bright orange

            for c in 1..cols {
                let frac = c as f32 / cols as f32;
                let x = img_screen_rect.min.x + img_screen_rect.width() * frac;
                clipped_painter.line_segment(
                    [pos2(x, img_screen_rect.min.y), pos2(x, img_screen_rect.max.y)],
                    grid_stroke,
                );
            }

            for r in 1..rows {
                let frac = r as f32 / rows as f32;
                let y = img_screen_rect.min.y + img_screen_rect.height() * frac;
                clipped_painter.line_segment(
                    [pos2(img_screen_rect.min.x, y), pos2(img_screen_rect.max.x, y)],
                    grid_stroke,
                );
            }

            // Draw tile labels (Page 1, Page 2, ...)
            let tile_w = img_screen_rect.width() / cols as f32;
            let tile_h = img_screen_rect.height() / rows as f32;
            let mut page_num = 1;
            for r in 0..rows {
                for c in 0..cols {
                    let tile_center = pos2(
                        img_screen_rect.min.x + (c as f32 + 0.5) * tile_w,
                        img_screen_rect.min.y + (r as f32 + 0.5) * tile_h,
                    );
                    let label_bg = Rect::from_center_size(tile_center, vec2(56.0, 24.0));
                    clipped_painter.rect_filled(
                        label_bg,
                        4.0,
                        Color32::from_black_alpha(180),
                    );
                    clipped_painter.text(
                        tile_center,
                        Align2::CENTER_CENTER,
                        format!("Page {}", page_num),
                        FontId::new(12.0, FontFamily::Proportional),
                        Color32::WHITE,
                    );
                    page_num += 1;
                }
            }
        } else if self.config.split_enabled
            && self.split_preview_mode == SplitPreviewMode::SingleTile
        {
            // Render specific tile on this single page
            let cols = self.config.split_cols.max(1);
            let rows = self.config.split_rows.max(1);
            let total = cols * rows;
            let tile_idx = self.selected_tile_idx.min(total.saturating_sub(1));
            let c = tile_idx % cols;
            let r = tile_idx / cols;

            let u0 = c as f32 / cols as f32;
            let u1 = (c + 1) as f32 / cols as f32;
            let v0 = r as f32 / rows as f32;
            let v1 = (r + 1) as f32 / rows as f32;

            let tile_w_px = (img_w as f32 / cols as f32).round() as u32;
            let tile_h_px = (img_h as f32 / rows as f32).round() as u32;

            let (draw_x_pt, draw_y_pt, draw_w_pt, draw_h_pt) = self
                .config
                .calculate_image_placement(page_w_pt, page_h_pt, tile_w_px, tile_h_px);

            let (img_screen_rect, base_uv) = self.compute_screen_rect_and_uv(
                sheet_rect,
                scale,
                draw_x_pt,
                draw_y_pt,
                draw_w_pt,
                draw_h_pt,
                page_h_pt,
                page_w_pt,
                is_visual_transposed,
            );

            // Sub-slice UV based on tile
            let tile_uv = Rect::from_min_max(
                pos2(
                    base_uv.min.x + (base_uv.max.x - base_uv.min.x) * u0,
                    base_uv.min.y + (base_uv.max.y - base_uv.min.y) * v0,
                ),
                pos2(
                    base_uv.min.x + (base_uv.max.x - base_uv.min.x) * u1,
                    base_uv.min.y + (base_uv.max.y - base_uv.min.y) * v1,
                ),
            );

            let clip_rect = if self.config.fit_mode == FitMode::Fill {
                printable_rect
            } else {
                sheet_rect
            };

            let clipped_painter = painter.with_clip_rect(clip_rect);
            self.draw_textured_quad(&clipped_painter, texture_id, img_screen_rect, tile_uv);
        } else {
            // Standard single image view
            let (draw_x_pt, draw_y_pt, draw_w_pt, draw_h_pt) = self
                .config
                .calculate_image_placement(page_w_pt, page_h_pt, img_w, img_h);

            let (img_screen_rect, uv_rect) = self.compute_screen_rect_and_uv(
                sheet_rect,
                scale,
                draw_x_pt,
                draw_y_pt,
                draw_w_pt,
                draw_h_pt,
                page_h_pt,
                page_w_pt,
                is_visual_transposed,
            );

            let clip_rect = if self.config.fit_mode == FitMode::Fill {
                printable_rect
            } else {
                sheet_rect
            };

            let clipped_painter = painter.with_clip_rect(clip_rect);
            self.draw_textured_quad(&clipped_painter, texture_id, img_screen_rect, uv_rect);
        }

        // Draw Sheet Info Caption at bottom of canvas
        let caption = format!(
            "{} • {} mm • Rotation: {}° • Zoom: {:.0}%",
            self.config.size_preset.name(),
            if is_visual_transposed {
                format!("{:.0} × {:.0}", page_h_pt / MM_TO_PT, page_w_pt / MM_TO_PT)
            } else {
                format!("{:.0} × {:.0}", page_w_pt / MM_TO_PT, page_h_pt / MM_TO_PT)
            },
            self.config.rotation.degrees(),
            self.zoom * 100.0,
        );

        painter.text(
            pos2(rect.center().x, sheet_rect.max.y + 18.0),
            Align2::CENTER_TOP,
            caption,
            FontId::new(12.0, FontFamily::Proportional),
            Color32::from_gray(140),
        );
    }

    /// Calculate screen rect and UVs accounting for page rotation
    fn compute_screen_rect_and_uv(
        &self,
        sheet_rect: Rect,
        scale: f32,
        draw_x_pt: f32,
        draw_y_pt: f32,
        draw_w_pt: f32,
        draw_h_pt: f32,
        _page_h_pt: f32,
        _page_w_pt: f32,
        _is_visual_transposed: bool,
    ) -> (Rect, Rect) {
        let (screen_x, screen_y, screen_w, screen_h) = match self.config.rotation {
            PageRotation::Deg0 => {
                let sx = sheet_rect.min.x + draw_x_pt * scale;
                let sy = sheet_rect.max.y - (draw_y_pt + draw_h_pt) * scale;
                let sw = draw_w_pt * scale;
                let sh = draw_h_pt * scale;
                (sx, sy, sw, sh)
            }
            PageRotation::Deg90 => {
                // Rotated 90 CW: X becomes Y from top, Y becomes X from left
                let sx = sheet_rect.min.x + draw_y_pt * scale;
                let sy = sheet_rect.min.y + draw_x_pt * scale;
                let sw = draw_h_pt * scale;
                let sh = draw_w_pt * scale;
                (sx, sy, sw, sh)
            }
            PageRotation::Deg180 => {
                let sx = sheet_rect.max.x - (draw_x_pt + draw_w_pt) * scale;
                let sy = sheet_rect.min.y + draw_y_pt * scale;
                let sw = draw_w_pt * scale;
                let sh = draw_h_pt * scale;
                (sx, sy, sw, sh)
            }
            PageRotation::Deg270 => {
                let sx = sheet_rect.max.x - (draw_y_pt + draw_h_pt) * scale;
                let sy = sheet_rect.max.y - (draw_x_pt + draw_w_pt) * scale;
                let sw = draw_h_pt * scale;
                let sh = draw_w_pt * scale;
                (sx, sy, sw, sh)
            }
        };

        let screen_rect = Rect::from_min_size(pos2(screen_x, screen_y), vec2(screen_w, screen_h));
        let uv_rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        (screen_rect, uv_rect)
    }

    /// Draw textured quad onto painter
    fn draw_textured_quad(
        &self,
        painter: &Painter,
        texture_id: egui::TextureId,
        rect: Rect,
        uv: Rect,
    ) {
        let (uv0, uv1, uv2, uv3) = match self.config.rotation {
            PageRotation::Deg0 => (
                pos2(uv.min.x, uv.min.y), // top-left
                pos2(uv.max.x, uv.min.y), // top-right
                pos2(uv.max.x, uv.max.y), // bottom-right
                pos2(uv.min.x, uv.max.y), // bottom-left
            ),
            PageRotation::Deg90 => (
                pos2(uv.min.x, uv.max.y), // was bottom-left -> top-left
                pos2(uv.min.x, uv.min.y), // was top-left -> top-right
                pos2(uv.max.x, uv.min.y), // was top-right -> bottom-right
                pos2(uv.max.x, uv.max.y), // was bottom-right -> bottom-left
            ),
            PageRotation::Deg180 => (
                pos2(uv.max.x, uv.max.y),
                pos2(uv.min.x, uv.max.y),
                pos2(uv.min.x, uv.min.y),
                pos2(uv.max.x, uv.min.y),
            ),
            PageRotation::Deg270 => (
                pos2(uv.max.x, uv.min.y),
                pos2(uv.max.x, uv.max.y),
                pos2(uv.min.x, uv.max.y),
                pos2(uv.min.x, uv.min.y),
            ),
        };

        let mut mesh = egui::Mesh::with_texture(texture_id);
        let base_idx = mesh.vertices.len() as u32;

        let p0 = rect.left_top();
        let p1 = rect.right_top();
        let p2 = rect.right_bottom();
        let p3 = rect.left_bottom();

        mesh.vertices.push(egui::epaint::Vertex {
            pos: p0,
            uv: uv0,
            color: Color32::WHITE,
        });
        mesh.vertices.push(egui::epaint::Vertex {
            pos: p1,
            uv: uv1,
            color: Color32::WHITE,
        });
        mesh.vertices.push(egui::epaint::Vertex {
            pos: p2,
            uv: uv2,
            color: Color32::WHITE,
        });
        mesh.vertices.push(egui::epaint::Vertex {
            pos: p3,
            uv: uv3,
            color: Color32::WHITE,
        });

        mesh.add_triangle(base_idx, base_idx + 1, base_idx + 2);
        mesh.add_triangle(base_idx, base_idx + 2, base_idx + 3);

        painter.add(Shape::mesh(mesh));
    }
}

impl eframe::App for Image2PdfApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_drag_and_drop(&ctx);

        // Top bar
        egui::Panel::top("top_header").show(ui, |ui| {
            ui.add_space(4.0);
            self.render_top_bar(ui);
            ui.add_space(4.0);
        });

        // Left sidebar
        egui::Panel::left("config_sidebar")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| {
                ui.add_space(4.0);
                self.render_sidebar(ui);
            });

        // Central preview canvas
        egui::CentralPanel::default().show(ui, |ui| {
            // Zoom toolbar floating at top of canvas
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("🔍 Canvas Zoom:")
                        .small()
                        .color(Color32::from_gray(140)),
                );
                if ui.small_button("−").clicked() {
                    self.zoom = (self.zoom - 0.1).max(0.2);
                }
                ui.label(format!("{:.0}%", self.zoom * 100.0));
                if ui.small_button("+").clicked() {
                    self.zoom = (self.zoom + 0.1).min(3.0);
                }
                if ui.small_button("Reset").clicked() {
                    self.zoom = 1.0;
                }
            });

            ui.separator();
            self.render_preview_canvas(ui, &ctx);
        });
    }
}
