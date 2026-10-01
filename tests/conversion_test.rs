use Image2SplitPDF::image_loader::LoadedImage;
use Image2SplitPDF::pdf::{
    generate_pdf, save_pdf_to_file, Orientation, PageRotation, PageSizePreset, PdfConfig,
};
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use std::path::PathBuf;

#[test]
fn test_all_formats_and_configurations() {
    let test_dir = PathBuf::from("target/test_samples");
    std::fs::create_dir_all(&test_dir).unwrap();

    let formats = [
        ("test.jpg", ImageFormat::Jpeg),
        ("test.png", ImageFormat::Png),
        ("test.bmp", ImageFormat::Bmp),
        ("test.webp", ImageFormat::WebP),
    ];

    for (filename, format) in formats {
        let img_path = test_dir.join(filename);
        let mut rgb = RgbImage::new(120, 80);
        for (x, y, pixel) in rgb.enumerate_pixels_mut() {
            *pixel = Rgb([(x * 2) as u8, (y * 3) as u8, 150]);
        }
        let dyn_img = DynamicImage::ImageRgb8(rgb);
        dyn_img
            .save_with_format(&img_path, format)
            .expect("Failed to save sample image");

        // 1. Verify loading with LoadedImage
        let loaded = LoadedImage::from_file(&img_path).expect("Failed to load format");
        assert_eq!(loaded.width(), 120);
        assert_eq!(loaded.height(), 80);

        // 2. Test A4 PDF generation with 0° rotation
        let mut config_a4 = PdfConfig::default();
        config_a4.size_preset = PageSizePreset::A4;
        config_a4.rotation = PageRotation::Deg0;
        let pdf_a4_path = test_dir.join(format!("{}_a4.pdf", filename));
        let bytes_written = save_pdf_to_file(&[loaded.working_image.clone()], &config_a4, &pdf_a4_path)
            .expect("Failed to generate A4 PDF");
        assert!(bytes_written > 100);
        assert!(std::fs::read(&pdf_a4_path).unwrap().starts_with(b"%PDF-"));

        // 3. Test A3 PDF generation with 90° rotation
        let mut config_a3 = PdfConfig::default();
        config_a3.size_preset = PageSizePreset::A3;
        config_a3.rotation = PageRotation::Deg90;
        config_a3.orientation = Orientation::Landscape;
        let pdf_a3_path = test_dir.join(format!("{}_a3.pdf", filename));
        let bytes_written = save_pdf_to_file(&[loaded.working_image.clone()], &config_a3, &pdf_a3_path)
            .expect("Failed to generate A3 PDF");
        assert!(bytes_written > 100);

        // 4. Test 2x2 Split Poster generation (4 pages)
        let mut config_split = PdfConfig::default();
        config_split.split_enabled = true;
        config_split.split_cols = 2;
        config_split.split_rows = 2;
        config_split.overlap_mm = 5.0;
        let pdf_split_bytes = generate_pdf(&[loaded.working_image.clone()], &config_split)
            .expect("Failed to generate split PDF");
        assert!(pdf_split_bytes.starts_with(b"%PDF-"));
        // Check that it contains 4 pages in the count
        let pdf_str = String::from_utf8_lossy(&pdf_split_bytes);
        assert!(pdf_str.contains("/Count 4"));
    }
}

