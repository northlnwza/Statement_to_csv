// Decrypt the PDF and rebuild positioned text lines from content operators.
use std::collections::HashMap;
use std::error::Error;

use pdf::content::{Op, TextDrawAdjusted};
use pdf::file::FileOptions;
use pdf::font::{Font, ToUnicodeMap};
use pdf::object::Resolve;

/// One font's glyph→unicode decoder.
struct FontDec {
    map: Option<ToUnicodeMap>,
    two_byte: bool, // composite (Type0/CID) fonts use 2-byte codes
}

impl FontDec {
    fn build(font: &Font, r: &impl Resolve) -> Self {
        FontDec {
            map: font.to_unicode(r).and_then(|res| res.ok()),
            two_byte: font.is_cid(),
        }
    }

    fn decode_into(&self, bytes: &[u8], out: &mut String) {
        if self.two_byte {
            for ch in bytes.chunks(2) {
                let code = match ch {
                    [hi, lo] => ((*hi as u16) << 8) | *lo as u16,
                    [b] => *b as u16,
                    _ => continue,
                };
                match self.map.as_ref().and_then(|m| m.get(code)) {
                    Some(s) => out.push_str(s),
                    None => out.push('\u{fffd}'),
                }
            }
        } else {
            for &b in bytes {
                match self.map.as_ref().and_then(|m| m.get(b as u16)) {
                    Some(s) => out.push_str(s),
                    None => out.push(b as char),
                }
            }
        }
    }
}

/// Extract logical text lines from every page, in reading order.
/// A tab (`\t`) is inserted where a horizontal gap suggests a column boundary.
pub fn extract_lines(path: &str, password: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let file = FileOptions::cached().password(password.as_bytes()).open(path)?;
    let r = file.resolver();
    let mut lines = Vec::new();

    for page in file.pages() {
        let page = page?;
        let resources = match page.resources() {
            Ok(res) => res,
            Err(_) => continue,
        };

        // Build a decoder per font name on this page.
        let mut decoders: HashMap<String, FontDec> = HashMap::new();
        for (name, lazy) in resources.fonts.iter() {
            if let Ok(font) = lazy.load(&r) {
                decoders.insert(name.as_str().to_string(), FontDec::build(&font, &r));
            }
        }

        let content = match &page.contents {
            Some(c) => c,
            None => continue,
        };
        let ops = content.operations(&r)?;

        let mut cur: Option<&FontDec> = None;
        let mut font_size = 10.0_f32;
        let mut tx = 0.0_f32; // pen x in text space
        let mut ty = 0.0_f32; // pen y (baseline)
        let mut last_y = f32::NAN;
        let mut last_x_end = 0.0_f32;
        let mut line = String::new();

        macro_rules! flush {
            () => {{
                let t = line.trim_end();
                if !t.is_empty() {
                    lines.push(t.to_string());
                }
                line.clear();
            }};
        }

        for op in ops {
            match op {
                Op::BeginText => { tx = 0.0; ty = 0.0; }
                Op::TextFont { name, size } => {
                    font_size = size.abs().max(1.0);
                    cur = decoders.get(name.as_str());
                }
                Op::SetTextMatrix { matrix } => {
                    tx = matrix.e;
                    ty = matrix.f;
                    if newline_if_moved(ty, &mut last_y, &mut last_x_end, tx, font_size) { flush!(); }
                }
                Op::MoveTextPosition { translation } => {
                    tx += translation.x;
                    ty += translation.y;
                    if newline_if_moved(ty, &mut last_y, &mut last_x_end, tx, font_size) { flush!(); }
                }
                Op::TextNewline => {
                    ty -= font_size * 1.2;
                    if newline_if_moved(ty, &mut last_y, &mut last_x_end, tx, font_size) { flush!(); }
                }
                Op::TextDraw { text } => {
                    column_gap(tx, last_x_end, font_size, &mut line);
                    if let Some(d) = cur { d.decode_into(text.as_bytes(), &mut line); }
                    tx += text.as_bytes().len() as f32 * font_size * 0.5;
                    last_x_end = tx;
                }
                Op::TextDrawAdjusted { array } => {
                    column_gap(tx, last_x_end, font_size, &mut line);
                    for it in array {
                        match it {
                            TextDrawAdjusted::Text(s) => {
                                if let Some(d) = cur { d.decode_into(s.as_bytes(), &mut line); }
                                tx += s.as_bytes().len() as f32 * font_size * 0.5;
                            }
                            TextDrawAdjusted::Spacing(adj) => {
                                let shift = adj / 1000.0 * font_size;
                                tx -= shift;
                                if shift > font_size * 0.25 && !line.ends_with(' ') {
                                    line.push(' ');
                                }
                            }
                        }
                    }
                    last_x_end = tx;
                }
                _ => {}
            }
        }
        flush!();
    }

    Ok(lines)
}

/// Returns true when the baseline moved enough vertically to be a new line.
fn newline_if_moved(ty: f32, last_y: &mut f32, last_x_end: &mut f32, tx: f32, font_size: f32) -> bool {
    if last_y.is_nan() {
        *last_y = ty;
        *last_x_end = tx;
        return false;
    }
    if (ty - *last_y).abs() > font_size * 0.5 {
        *last_y = ty;
        *last_x_end = tx;
        true
    } else {
        false
    }
}

/// Insert a tab if the pen jumped right by more than ~half a glyph (column gap).
fn column_gap(tx: f32, last_x_end: f32, font_size: f32, line: &mut String) {
    if !line.is_empty() && tx - last_x_end > font_size * 0.4 && !line.ends_with('\t') {
        line.push('\t');
    }
}
