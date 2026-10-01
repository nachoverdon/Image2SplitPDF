use Image2SplitPDF::cli::{CliArgs, FitModeArg, OrientationArg, RotationArg, run_cli};
use image::{ImageFormat, Rgb, RgbImage};
use std::path::PathBuf;

#[test]
fn test_cli_args_parsing_presets() {
    let mut args = CliArgs {
        images: vec![PathBuf::from("test.png")],
        output: None,
        cli: false,
        gui: false,
        page_size: "a4".to_string(),
        orientation: OrientationArg::Auto,
        page_rotation: RotationArg::Deg0,
        image_rotation: RotationArg::Deg0,
        margin: "normal".to_string(),
        fit: FitModeArg::Fit,
        split: false,
        cols: 2,
        rows: 2,
        auto_grid: None,
        overlap: 5.0,
        verbose: false,
    };

    assert!(!args.should_run_cli());

    args.cli = true;
    assert!(args.should_run_cli());

    args.cli = false;
    args.output = Some(PathBuf::from("out.pdf"));
    assert!(args.should_run_cli());

    // Test page sizes
    let (_, w, h) = args.parse_page_size().unwrap();
    assert_eq!((w, h), (210.0, 297.0));

    args.page_size = "a3".to_string();
    let (_, w, h) = args.parse_page_size().unwrap();
    assert_eq!((w, h), (297.0, 420.0));

    args.page_size = "150x250".to_string();
    let (_, w, h) = args.parse_page_size().unwrap();
    assert_eq!((w, h), (150.0, 250.0));

    args.page_size = "invalid".to_string();
    assert!(args.parse_page_size().is_err());

    // Test margins
    args.margin = "none".to_string();
    assert_eq!(args.parse_margin().unwrap().1, 0.0);

    args.margin = "12.5mm".to_string();
    assert_eq!(args.parse_margin().unwrap().1, 12.5);

    args.margin = "invalid".to_string();
    assert!(args.parse_margin().is_err());
}

#[test]
fn test_cli_execution_end_to_end() {
    let test_dir = PathBuf::from("target/test_samples/cli");
    std::fs::create_dir_all(&test_dir).unwrap();

    // Create a sample test image
    let img_path = test_dir.join("sample.png");
    let mut rgb = RgbImage::new(400, 300);
    for (x, y, pixel) in rgb.enumerate_pixels_mut() {
        *pixel = Rgb([(x % 255) as u8, (y % 255) as u8, 200]);
    }
    rgb.save_with_format(&img_path, ImageFormat::Png).unwrap();

    let out_path = test_dir.join("sample_output.pdf");
    if out_path.exists() {
        std::fs::remove_file(&out_path).unwrap();
    }

    let args = CliArgs {
        images: vec![img_path.clone()],
        output: Some(out_path.clone()),
        cli: true,
        gui: false,
        page_size: "a4".to_string(),
        orientation: OrientationArg::Auto,
        page_rotation: RotationArg::Deg0,
        image_rotation: RotationArg::Deg90,
        margin: "normal".to_string(),
        fit: FitModeArg::Fit,
        split: false,
        cols: 2,
        rows: 2,
        auto_grid: None,
        overlap: 5.0,
        verbose: true,
    };

    assert!(run_cli(&args).is_ok());
    assert!(out_path.exists());
    let bytes = std::fs::read(&out_path).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));

    // Test split poster via CLI
    let split_out_path = test_dir.join("sample_split.pdf");
    let split_args = CliArgs {
        images: vec![img_path],
        output: Some(split_out_path.clone()),
        cli: true,
        gui: false,
        page_size: "a4".to_string(),
        orientation: OrientationArg::Auto,
        page_rotation: RotationArg::Deg0,
        image_rotation: RotationArg::Deg0,
        margin: "small".to_string(),
        fit: FitModeArg::Fit,
        split: true,
        cols: 2,
        rows: 3,
        auto_grid: None,
        overlap: 5.0,
        verbose: false,
    };

    assert!(run_cli(&split_args).is_ok());
    assert!(split_out_path.exists());
    let split_bytes = std::fs::read(&split_out_path).unwrap();
    assert!(split_bytes.starts_with(b"%PDF-"));
    let pdf_str = String::from_utf8_lossy(&split_bytes);
    assert!(pdf_str.contains("/Count 6")); // 2 cols * 3 rows = 6 pages
}
