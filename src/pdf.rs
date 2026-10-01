use image::{DynamicImage, GenericImageView};
use pdf_writer::{Content, Filter, Name, Pdf, Rect, Ref};
use std::path::Path;

const MM_TO_PT: f32 = 72.0 / 25.4; // ~2.8346457

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSizePreset {
    A4,
    A3,
    A5,
    Letter,
    Legal,
    Custom,
}

impl PageSizePreset {
    pub const ALL: [Self; 5] = [
        Self::A4,
        Self::A3,
        Self::A5,
        Self::Letter,
        Self::Legal,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::A4 => "A4 (210 × 297 mm)",
            Self::A3 => "A3 (297 × 420 mm)",
            Self::A5 => "A5 (148 × 210 mm)",
            Self::Letter => "Letter (8.5 × 11 in)",
            Self::Legal => "Legal (8.5 × 14 in)",
            Self::Custom => "Custom Size",
        }
    }

    /// Returns (width_mm, height_mm) in portrait orientation
    pub fn dimensions_mm(&self) -> (f32, f32) {
        match self {
            Self::A4 => (210.0, 297.0),
            Self::A3 => (297.0, 420.0),
            Self::A5 => (148.0, 210.0),
            Self::Letter => (215.9, 279.4),
            Self::Legal => (215.9, 355.6),
            Self::Custom => (210.0, 297.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
    Auto,
}

impl Orientation {
    pub const ALL: [Self; 3] = [Self::Portrait, Self::Landscape, Self::Auto];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Portrait => "Portrait",
            Self::Landscape => "Landscape",
            Self::Auto => "Auto (Match Image)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageRotation {
    Deg0 = 0,
    Deg90 = 90,
    Deg180 = 180,
    Deg270 = 270,
}

impl PageRotation {
    pub const ALL: [Self; 4] = [Self::Deg0, Self::Deg90, Self::Deg180, Self::Deg270];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Deg0 => "0° (Normal)",
            Self::Deg90 => "90° Clockwise",
            Self::Deg180 => "180° Inverted",
            Self::Deg270 => "270° Counter-Clockwise",
        }
    }

    pub fn degrees(&self) -> i32 {
        *self as i32
    }

    pub fn rotate_cw(&self) -> Self {
        match self {
            Self::Deg0 => Self::Deg90,
            Self::Deg90 => Self::Deg180,
            Self::Deg180 => Self::Deg270,
            Self::Deg270 => Self::Deg0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitMode {
    Fit,      // Contain (keep aspect ratio, letterboxed)
    Fill,     // Cover (keep aspect ratio, cropped to margin area)
    Stretch,  // Stretch to exact margin area
    Original, // 100% DPI (centered)
}

impl FitMode {
    pub const ALL: [Self; 4] = [Self::Fit, Self::Fill, Self::Stretch, Self::Original];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Fit => "Fit to Page (Keep Aspect Ratio)",
            Self::Fill => "Fill Page (Crop Overflow)",
            Self::Stretch => "Stretch to Fit Margins",
            Self::Original => "Original Size (Centered 300 DPI)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginPreset {
    None,
    Small,  // 5 mm
    Normal, // 10 mm
    Large,  // 20 mm
    Custom,
}

impl MarginPreset {
    pub const ALL: [Self; 4] = [Self::None, Self::Small, Self::Normal, Self::Large];

    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "None (0 mm)",
            Self::Small => "Small (5 mm)",
            Self::Normal => "Normal (10 mm)",
            Self::Large => "Large (20 mm)",
            Self::Custom => "Custom Margin",
        }
    }

    pub fn mm(&self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Small => 5.0,
            Self::Normal => 10.0,
            Self::Large => 20.0,
            Self::Custom => 10.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PdfConfig {
    pub size_preset: PageSizePreset,
    pub custom_width_mm: f32,
    pub custom_height_mm: f32,
    pub orientation: Orientation,
    pub rotation: PageRotation,
    pub fit_mode: FitMode,
    pub margin_preset: MarginPreset,
    pub custom_margin_mm: f32,
    // Split / Poster settings
    pub split_enabled: bool,
    pub split_cols: usize,
    pub split_rows: usize,
    pub overlap_mm: f32,
}

impl Default for PdfConfig {
    fn default() -> Self {
        Self {
            size_preset: PageSizePreset::A4,
            custom_width_mm: 210.0,
            custom_height_mm: 297.0,
            orientation: Orientation::Auto,
            rotation: PageRotation::Deg0,
            fit_mode: FitMode::Fit,
            margin_preset: MarginPreset::Normal,
            custom_margin_mm: 10.0,
            split_enabled: false,
            split_cols: 2,
            split_rows: 2,
            overlap_mm: 5.0,
        }
    }
}

impl PdfConfig {
    /// Effective margin in mm
    pub fn margin_mm(&self) -> f32 {
        match self.margin_preset {
            MarginPreset::Custom => self.custom_margin_mm.max(0.0),
            preset => preset.mm(),
        }
    }

    /// Calculate effective page dimensions (width_pt, height_pt) for a given image
    pub fn resolved_page_size_pt(&self, img_width: u32, img_height: u32) -> (f32, f32) {
        let (mut w_mm, mut h_mm) = match self.size_preset {
            PageSizePreset::Custom => (
                self.custom_width_mm.max(10.0),
                self.custom_height_mm.max(10.0),
            ),
            preset => preset.dimensions_mm(),
        };

        // Standardize to portrait first
        let min_dim = w_mm.min(h_mm);
        let max_dim = w_mm.max(h_mm);

        let is_landscape = match self.orientation {
            Orientation::Portrait => false,
            Orientation::Landscape => true,
            Orientation::Auto => {
                if self.split_enabled {
                    // For split mode, determine based on tile aspect ratio
                    let tile_w = (img_width as f32) / (self.split_cols.max(1) as f32);
                    let tile_h = (img_height as f32) / (self.split_rows.max(1) as f32);
                    tile_w > tile_h
                } else {
                    img_width > img_height
                }
            }
        };

        if is_landscape {
            w_mm = max_dim;
            h_mm = min_dim;
        } else {
            w_mm = min_dim;
            h_mm = max_dim;
        }

        (w_mm * MM_TO_PT, h_mm * MM_TO_PT)
    }

    /// Calculate image placement rectangle on a page: [x_pt, y_pt, width_pt, height_pt]
    /// (x_pt, y_pt are from bottom-left corner of the page in PDF coordinate system)
    pub fn calculate_image_placement(
        &self,
        page_w_pt: f32,
        page_h_pt: f32,
        img_w_px: u32,
        img_h_px: u32,
    ) -> (f32, f32, f32, f32) {
        let margin_pt = self.margin_mm() * MM_TO_PT;
        let printable_w = (page_w_pt - 2.0 * margin_pt).max(1.0);
        let printable_h = (page_h_pt - 2.0 * margin_pt).max(1.0);

        let img_w = (img_w_px as f32).max(1.0);
        let img_h = (img_h_px as f32).max(1.0);

        let (draw_w, draw_h) = match self.fit_mode {
            FitMode::Fit => {
                let scale = (printable_w / img_w).min(printable_h / img_h);
                (img_w * scale, img_h * scale)
            }
            FitMode::Fill => {
                let scale = (printable_w / img_w).max(printable_h / img_h);
                (img_w * scale, img_h * scale)
            }
            FitMode::Stretch => (printable_w, printable_h),
            FitMode::Original => {
                // 300 DPI: 1 inch = 72 pt = 300 px -> scale = 72 / 300 = 0.24
                let scale = 72.0 / 300.0;
                (img_w * scale, img_h * scale)
            }
        };

        // Center on the printable area
        let draw_x = margin_pt + (printable_w - draw_w) / 2.0;
        let draw_y = margin_pt + (printable_h - draw_h) / 2.0;

        (draw_x, draw_y, draw_w, draw_h)
    }

    /// Total number of pages produced from an image
    pub fn total_pages(&self) -> usize {
        if self.split_enabled {
            self.split_cols.max(1) * self.split_rows.max(1)
        } else {
            1
        }
    }

    /// Calculate the grid of pages (columns, rows) required to print an image
    /// of dimensions `(img_w, img_h)` at a given target DPI without downscaling.
    ///
    /// Considers paper dimensions, margins, overlap, and orientation.
    /// Clamps result to `1..=25` for cols and rows.
    pub fn calculate_auto_grid(
        &self,
        img_w: u32,
        img_h: u32,
        target_dpi: f32,
    ) -> (usize, usize) {
        if img_w == 0 || img_h == 0 {
            return (1, 1);
        }

        let dpi = if target_dpi.is_finite() && target_dpi >= 10.0 {
            target_dpi
        } else {
            300.0
        };

        // Standardize page dimensions in mm (min_dim, max_dim)
        let (w_mm, h_mm) = match self.size_preset {
            PageSizePreset::Custom => (
                self.custom_width_mm.max(10.0),
                self.custom_height_mm.max(10.0),
            ),
            preset => preset.dimensions_mm(),
        };
        let min_dim_mm = w_mm.min(h_mm);
        let max_dim_mm = w_mm.max(h_mm);

        let margin_mm = self.margin_mm();
        let overlap_mm = self.overlap_mm.max(0.0);

        // Printable area per page (must be at least 10 mm)
        let printable_w_portrait = (min_dim_mm - 2.0 * margin_mm).max(10.0);
        let printable_h_portrait = (max_dim_mm - 2.0 * margin_mm).max(10.0);

        let printable_w_landscape = (max_dim_mm - 2.0 * margin_mm).max(10.0);
        let printable_h_landscape = (min_dim_mm - 2.0 * margin_mm).max(10.0);

        // Effective advance per tile (excluding overlap area)
        let eff_w_portrait = (printable_w_portrait - overlap_mm).max(5.0);
        let eff_h_portrait = (printable_h_portrait - overlap_mm).max(5.0);

        let eff_w_landscape = (printable_w_landscape - overlap_mm).max(5.0);
        let eff_h_landscape = (printable_h_landscape - overlap_mm).max(5.0);

        // Image dimensions in mm at target DPI
        let img_w_mm = (img_w as f32) / dpi * 25.4;
        let img_h_mm = (img_h as f32) / dpi * 25.4;

        let calc_grid_for_dims = |p_w: f32, p_h: f32, eff_w: f32, eff_h: f32| -> (usize, usize) {
            let cols = if img_w_mm <= p_w {
                1
            } else {
                1 + ((img_w_mm - p_w) / eff_w).ceil() as usize
            };
            let rows = if img_h_mm <= p_h {
                1
            } else {
                1 + ((img_h_mm - p_h) / eff_h).ceil() as usize
            };
            (cols.clamp(1, 25), rows.clamp(1, 25))
        };

        match self.orientation {
            Orientation::Portrait => {
                calc_grid_for_dims(
                    printable_w_portrait,
                    printable_h_portrait,
                    eff_w_portrait,
                    eff_h_portrait,
                )
            }
            Orientation::Landscape => {
                calc_grid_for_dims(
                    printable_w_landscape,
                    printable_h_landscape,
                    eff_w_landscape,
                    eff_h_landscape,
                )
            }
            Orientation::Auto => {
                let grid_p = calc_grid_for_dims(
                    printable_w_portrait,
                    printable_h_portrait,
                    eff_w_portrait,
                    eff_h_portrait,
                );
                let grid_l = calc_grid_for_dims(
                    printable_w_landscape,
                    printable_h_landscape,
                    eff_w_landscape,
                    eff_h_landscape,
                );

                let pages_p = grid_p.0 * grid_p.1;
                let pages_l = grid_l.0 * grid_l.1;

                // Check consistency with resolved_page_size_pt (which uses tile_w > tile_h for Landscape)
                let tile_w_p = (img_w as f32) / (grid_p.0 as f32);
                let tile_h_p = (img_h as f32) / (grid_p.1 as f32);
                let p_consistent = tile_w_p <= tile_h_p;

                let tile_w_l = (img_w as f32) / (grid_l.0 as f32);
                let tile_h_l = (img_h as f32) / (grid_l.1 as f32);
                let l_consistent = tile_w_l >= tile_h_l;

                if p_consistent && !l_consistent {
                    grid_p
                } else if l_consistent && !p_consistent {
                    grid_l
                } else if pages_l < pages_p {
                    grid_l
                } else if pages_p < pages_l {
                    grid_p
                } else {
                    // Tie breaker: match the image aspect ratio
                    if img_w >= img_h {
                        grid_l
                    } else {
                        grid_p
                    }
                }
            }
        }
    }
}

/// Convert any DynamicImage into RGBA flattened on white (RGB8) for PDF output.
/// Highly optimized for large images: fast-path for RGB8 images without copying,
/// and fast integer alpha blending for transparent images.
pub fn to_rgb_flattened(image: &DynamicImage) -> (u32, u32, Vec<u8>) {
    let (w, h) = image.dimensions();
    let total_pixels = (w as usize) * (h as usize);
    let total_bytes = total_pixels
        .checked_mul(3)
        .expect("Image byte size overflowed usize");

    // Fast-path: If the image is already standard RGB8, directly clone its raw buffer!
    if let DynamicImage::ImageRgb8(rgb) = image {
        return (w, h, rgb.as_raw().clone());
    }

    let rgba = image.to_rgba8();
    let raw = rgba.as_raw();
    let mut rgb_bytes = Vec::with_capacity(total_bytes);

    for chunk in raw.chunks_exact(4) {
        let a = chunk[3];
        if a == 255 {
            rgb_bytes.push(chunk[0]);
            rgb_bytes.push(chunk[1]);
            rgb_bytes.push(chunk[2]);
        } else if a == 0 {
            rgb_bytes.push(255);
            rgb_bytes.push(255);
            rgb_bytes.push(255);
        } else {
            let a = a as u32;
            let inv_a = 255 - a;
            let r = ((chunk[0] as u32 * a + 255 * inv_a + 127) / 255) as u8;
            let g = ((chunk[1] as u32 * a + 255 * inv_a + 127) / 255) as u8;
            let b = ((chunk[2] as u32 * a + 255 * inv_a + 127) / 255) as u8;
            rgb_bytes.push(r);
            rgb_bytes.push(g);
            rgb_bytes.push(b);
        }
    }

    (w, h, rgb_bytes)
}

/// Extract a tile for grid/poster splitting
pub fn extract_tile(
    img: &DynamicImage,
    col: usize,
    row: usize,
    total_cols: usize,
    total_rows: usize,
    overlap_px: u32,
) -> DynamicImage {
    let (img_w, img_h) = img.dimensions();
    let base_tile_w = img_w / (total_cols.max(1) as u32);
    let base_tile_h = img_h / (total_rows.max(1) as u32);

    let x0 = if col == 0 {
        0
    } else {
        (col as u32 * base_tile_w)
            .saturating_sub(overlap_px)
            .min(img_w.saturating_sub(1))
    };

    let y0 = if row == 0 {
        0
    } else {
        (row as u32 * base_tile_h)
            .saturating_sub(overlap_px)
            .min(img_h.saturating_sub(1))
    };

    let x1 = if col + 1 >= total_cols {
        img_w
    } else {
        ((col as u32 + 1) * base_tile_w + overlap_px).min(img_w)
    };

    let y1 = if row + 1 >= total_rows {
        img_h
    } else {
        ((row as u32 + 1) * base_tile_h + overlap_px).min(img_h)
    };

    let crop_w = (x1.saturating_sub(x0)).max(1).min(img_w - x0);
    let crop_h = (y1.saturating_sub(y0)).max(1).min(img_h - y0);

    img.crop_imm(x0, y0, crop_w, crop_h)
}

/// Generate PDF bytes from a list of DynamicImages according to the config
pub fn generate_pdf(images: &[DynamicImage], config: &PdfConfig) -> Result<Vec<u8>, String> {
    let refs: Vec<&DynamicImage> = images.iter().collect();
    generate_pdf_from_refs(&refs, config)
}

/// Generate PDF bytes from image references according to the config without cloning images
pub fn generate_pdf_from_refs(images: &[&DynamicImage], config: &PdfConfig) -> Result<Vec<u8>, String> {
    if images.is_empty() {
        return Err("No images provided for PDF generation.".to_string());
    }

    let mut pdf = Pdf::new();
    let mut ref_counter = 1;

    let catalog_id = Ref::new(ref_counter);
    ref_counter += 1;
    let page_tree_id = Ref::new(ref_counter);
    ref_counter += 1;

    // Collect all pages (each page gets: page_id, content_id, image_id, image_data, placement, dimensions)
    struct PageItem {
        page_id: Ref,
        content_id: Ref,
        image_id: Ref,
        page_w_pt: f32,
        page_h_pt: f32,
        draw_x: f32,
        draw_y: f32,
        draw_w: f32,
        draw_h: f32,
        clip_rect: Option<(f32, f32, f32, f32)>, // for Fill mode
        img_w: u32,
        img_h: u32,
        compressed_rgb: Vec<u8>,
    }

    let mut pages_data = Vec::new();

    for &img in images {
        if config.split_enabled {
            let cols = config.split_cols.max(1);
            let rows = config.split_rows.max(1);

            // Calculate overlap in px based on estimated DPI
            let overlap_mm = config.overlap_mm.max(0.0);
            let avg_dpi = (img.width() as f32 / (210.0 / 25.4 * cols as f32)).clamp(72.0, 600.0);
            let overlap_px = (overlap_mm / 25.4 * avg_dpi).round() as u32;

            for r in 0..rows {
                for c in 0..cols {
                    let tile_img = extract_tile(img, c, r, cols, rows, overlap_px);
                    let (tile_w, tile_h, rgb_bytes) = to_rgb_flattened(&tile_img);
                    drop(tile_img);
                    let compressed =
                        miniz_oxide::deflate::compress_to_vec_zlib(&rgb_bytes, 6);
                    drop(rgb_bytes);

                    let (page_w_pt, page_h_pt) =
                        config.resolved_page_size_pt(tile_w, tile_h);
                    let (draw_x, draw_y, draw_w, draw_h) =
                        config.calculate_image_placement(page_w_pt, page_h_pt, tile_w, tile_h);

                    let margin_pt = config.margin_mm() * MM_TO_PT;
                    let clip_rect = if config.fit_mode == FitMode::Fill {
                        Some((
                            margin_pt,
                            margin_pt,
                            page_w_pt - 2.0 * margin_pt,
                            page_h_pt - 2.0 * margin_pt,
                        ))
                    } else {
                        None
                    };

                    let page_id = Ref::new(ref_counter);
                    ref_counter += 1;
                    let content_id = Ref::new(ref_counter);
                    ref_counter += 1;
                    let image_id = Ref::new(ref_counter);
                    ref_counter += 1;

                    pages_data.push(PageItem {
                        page_id,
                        content_id,
                        image_id,
                        page_w_pt,
                        page_h_pt,
                        draw_x,
                        draw_y,
                        draw_w,
                        draw_h,
                        clip_rect,
                        img_w: tile_w,
                        img_h: tile_h,
                        compressed_rgb: compressed,
                    });
                }
            }
        } else {
            let (img_w, img_h, rgb_bytes) = to_rgb_flattened(img);
            let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&rgb_bytes, 6);
            drop(rgb_bytes);

            let (page_w_pt, page_h_pt) = config.resolved_page_size_pt(img_w, img_h);
            let (draw_x, draw_y, draw_w, draw_h) =
                config.calculate_image_placement(page_w_pt, page_h_pt, img_w, img_h);

            let margin_pt = config.margin_mm() * MM_TO_PT;
            let clip_rect = if config.fit_mode == FitMode::Fill {
                Some((
                    margin_pt,
                    margin_pt,
                    page_w_pt - 2.0 * margin_pt,
                    page_h_pt - 2.0 * margin_pt,
                ))
            } else {
                None
            };

            let page_id = Ref::new(ref_counter);
            ref_counter += 1;
            let content_id = Ref::new(ref_counter);
            ref_counter += 1;
            let image_id = Ref::new(ref_counter);
            ref_counter += 1;

            pages_data.push(PageItem {
                page_id,
                content_id,
                image_id,
                page_w_pt,
                page_h_pt,
                draw_x,
                draw_y,
                draw_w,
                draw_h,
                clip_rect,
                img_w,
                img_h,
                compressed_rgb: compressed,
            });
        }
    }

    let page_count = pages_data.len();
    if page_count == 0 {
        return Err("No pages generated.".to_string());
    }

    // Set catalog & pages tree
    pdf.catalog(catalog_id).pages(page_tree_id);
    let kids: Vec<Ref> = pages_data.iter().map(|p| p.page_id).collect();
    pdf.pages(page_tree_id)
        .kids(kids)
        .count(page_count as i32);

    // Build pages
    for page_item in &pages_data {
        let mut page = pdf.page(page_item.page_id);
        page.parent(page_tree_id);
        page.media_box(Rect::new(0.0, 0.0, page_item.page_w_pt, page_item.page_h_pt));
        page.rotate(config.rotation.degrees());

        let mut res = page.resources();
        res.x_objects().pair(Name(b"Im0"), page_item.image_id);
        drop(res);

        page.contents(page_item.content_id);
        drop(page);

        // Content stream
        let mut content = Content::new();
        content.save_state();

        // If Fill mode, apply clip path to margin area
        if let Some((cx, cy, cw, ch)) = page_item.clip_rect {
            content.rect(cx, cy, cw, ch);
            content.clip_nonzero();
            content.end_path();
        }

        // Draw image using transformation matrix
        content.transform([
            page_item.draw_w,
            0.0,
            0.0,
            page_item.draw_h,
            page_item.draw_x,
            page_item.draw_y,
        ]);
        content.x_object(Name(b"Im0"));
        content.restore_state();

        let stream_bytes = content.finish();
        pdf.stream(page_item.content_id, &stream_bytes);

        // Image XObject
        let mut img_obj = pdf.image_xobject(page_item.image_id, &page_item.compressed_rgb);
        img_obj.width(page_item.img_w as i32);
        img_obj.height(page_item.img_h as i32);
        img_obj.color_space().device_rgb();
        img_obj.bits_per_component(8);
        img_obj.filter(Filter::FlateDecode);
        drop(img_obj);
    }

    Ok(pdf.finish())
}

/// Save generated PDF from image references to file path
pub fn save_pdf_from_refs_to_file<P: AsRef<Path>>(
    images: &[&DynamicImage],
    config: &PdfConfig,
    dest_path: P,
) -> Result<usize, String> {
    let pdf_bytes = generate_pdf_from_refs(images, config)?;
    let byte_count = pdf_bytes.len();
    std::fs::write(&dest_path, pdf_bytes).map_err(|e| format!("Failed to write PDF: {}", e))?;
    Ok(byte_count)
}

/// Save generated PDF to file path
pub fn save_pdf_to_file<P: AsRef<Path>>(
    images: &[DynamicImage],
    config: &PdfConfig,
    dest_path: P,
) -> Result<usize, String> {
    let refs: Vec<&DynamicImage> = images.iter().collect();
    save_pdf_from_refs_to_file(&refs, config, dest_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_generation() {
        let img = DynamicImage::new_rgb8(100, 100);
        let config = PdfConfig::default();
        let bytes = generate_pdf(&[img], &config).expect("Failed to generate PDF");
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.ends_with(b"%%EOF\n") || bytes.ends_with(b"%%EOF"));
    }

    #[test]
    fn test_pdf_split_generation() {
        let img = DynamicImage::new_rgb8(200, 200);
        let mut config = PdfConfig::default();
        config.split_enabled = true;
        config.split_cols = 2;
        config.split_rows = 2;
        let bytes = generate_pdf(&[img], &config).expect("Failed to generate split PDF");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn test_calculate_auto_grid_small_image() {
        let config = PdfConfig::default(); // A4, 10mm margin, Auto orientation
        // At 300 DPI, 800x600 px is ~67.7 x 50.8 mm, which easily fits on a single A4 page
        let (cols, rows) = config.calculate_auto_grid(800, 600, 300.0);
        assert_eq!(cols, 1);
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_calculate_auto_grid_large_image() {
        let mut config = PdfConfig::default();
        config.size_preset = PageSizePreset::A4;
        config.margin_preset = MarginPreset::Normal; // 10mm
        config.overlap_mm = 5.0;
        config.orientation = Orientation::Portrait;

        // 6000 x 4000 px at 300 DPI:
        // width = 508 mm (printable = 190 mm, eff = 185 mm -> 3 cols)
        // height = 338.7 mm (printable = 277 mm, eff = 272 mm -> 2 rows)
        let (cols, rows) = config.calculate_auto_grid(6000, 4000, 300.0);
        assert_eq!(cols, 3);
        assert_eq!(rows, 2);
    }

    #[test]
    fn test_calculate_auto_grid_orientations() {
        let mut config = PdfConfig::default();
        config.size_preset = PageSizePreset::A4;
        config.margin_preset = MarginPreset::Normal;
        config.overlap_mm = 5.0;

        // In Landscape explicitly:
        // printable = 277 x 190 mm (eff = 272 x 185 mm)
        // 6000 px -> 508 mm -> 2 cols
        // 4000 px -> 338.7 mm -> 2 rows
        config.orientation = Orientation::Landscape;
        let (cols_l, rows_l) = config.calculate_auto_grid(6000, 4000, 300.0);
        assert_eq!(cols_l, 2);
        assert_eq!(rows_l, 2);

        // In Auto orientation:
        // Should choose Landscape because 2x2 (4 pages) < 3x2 (6 pages)
        config.orientation = Orientation::Auto;
        let (cols_auto, rows_auto) = config.calculate_auto_grid(6000, 4000, 300.0);
        assert_eq!(cols_auto, 2);
        assert_eq!(rows_auto, 2);
    }

    #[test]
    fn test_calculate_auto_grid_panoramic_and_tall() {
        let mut config = PdfConfig::default();
        config.size_preset = PageSizePreset::A4;
        config.orientation = Orientation::Portrait;

        // Tall banner: 100 x 20480 px
        // 100 px at 300 DPI = 8.5 mm -> 1 col
        // 20480 px at 300 DPI = 1734 mm -> 7 rows on A4
        let (cols, rows) = config.calculate_auto_grid(100, 20480, 300.0);
        assert_eq!(cols, 1);
        assert_eq!(rows, 7);

        // Zero dimensions fallback
        let (cols_0, rows_0) = config.calculate_auto_grid(0, 0, 300.0);
        assert_eq!(cols_0, 1);
        assert_eq!(rows_0, 1);
    }
}

