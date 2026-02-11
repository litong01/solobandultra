//! SVG builder — accumulates SVG elements and produces the final string.
//!
//! Also contains VexFlow outline → SVG path conversion helpers.

use super::constants::*;
use super::glyphs::*;

// ═══════════════════════════════════════════════════════════════════════
// SvgBuilder
// ═══════════════════════════════════════════════════════════════════════

pub(super) struct SvgBuilder {
    pub(super) elements: Vec<String>,
    width: f64,
    height: f64,
}

impl SvgBuilder {
    pub(super) fn new(width: f64, height: f64) -> Self {
        Self {
            elements: Vec::new(),
            width,
            height,
        }
    }

    pub(super) fn build(self) -> String {
        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="{}" height="{}" style="font-family: 'Georgia', 'Times New Roman', serif;">"#,
            self.width, self.height, self.width, self.height
        );
        svg.push('\n');
        for el in &self.elements {
            svg.push_str("  ");
            svg.push_str(el);
            svg.push('\n');
        }
        svg.push_str("</svg>\n");
        svg
    }

    pub(super) fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: &str, width: f64) {
        self.elements.push(format!(
            r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="{:.1}" stroke-linecap="round"/>"#,
            x1, y1, x2, y2, color, width
        ));
    }

    pub(super) fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str, stroke_width: f64) {
        if stroke_width > 0.0 {
            self.elements.push(format!(
                r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{}" stroke="{}" stroke-width="{:.1}"/>"#,
                x, y, w, h, fill, stroke, stroke_width
            ));
        } else {
            self.elements.push(format!(
                r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{}"/>"#,
                x, y, w, h, fill
            ));
        }
    }

    pub(super) fn circle(&mut self, cx: f64, cy: f64, r: f64, fill: &str) {
        self.elements.push(format!(
            r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{}"/>"#,
            cx, cy, r, fill
        ));
    }

    pub(super) fn text(&mut self, x: f64, y: f64, content: &str, size: f64, weight: &str, fill: &str, anchor: &str) {
        let escaped = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        self.elements.push(format!(
            r#"<text x="{:.1}" y="{:.1}" font-size="{:.0}" font-weight="{}" fill="{}" text-anchor="{}">{}</text>"#,
            x, y, size, weight, fill, anchor, escaped
        ));
    }

    /// Render chord symbols matching OSMD style: Times New Roman, normal weight, no letter-spacing
    pub(super) fn chord_text(&mut self, x: f64, y: f64, content: &str, size: f64, fill: &str) {
        let escaped = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        self.elements.push(format!(
            r#"<text x="{:.1}" y="{:.1}" font-family="Times New Roman, serif" font-size="{:.0}" font-weight="normal" fill="{}" text-anchor="start">{}</text>"#,
            x, y, size, fill, escaped
        ));
    }

    pub(super) fn path(&mut self, d: &str, fill: &str, stroke: &str, stroke_width: f64) {
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="{}" stroke-width="{:.1}" stroke-linecap="round"/>"#,
            d, fill, stroke, stroke_width
        ));
    }

    pub(super) fn notehead(&mut self, cx: f64, cy: f64, filled: bool, _is_whole: bool) {
        let rx = NOTEHEAD_RX;
        let ry = NOTEHEAD_RY;
        if filled {
            self.elements.push(format!(
                r#"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{:.1}" fill="{}" stroke="none" stroke-width="0" transform="rotate(-15,{:.1},{:.1})"/>"#,
                cx, cy, rx, ry, NOTE_COLOR, cx, cy
            ));
        } else {
            let sw = 2.0;
            self.elements.push(format!(
                r#"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{:.1}" fill="none" stroke="{}" stroke-width="{:.1}" transform="rotate(-15,{:.1},{:.1})"/>"#,
                cx, cy, rx - sw / 2.0, ry - sw / 2.0, NOTE_COLOR, sw, cx, cy
            ));
        }
    }

    pub(super) fn beam_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, thickness: f64) {
        let half = thickness / 2.0;
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt().max(0.1);
        let nx = -dy / len * half;
        let ny = dx / len * half;

        let path = format!(
            "M{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} Z",
            x1 + nx, y1 + ny,
            x2 + nx, y2 + ny,
            x2 - nx, y2 - ny,
            x1 - nx, y1 - ny,
        );
        self.elements.push(format!(
            r#"<path d="{}" fill="{}"/>"#,
            path, NOTE_COLOR
        ));
    }

    pub(super) fn treble_clef(&mut self, x: f64, y: f64) {
        let scale = 0.243;
        let tx = x - 138.0 * scale;
        let ty = y - 148.0 * scale - 4.0;

        let p1 = "M156.716,61.478c-4.111,6.276-8.881,11.511-14.212,15.609\
l-8.728,6.962c-13.339,11.855-22.937,21.433-28.542,28.464\
c-10.209,12.788-15.806,25.779-16.65,38.611c-0.942,14.473,3.187,28.21,12.275,40.84\
c9.636,13.458,21.8,20.754,36.164,21.69c3.291,0.218,6.897,0.182,9.896-0.015\
l-1.121-10.104c-2.09,0.192-4.306,0.223-6.628,0.068\
c-9.437-0.617-17.864-4.511-25.064-11.573c-7.524-7.333-10.895-15.415-10.287-24.7\
c1.149-17.59,12.562-35.004,33.925-51.792l9.543-7.599\
c8.394-7.174,15.192-16.191,20.216-26.825c4.971-10.556,7.886-21.983,8.673-33.96\
c0.466-7.037-0.513-15.775-2.874-25.965c-3.241-13.839-7.854-20.765-14.136-21.179\
c-2.232-0.138-4.676,0.986-7.658,3.617c-7.252,6.548-12.523,14.481-15.683,23.542\
c-2.438,6.926-4.057,16.189-4.805,27.529c-0.313,4.72,0.313,13.438,1.805,23.962\
l8.844-8.192c-0.028-1.183,0.005-2.413,0.096-3.703\
c0.466-7.221,2.289-15.062,5.394-23.293c3.956-10.296,7.689-13.409,10.133-14.204\
c0.668-0.218,1.32-0.298,2.015-0.254c3.185,0.212,6.358,1.559,5.815,9.979\
C164.664,46.132,161.831,53.693,156.716,61.478z";

        let p2 = "M164.55,209.161c5.728-2.568,10.621-6.478,14.576-11.651\
c5.055-6.561,7.897-14.316,8.467-23.047c0.72-10.719-1.854-20.438-7.617-28.895\
c-6.322-9.264-14.98-14.317-25.745-15.026c-1.232-0.081-2.543-0.075-3.895,0.025\
l-2.304-17.191l-9.668,7.112l1.483,12.194\
c-5.789,2.393-10.827,6.17-15.017,11.255c-4.823,5.924-7.508,12.443-7.964,19.382\
c-0.466,7.208,1.142,13.81,4.782,19.583c1.895,3.081,4.507,5.82,7.498,8.058\
c4.906,3.65,10.563,3.376,11.459,1.393c0.906-1.983-2.455-5.095-5.09-9.248\
c-1.502-2.351-2.242-5.173-2.242-8.497c0-7.053,4.256-13.116,10.317-15.799\
l5.673,44.211l1.325,10.258c0.864,4.873,1.719,9.725,2.537,14.52\
c1,6.488,1.352,12.112,1.041,16.715c-0.419,6.375-2.408,11.584-5.919,15.493\
c-2.234,2.485-4.844,4.055-7.795,4.925c3.961-3.962,6.414-9.43,6.414-15.478\
c0-12.075-9.792-21.872-21.87-21.872c-3.353,0-6.491,0.812-9.329,2.159\
c-0.36,0.155-0.699,0.388-1.054,0.574c-0.779,0.425-1.559,0.85-2.286,1.362\
c-0.249,0.187-0.487,0.403-0.732,0.605c-4.888,3.816-8.091,9.616-8.375,16.229\
c0,0.01-0.011,0.021-0.011,0.031c0,0.005,0,0.01,0,0.016\
c-0.013,0.311-0.09,0.59-0.09,0.896c0,0.259,0.067,0.492,0.078,0.74\
c-0.011,7.084,2.933,13.179,8.839,18.118c5.584,4.666,12.277,7.28,19.892,7.777\
c4.327,0.28,8.505-0.217,12.407-1.485c3.189-1.041,6.275-2.62,9.149-4.687\
c6.96-5.022,10.75-11.584,11.272-19.532c0.399-6.063,0.094-13.235-0.937-21.411\
l-2.838-18.429l-7.156-52.899c7.984,1.532,14.027,8.543,14.027,16.968\
c0,5.986-1.937,15.431-5.551,20.376L164.55,209.161z";

        self.elements.push(format!(
            r#"<g transform="translate({:.2},{:.2}) scale({})"><path d="{}" fill="{}"/><path d="{}" fill="{}"/></g>"#,
            tx, ty, scale, p1, NOTE_COLOR, p2, NOTE_COLOR
        ));
    }

    pub(super) fn bass_clef(&mut self, x: f64, y: f64) {
        let scale = 0.06;
        let tx = x - 176.0 * scale - 2.0;
        let ty = y - 169.0 * scale;

        let p1 = "M176.014,0l-2.823,0.01\
C89.091,1.164,20.78,63.557,15.904,118.564\
c-3.125,35.072,4.693,63.941,22.568,83.494\
c16.307,17.803,39.765,26.836,69.727,26.836\
c31.095,0,61.603-29.77,61.603-60.106\
c0-30.803-25.076-55.869-55.888-55.869\
c-16.569,0-27.575,7.323-34.858,12.179\
c-2.853,1.892-5.796,3.854-7.121,3.854\
c-0.446,0-1.477-1.184-2.458-5.635\
c-3.399-15.335,1.902-33.644,14.212-48.98\
c10.399-12.978,34.858-34.726,81.876-34.726\
c65.67,0,101.833,52.894,101.833,148.952\
c0,192.852-165.703,271.845-216.483,291.459\
c-10.398,4.016-13.778,12.716-12.492,19.553\
C39.828,507.002,45.947,512,53.686,512\
c2.448,0,5.037-0.496,7.657-1.477l5.807-2.165\
C262.916,435.82,362.19,326.247,362.19,182.648\
C362.19,57.164,265.688,0,176.014,0z";

        let p2 = "M455.486,126.84\
c22.771,0,41.282-18.522,41.282-41.292\
c0-22.76-18.512-41.271-41.282-41.271\
c-22.759,0-41.281,18.511-41.281,41.271\
C414.205,108.318,432.726,126.84,455.486,126.84z";

        let p3 = "M455.486,211.365\
c-22.759,0-41.281,18.522-41.281,41.282\
c0,22.77,18.522,41.281,41.281,41.281\
c22.771,0,41.282-18.511,41.282-41.281\
C496.768,229.887,478.256,211.365,455.486,211.365z";

        self.elements.push(format!(
            r#"<g transform="translate({:.2},{:.2}) scale({})"><path d="{}" fill="{}"/><path d="{}" fill="{}"/><path d="{}" fill="{}"/></g>"#,
            tx, ty, scale, p1, NOTE_COLOR, p2, NOTE_COLOR, p3, NOTE_COLOR
        ));
    }

    pub(super) fn alto_clef(&mut self, _x: f64, y: f64) {
        let x = _x;
        self.rect(x - 2.0, y - 20.0, 3.0, 80.0, NOTE_COLOR, "none", 0.0);
        self.rect(x + 4.0, y - 20.0, 1.5, 80.0, NOTE_COLOR, "none", 0.0);
    }

    /// Render a sharp accidental glyph using VexFlow font outline.
    pub(super) fn sharp_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(SHARP_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }

    /// Render a flat accidental glyph using VexFlow font outline.
    pub(super) fn flat_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(FLAT_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }

    /// Render a natural accidental glyph using VexFlow font outline.
    pub(super) fn natural_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(NATURAL_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }

    /// Render a double-sharp accidental glyph using VexFlow font outline.
    pub(super) fn double_sharp_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(DOUBLE_SHARP_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }

    /// Render a double-flat accidental glyph using VexFlow font outline.
    pub(super) fn double_flat_glyph(&mut self, x: f64, y: f64) {
        let s = ACCIDENTAL_GLYPH_SCALE;
        let path = vexflow_outline_to_svg(DOUBLE_FLAT_GLYPH, s, x, y);
        self.elements.push(format!(
            r#"<path d="{}" fill="{}" stroke="none"/>"#,
            path, NOTE_COLOR
        ));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// VexFlow outline → SVG path converters
// ═══════════════════════════════════════════════════════════════════════

/// Convert a VexFlow glyph outline string to an SVG path `d` attribute.
///
/// The y-axis is inverted (font y goes up; SVG y goes down).
pub(super) fn vexflow_outline_to_svg(outline: &str, scale: f64, ox: f64, oy: f64) -> String {
    let tokens: Vec<&str> = outline.split_whitespace().collect();
    let mut path = String::with_capacity(outline.len());
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i] {
            "m" if i + 2 < tokens.len() => {
                let x: f64 = tokens[i + 1].parse().unwrap_or(0.0);
                let y: f64 = tokens[i + 2].parse().unwrap_or(0.0);
                path.push_str(&format!("M{:.1} {:.1}", ox + x * scale, oy - y * scale));
                i += 3;
            }
            "l" if i + 2 < tokens.len() => {
                let x: f64 = tokens[i + 1].parse().unwrap_or(0.0);
                let y: f64 = tokens[i + 2].parse().unwrap_or(0.0);
                path.push_str(&format!("L{:.1} {:.1}", ox + x * scale, oy - y * scale));
                i += 3;
            }
            "b" if i + 6 < tokens.len() => {
                let ex: f64  = tokens[i + 1].parse().unwrap_or(0.0);
                let ey: f64  = tokens[i + 2].parse().unwrap_or(0.0);
                let c1x: f64 = tokens[i + 3].parse().unwrap_or(0.0);
                let c1y: f64 = tokens[i + 4].parse().unwrap_or(0.0);
                let c2x: f64 = tokens[i + 5].parse().unwrap_or(0.0);
                let c2y: f64 = tokens[i + 6].parse().unwrap_or(0.0);
                path.push_str(&format!(
                    "C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
                    ox + c1x * scale, oy - c1y * scale,
                    ox + c2x * scale, oy - c2y * scale,
                    ox + ex * scale,  oy - ey * scale,
                ));
                i += 7;
            }
            "q" if i + 4 < tokens.len() => {
                let ex: f64 = tokens[i + 1].parse().unwrap_or(0.0);
                let ey: f64 = tokens[i + 2].parse().unwrap_or(0.0);
                let cx: f64 = tokens[i + 3].parse().unwrap_or(0.0);
                let cy: f64 = tokens[i + 4].parse().unwrap_or(0.0);
                path.push_str(&format!(
                    "Q{:.1} {:.1} {:.1} {:.1}",
                    ox + cx * scale, oy - cy * scale,
                    ox + ex * scale, oy - ey * scale,
                ));
                i += 5;
            }
            _ => { i += 1; }
        }
    }

    path.push('Z');
    path
}