#[test]
fn test_big_image_preview_downscaling_and_pdf() {
    use Image2SplitPDF::pdf::save_pdf_from_refs_to_file;

    let test_dir = PathBuf::from("target/test_samples");
    std::fs::create_dir_all(&test_dir).unwrap();

    // 1. Create a 100 x 20480 pixel image (exactly reproducing the crash dimension Y = 20480)
    let width = 100;
    let height = 20480;
    let mut rgb = RgbImage::new(width, height);
    for (x, y, pixel) in rgb.enumerate_pixels_mut() {
        *pixel = Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
    }
    let dyn_img = DynamicImage::ImageRgb8(rgb);

    let mut loaded = LoadedImage {
        path: None,
        filename: "huge_panoramic_image.png".to_string(),
        original: dyn_img.clone(),
        working_image: dyn_img,
        image_rotation: 0,
        format_name: "Png".to_string(),
        file_size_bytes: Some(6_144_000),
    };

    assert_eq!(loaded.width(), 100);
    assert_eq!(loaded.height(), 20480);

    // 2. Test preview generation: must NOT exceed GPU texture limit (2048)
    let preview = loaded.to_egui_color_image();
    assert!(
        preview.size[0] <= LoadedImage::MAX_PREVIEW_DIMENSION as usize,
        "Preview width {} exceeded MAX_PREVIEW_DIMENSION {}",
        preview.size[0],
        LoadedImage::MAX_PREVIEW_DIMENSION
    );
    assert!(
        preview.size[1] <= LoadedImage::MAX_PREVIEW_DIMENSION as usize,
        "Preview height {} exceeded MAX_PREVIEW_DIMENSION {}",
        preview.size[1],
        LoadedImage::MAX_PREVIEW_DIMENSION
    );
    // Height should be exactly 2048, and width should be downscaled proportionally: 100 * 2048 / 20480 = 10
    assert_eq!(preview.size[1], 2048);
    assert_eq!(preview.size[0], 10);

    // 3. Test rotation with big image: width and height swap
    loaded.rotate_cw();
    assert_eq!(loaded.width(), 20480);
    assert_eq!(loaded.height(), 100);

    let preview_rot = loaded.to_egui_color_image();
    assert_eq!(preview_rot.size[0], 2048);
    assert_eq!(preview_rot.size[1], 10);

    // 4. Test single-page PDF generation for the big image
    let config_single = PdfConfig::default();
    let pdf_single_bytes = generate_pdf(&[loaded.working_image.clone()], &config_single)
        .expect("Failed to generate single-page PDF for big image");
    assert!(pdf_single_bytes.starts_with(b"%PDF-"));

    // 5. Test split poster PDF generation for the big image (e.g. 1 col x 4 rows)
    let mut config_split = PdfConfig::default();
    config_split.split_enabled = true;
    config_split.split_cols = 1;
    config_split.split_rows = 4;
    config_split.overlap_mm = 5.0;

    let dest_pdf = test_dir.join("big_image_split.pdf");
    let bytes_written = save_pdf_from_refs_to_file(&[&loaded.working_image], &config_split, &dest_pdf)
        .expect("Failed to save split PDF for big image");
    assert!(bytes_written > 100);
    let read_bytes = std::fs::read(&dest_pdf).unwrap();
    assert!(read_bytes.starts_with(b"%PDF-"));
    let pdf_str = String::from_utf8_lossy(&read_bytes);
    assert!(pdf_str.contains("/Count 4"));

    // 6. Test LoadedImage::from_file with 20480 px image
    let big_img_path = test_dir.join("big_test_image.png");
    loaded.original.save(&big_img_path).expect("Failed to save big image to file");
    let loaded_from_file = LoadedImage::from_file(&big_img_path).expect("Failed to load big image from file");
    assert_eq!(loaded_from_file.width(), 100);
    assert_eq!(loaded_from_file.height(), 20480);
    let preview_from_file = loaded_from_file.to_egui_color_image();
    assert_eq!(preview_from_file.size[1], 2048);
    assert_eq!(preview_from_file.size[0], 10);
}

#[test]
fn test_auto_grid_calculation_and_pdf_export() {
    use Image2SplitPDF::pdf::save_pdf_from_refs_to_file;

    let test_dir = PathBuf::from("target/test_samples");
    std::fs::create_dir_all(&test_dir).unwrap();

    // 1. Small image (200x150) -> fits on 1 page at 300 DPI
    let small_img = DynamicImage::new_rgb8(200, 150);
    let mut config = PdfConfig::default();
    let (cols, rows) = config.calculate_auto_grid(small_img.width(), small_img.height(), 300.0);
    assert_eq!(cols, 1);
    assert_eq!(rows, 1);

    // 2. Large image (6000x4000) at 300 DPI in Portrait orientation
    config.orientation = Orientation::Portrait;
    let (cols_p, rows_p) = config.calculate_auto_grid(6000, 4000, 300.0);
    assert_eq!(cols_p, 3);
    assert_eq!(rows_p, 2);

    // 3. Set the config to the auto-calculated grid and generate PDF
    config.split_enabled = true;
    config.split_cols = cols_p;
    config.split_rows = rows_p;

    let test_img = DynamicImage::new_rgb8(600, 400); // scaled down for quick test rendering
    let dest_pdf = test_dir.join("auto_grid_poster.pdf");
    let bytes_written = save_pdf_from_refs_to_file(&[&test_img], &config, &dest_pdf)
        .expect("Failed to generate auto-adjusted split PDF");
    assert!(bytes_written > 100);

    let file_bytes = std::fs::read(&dest_pdf).unwrap();
    let pdf_str = String::from_utf8_lossy(&file_bytes);
    assert!(pdf_str.contains("/Count 6")); // 3 cols x 2 rows = 6 pages

    // 4. Test target DPI scaling:
    // Lower DPI means pixels are printed larger -> physically larger print -> more pages needed.
    // Higher DPI means denser pixels -> physically smaller print -> fewer pages needed.
    let (cols_150, rows_150) = config.calculate_auto_grid(6000, 4000, 150.0);
    assert!(cols_150 >= cols_p);
    assert!(rows_150 >= rows_p);
    assert_eq!(cols_150, 6);
    assert_eq!(rows_150, 3);

    let (cols_600, rows_600) = config.calculate_auto_grid(6000, 4000, 600.0);
    assert!(cols_600 <= cols_p);
    assert!(rows_600 <= rows_p);
    assert_eq!(cols_600, 2);
    assert_eq!(rows_600, 1);
}


