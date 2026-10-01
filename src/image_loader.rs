use image::{DynamicImage, ImageReader};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct LoadedImage {
    pub path: Option<PathBuf>,
    pub filename: String,
    pub original: DynamicImage,
    pub working_image: DynamicImage,
    pub image_rotation: u32, // 0, 90, 180, 270
    pub format_name: String,
    pub file_size_bytes: Option<u64>,
}

impl LoadedImage {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path = path.as_ref();
        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let file_size_bytes = std::fs::metadata(path).ok().map(|m| m.len());

        let mut reader = ImageReader::open(path)
            .map_err(|e| format!("Could not open file: {}", e))?
            .with_guessed_format()
            .map_err(|e| format!("Could not determine image format: {}", e))?;
        reader.no_limits();

        let format_name = reader
            .format()
            .map(|f| format!("{:?}", f))
            .unwrap_or_else(|| "Unknown".to_string());

        let img = reader
            .decode()
            .map_err(|e| format!("Could not decode image: {}", e))?;

        Ok(Self {
            path: Some(path.to_path_buf()),
            filename,
            working_image: img.clone(),
            original: img,
            image_rotation: 0,
            format_name,
            file_size_bytes,
        })
    }

    pub fn from_memory(data: &[u8], name: &str) -> Result<Self, String> {
        let mut reader = ImageReader::new(std::io::Cursor::new(data))
            .with_guessed_format()
            .map_err(|e| format!("Could not determine image format: {}", e))?;
        reader.no_limits();

        let format_name = reader
            .format()
            .map(|f| format!("{:?}", f))
            .unwrap_or_else(|| "Unknown".to_string());

        let img = reader
            .decode()
            .map_err(|e| format!("Could not decode image: {}", e))?;

        Ok(Self {
            path: None,
            filename: name.to_string(),
            working_image: img.clone(),
            original: img,
            image_rotation: 0,
            format_name,
            file_size_bytes: Some(data.len() as u64),
        })
    }

    pub fn width(&self) -> u32 {
        self.working_image.width()
    }

    pub fn height(&self) -> u32 {
        self.working_image.height()
    }

    pub fn aspect_ratio(&self) -> f32 {
        let (w, h) = (self.width(), self.height());
        if h > 0 {
            w as f32 / h as f32
        } else {
            1.0
        }
    }

    pub fn rotate_cw(&mut self) {
        self.image_rotation = (self.image_rotation + 90) % 360;
        self.working_image = match self.image_rotation {
            90 => self.original.rotate90(),
            180 => self.original.rotate180(),
            270 => self.original.rotate270(),
            _ => self.original.clone(),
        };
    }

    pub fn set_rotation(&mut self, degrees: u32) {
        self.image_rotation = degrees % 360;
        self.working_image = match self.image_rotation {
            90 => self.original.rotate90(),
            180 => self.original.rotate180(),
            270 => self.original.rotate270(),
            _ => self.original.clone(),
        };
    }

    pub fn formatted_size(&self) -> String {
        match self.file_size_bytes {
            Some(bytes) if bytes < 1024 => format!("{} B", bytes),
            Some(bytes) if bytes < 1024 * 1024 => format!("{:.1} KB", bytes as f64 / 1024.0),
            Some(bytes) => format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0)),
            None => "Unknown".to_string(),
        }
    }

    /// Safe maximum dimension for GPU preview textures.
    /// Hardware/WGPU limits are often 8192 or 4096 (and WebGPU default 2048).
    /// 2048 px provides retina-sharp UI preview rendering while never exceeding GPU texture limits.
    pub const MAX_PREVIEW_DIMENSION: u32 = 2048;

    /// Converts the current working image to an egui ColorImage for preview display,
    /// downscaling if necessary so it never exceeds GPU texture limits.
    pub fn to_egui_color_image(&self) -> eframe::egui::ColorImage {
        self.to_egui_color_image_with_max_size(Self::MAX_PREVIEW_DIMENSION)
    }

    /// Converts the current working image to an egui ColorImage with a custom maximum dimension constraint.
    pub fn to_egui_color_image_with_max_size(&self, max_dim: u32) -> eframe::egui::ColorImage {
        let (w, h) = (self.width(), self.height());
        if w == 0 || h == 0 {
            return eframe::egui::ColorImage::new([1, 1], vec![eframe::egui::Color32::TRANSPARENT]);
        }

        if w > max_dim || h > max_dim {
            let thumb = self.working_image.thumbnail(max_dim, max_dim);
            let tw = thumb.width() as usize;
            let th = thumb.height() as usize;
            let rgba = thumb.into_rgba8();
            eframe::egui::ColorImage::from_rgba_unmultiplied([tw, th], rgba.as_raw())
        } else {
            let rgba = self.working_image.to_rgba8();
            eframe::egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw())
        }
    }
}
