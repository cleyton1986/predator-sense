/// Scales every `font-size: Npx` declaration in the given CSS by `scale`.
/// Clamped to [0.8, 1.5] so extreme values can't be persisted into a broken layout.
pub fn scale_css(css: &str, scale: f64) -> String {
    let scale = scale.clamp(0.8, 1.5);
    let needle = "font-size:";
    let mut out = String::with_capacity(css.len());
    let mut rest = css;

    while let Some(pos) = rest.find(needle) {
        let (before, after) = rest.split_at(pos);
        out.push_str(before);
        out.push_str(needle);

        let after_needle = &after[needle.len()..];
        let value_start = after_needle.len() - after_needle.trim_start().len();
        let trimmed = after_needle.trim_start();

        match trimmed.find("px") {
            Some(px_pos) if trimmed[..px_pos].chars().all(|c| c.is_ascii_digit() || c == '.') && !trimmed[..px_pos].is_empty() => {
                let num_str = &trimmed[..px_pos];
                let px: f64 = num_str.parse().unwrap_or(0.0);
                let scaled = (px * scale).round() as i64;
                out.push_str(&after_needle[..value_start]);
                out.push_str(&format!("{}px", scaled));
                rest = &trimmed[px_pos + 2..];
            }
            _ => {
                // Not a plain "Npx" value (shouldn't happen in this stylesheet) - leave untouched.
                out.push_str(&after_needle[..value_start]);
                rest = trimmed;
            }
        }
    }

    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_simple_value() {
        let css = ".foo { font-size: 12px; }";
        let scaled = scale_css(css, 1.5);
        assert_eq!(scaled, ".foo { font-size: 18px; }");
    }

    #[test]
    fn scales_multiple_values() {
        let css = ".a { font-size: 10px; } .b { font-size: 20px; }";
        let scaled = scale_css(css, 1.5);
        assert_eq!(scaled, ".a { font-size: 15px; } .b { font-size: 30px; }");
    }

    #[test]
    fn clamps_extreme_scale() {
        let css = ".a { font-size: 10px; }";
        let scaled = scale_css(css, 5.0);
        assert_eq!(scaled, ".a { font-size: 15px; }");
    }

    #[test]
    fn identity_at_scale_one() {
        let css = ".a { font-size: 13px; }";
        assert_eq!(scale_css(css, 1.0), css);
    }
}