/// Convert a VexFlow font outline to a standard SVG path (Y-negated, no offset).
pub(super) fn vf_outline_to_svg(outline: &str, scale: f64) -> String {
    let mut result = String::with_capacity(outline.len());
    let parts: Vec<&str> = outline.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "m" if i + 2 < parts.len() => {
                let x: f64 = parts[i+1].parse().unwrap_or(0.0) * scale;
                let y: f64 = parts[i+2].parse().unwrap_or(0.0) * -scale;
                result.push_str(&format!("M{:.2},{:.2}", x, y));
                i += 3;
            }
            "l" if i + 2 < parts.len() => {
                let x: f64 = parts[i+1].parse().unwrap_or(0.0) * scale;
                let y: f64 = parts[i+2].parse().unwrap_or(0.0) * -scale;
                result.push_str(&format!("L{:.2},{:.2}", x, y));
                i += 3;
            }
            "b" if i + 6 < parts.len() => {
                let ex: f64 = parts[i+1].parse().unwrap_or(0.0) * scale;
                let ey: f64 = parts[i+2].parse().unwrap_or(0.0) * -scale;
                let c1x: f64 = parts[i+3].parse().unwrap_or(0.0) * scale;
                let c1y: f64 = parts[i+4].parse().unwrap_or(0.0) * -scale;
                let c2x: f64 = parts[i+5].parse().unwrap_or(0.0) * scale;
                let c2y: f64 = parts[i+6].parse().unwrap_or(0.0) * -scale;
                result.push_str(&format!("C{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
                    c1x, c1y, c2x, c2y, ex, ey));
                i += 7;
            }
            _ => { i += 1; }
        }
    }
    result.push('Z');
    result
}

// ═══════════════════════════════════════════════════════════════════════
// Empty SVG fallback
// ═══════════════════════════════════════════════════════════════════════

pub(super) fn empty_svg(message: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 400 100\">\
         <text x=\"200\" y=\"50\" text-anchor=\"middle\" font-size=\"14\" fill=\"gray\">{}</text>\
         </svg>",
        message
    )
}
