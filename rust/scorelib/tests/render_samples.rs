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
    let svg = render_file_to_svg(&path).expect("Failed to render asa-branca");

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
    let svg = render_file_to_svg(&path).expect("Failed to render 童年");

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
fn render_produces_valid_svg_dimensions() {
    let path = sheetmusic_dir().join("asa-branca.musicxml");
    let score = parse_file(&path).unwrap();
    let svg = render_score_to_svg(&score);

    // Check viewBox is present
    assert!(svg.contains("viewBox="), "SVG should have viewBox");
    assert!(svg.contains("width="), "SVG should have width");
    assert!(svg.contains("height="), "SVG should have height");
}
