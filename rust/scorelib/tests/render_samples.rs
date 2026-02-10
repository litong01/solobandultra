//! Rendering tests — parse sample files and render to SVG.

use scorelib::{parse_file, render_score_to_svg, render_file_to_svg};
use std::path::PathBuf;

fn sheetmusic_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sheetmusic")
}

fn output_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_output");
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[test]
fn render_asa_branca_svg() {
    let path = sheetmusic_dir().join("asa-branca.musicxml");
    let svg = render_file_to_svg(&path, None).expect("Failed to render asa-branca");

    // Basic SVG structure checks
    assert!(svg.starts_with("<svg"), "Output should be SVG");
    assert!(svg.contains("</svg>"), "SVG should be closed");
    assert!(svg.contains("Asa branca"), "SVG should contain title");
    assert!(svg.contains("Luiz Gonzaga"), "SVG should contain composer");

    // Should have staff lines
    assert!(svg.contains("<line"), "SVG should contain lines (staff lines)");

    // Should have noteheads
    assert!(svg.contains("<ellipse"), "SVG should contain ellipses (noteheads)");

    // Write to file for visual inspection
    let out = output_dir().join("asa-branca.svg");
    std::fs::write(&out, &svg).expect("Failed to write SVG");
    println!("✓ Rendered asa-branca.svg ({} bytes)", svg.len());
    println!("  Output: {}", out.display());
}

#[test]
fn render_tongnian_svg() {
    let path = sheetmusic_dir().join("童年.mxl");
    let svg = render_file_to_svg(&path, None).expect("Failed to render 童年");

    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("童年"), "SVG should contain Chinese title");
    assert!(svg.contains("罗大佑"), "SVG should contain Chinese composer");
    assert!(svg.contains("<ellipse"), "SVG should contain noteheads");

    let out = output_dir().join("tongnian.svg");
    std::fs::write(&out, &svg).expect("Failed to write SVG");
    println!("✓ Rendered 童年.svg ({} bytes)", svg.len());
    println!("  Output: {}", out.display());
}

#[test]
fn render_chopin_trois_valses_svg() {
    let path = sheetmusic_dir().join("chopin-trois-valses.mxl");
    let svg = render_file_to_svg(&path, None).expect("Failed to render chopin-trois-valses");

    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("</svg>"));
    assert!(svg.contains("<ellipse"), "SVG should contain noteheads");

    let out = output_dir().join("chopin-trois-valses.svg");
    std::fs::write(&out, &svg).expect("Failed to write SVG");
    println!("✓ Rendered chopin-trois-valses.svg ({} bytes)", svg.len());
    println!("  Output: {}", out.display());
}

#[test]
fn render_produces_valid_svg_dimensions() {
    let path = sheetmusic_dir().join("asa-branca.musicxml");
    let score = parse_file(&path).unwrap();
    let svg = render_score_to_svg(&score, None);

    // Check viewBox is present
    assert!(svg.contains("viewBox="), "SVG should have viewBox");
    assert!(svg.contains("width="), "SVG should have width");
    assert!(svg.contains("height="), "SVG should have height");
}

#[test]
fn render_narrow_phone_width() {
    // Simulate a phone screen (390pt wide)
    let phone_width = Some(390.0);

    let path = sheetmusic_dir().join("asa-branca.musicxml");
    let score = parse_file(&path).unwrap();

    let svg_wide = render_score_to_svg(&score, None);
    let svg_narrow = render_score_to_svg(&score, phone_width);

    // Narrow SVG should be taller (more systems) since fewer measures fit per line
    assert!(svg_narrow.contains("viewBox=\"0 0 390"));
    assert!(svg_wide.contains("viewBox=\"0 0 820"));

    // Narrow SVG should have more systems → taller total height
    let height_wide = extract_height(&svg_wide);
    let height_narrow = extract_height(&svg_narrow);
    assert!(
        height_narrow > height_wide,
        "Narrow layout ({}) should be taller than wide layout ({})",
        height_narrow, height_wide
    );

    // Write both for visual comparison
    let out_wide = output_dir().join("asa-branca-wide.svg");
    let out_narrow = output_dir().join("asa-branca-phone.svg");
    std::fs::write(&out_wide, &svg_wide).ok();
    std::fs::write(&out_narrow, &svg_narrow).ok();
    println!("✓ Wide (820): height={}, Narrow (390): height={}", height_wide, height_narrow);
}

#[test]
fn render_blue_bag_folly_svg() {
    let path = sheetmusic_dir().join("blue-bag-folly.musicxml");
    let svg = render_file_to_svg(&path, None).expect("Failed to render blue-bag-folly");

    // Basic SVG structure checks
    assert!(svg.starts_with("<svg"), "Output should be SVG");
    assert!(svg.contains("</svg>"), "SVG should be closed");
    assert!(svg.contains("Blue Bag Folly"), "SVG should contain title");

    // Should have staff lines
    assert!(svg.contains("<line"), "SVG should contain lines (staff lines)");

    // Should have noteheads
    assert!(svg.contains("<ellipse"), "SVG should contain ellipses (noteheads)");

    // Non-trivial output
    assert!(svg.len() > 10000, "SVG should be substantial (got {} bytes)", svg.len());

    // Write to file for visual inspection
    let out = output_dir().join("blue-bag-folly.svg");
    std::fs::write(&out, &svg).expect("Failed to write SVG");
    println!("✓ Rendered blue-bag-folly.svg ({} bytes)", svg.len());
    println!("  Output: {}", out.display());
}

fn extract_height(svg: &str) -> f64 {
    // Extract height from: height="1234"
    svg.split("height=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}
