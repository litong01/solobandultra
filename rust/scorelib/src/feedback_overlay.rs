//! Feedback overlay: inject a layer of colored dots into the score SVG.
//!
//! Used for the performance report — dots are placed below each note (same
//! positions as the JS overlay), with colors for correct / wrong timing /
//! wrong pitch / missed. Generated in Rust for better performance and a
//! single SVG to render (no WebView script).

use serde::Deserialize;

/// Vertical spacing between stacked dots (multiple passes) in SVG units.
const ROW_HEIGHT: f64 = 14.0;
/// Radius of each feedback dot.
const DOT_RADIUS: f64 = 4.0;

/// One overlay dot entry: position and list of colors (one per pass).
#[derive(Debug, Deserialize)]
struct OverlayDot {
    x: f64,
    y: f64,
    colors: Vec<String>,
}

/// Add the feedback overlay layer to the score SVG.
///
/// `overlay_dots_json` is a JSON array of `{ "x": number, "y": number, "colors": string[] }`
/// in SVG coordinates. Multiple colors at the same (x, y) are drawn as vertically stacked
/// circles (e.g. for multiple passes of the same measure).
///
/// Returns the original SVG with a new `<g id="feedback-overlay">...</g>` inserted
/// immediately before the closing `</svg>`.
pub fn add_feedback_overlay_to_svg(svg: &str, overlay_dots_json: &str) -> Result<String, String> {
    let dots: Vec<OverlayDot> = serde_json::from_str(overlay_dots_json)
        .map_err(|e| format!("Invalid overlay JSON: {e}"))?;

    let overlay_group = build_overlay_group(&dots);

    // Insert the overlay group before the closing </svg>
    let marker = "</svg>";
    let pos = svg
        .rfind(marker)
        .ok_or("SVG has no closing </svg> tag")?;
    let (before, after) = svg.split_at(pos);
    let mut out = String::with_capacity(before.len() + overlay_group.len() + after.len());
    out.push_str(before);
    out.push_str(&overlay_group);
    out.push_str(after);
    Ok(out)
}

fn build_overlay_group(dots: &[OverlayDot]) -> String {
    let mut g = String::from(r#"<g id="feedback-overlay">"#);
    for dot in dots {
        let x = dot.x;
        let mut y = dot.y;
        for color in &dot.colors {
            let fill = escape_attr(color);
            g.push_str(&format!(
                r#"<circle cx="{}" cy="{}" r="{}" fill="{}"/>"#,
                x, y, DOT_RADIUS, fill
            ));
            y += ROW_HEIGHT;
        }
    }
    g.push_str("</g>");
    g
}

/// Escape a string for use inside an XML attribute value.
fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_injects_group_before_closing_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100"/></svg>"#;
        let dots = r##"[{"x": 50, "y": 80, "colors": ["#4CAF50", "#FFC107"]}]"##;
        let out = add_feedback_overlay_to_svg(svg, dots).unwrap();
        assert!(out.contains(r#"<g id="feedback-overlay">"#));
        assert!(out.contains(r#"<circle cx="50" cy="80" r="4" fill="#4CAF50"/>"#));
        assert!(out.contains(r#"<circle cx="50" cy="94" r="4" fill="#FFC107"/>"#));
        assert!(out.ends_with("</svg>"));
    }
}
