use crate::error::{Result, ThemeError};

/// Normalized sRGB color representation with components in range [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    /// Creates a new Color from RGBA floats in [0.0, 1.0].
    pub fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    /// Parse a CSS color string. Currently supports:
    /// - `#RGB`
    /// - `#RGBA`
    /// - `#RRGGBB`
    /// - `#RRGGBBAA`
    /// - `rgb(r, g, b)`
    /// - `rgba(r, g, b, a)`
    pub fn parse(s: &str) -> Result<Self> {
        let trimmed = s.trim();
        if let Some(hex) = trimmed.strip_prefix('#') {
            Self::parse_hex(hex)
        } else if trimmed.starts_with("rgb(") && trimmed.ends_with(')') {
            Self::parse_rgb_func(&trimmed[4..trimmed.len() - 1])
        } else if trimmed.starts_with("rgba(") && trimmed.ends_with(')') {
            Self::parse_rgba_func(&trimmed[5..trimmed.len() - 1])
        } else {
            Err(ThemeError::InvalidColor(format!(
                "Unsupported color format '{}' (expected #hex or rgb/rgba)",
                s
            )))
        }
    }

    fn parse_hex(hex: &str) -> Result<Self> {
        let len = hex.len();
        match len {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let g = u8::from_str_radix(&hex[1..2], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let b = u8::from_str_radix(&hex[2..3], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                Ok(Self::new(
                    (r * 17) as f64 / 255.0,
                    (g * 17) as f64 / 255.0,
                    (b * 17) as f64 / 255.0,
                    1.0,
                ))
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let g = u8::from_str_radix(&hex[1..2], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let b = u8::from_str_radix(&hex[2..3], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let a = u8::from_str_radix(&hex[3..4], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                Ok(Self::new(
                    (r * 17) as f64 / 255.0,
                    (g * 17) as f64 / 255.0,
                    (b * 17) as f64 / 255.0,
                    (a * 17) as f64 / 255.0,
                ))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                Ok(Self::new(
                    r as f64 / 255.0,
                    g as f64 / 255.0,
                    b as f64 / 255.0,
                    1.0,
                ))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                let a = u8::from_str_radix(&hex[6..8], 16)
                    .map_err(|_| ThemeError::InvalidColor(hex.to_string()))?;
                Ok(Self::new(
                    r as f64 / 255.0,
                    g as f64 / 255.0,
                    b as f64 / 255.0,
                    a as f64 / 255.0,
                ))
            }
            _ => Err(ThemeError::InvalidColor(format!(
                "Invalid hex length {}: #{}",
                len, hex
            ))),
        }
    }

    fn parse_rgb_func(inner: &str) -> Result<Self> {
        let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            return Err(ThemeError::InvalidColor(format!(
                "rgb() expects 3 arguments, got {}",
                parts.len()
            )));
        }
        let r: f64 = parts[0]
            .parse()
            .map_err(|_| ThemeError::InvalidColor(inner.to_string()))?;
        let g: f64 = parts[1]
            .parse()
            .map_err(|_| ThemeError::InvalidColor(inner.to_string()))?;
        let b: f64 = parts[2]
            .parse()
            .map_err(|_| ThemeError::InvalidColor(inner.to_string()))?;
        Ok(Self::new(r / 255.0, g / 255.0, b / 255.0, 1.0))
    }

    fn parse_rgba_func(inner: &str) -> Result<Self> {
        let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        if parts.len() != 4 {
            return Err(ThemeError::InvalidColor(format!(
                "rgba() expects 4 arguments, got {}",
                parts.len()
            )));
        }
        let r: f64 = parts[0]
            .parse()
            .map_err(|_| ThemeError::InvalidColor(inner.to_string()))?;
        let g: f64 = parts[1]
            .parse()
            .map_err(|_| ThemeError::InvalidColor(inner.to_string()))?;
        let b: f64 = parts[2]
            .parse()
            .map_err(|_| ThemeError::InvalidColor(inner.to_string()))?;
        let a: f64 = parts[3]
            .parse()
            .map_err(|_| ThemeError::InvalidColor(inner.to_string()))?;
        Ok(Self::new(r / 255.0, g / 255.0, b / 255.0, a))
    }

    /// Blend this color (as foreground) over a solid background color.
    pub fn blend_over(&self, bg: &Color) -> Color {
        if (self.a - 1.0).abs() < f64::EPSILON {
            return *self;
        }
        let alpha = self.a;
        let inv_alpha = 1.0 - alpha;
        Color::new(
            self.r * alpha + bg.r * inv_alpha,
            self.g * alpha + bg.g * inv_alpha,
            self.b * alpha + bg.b * inv_alpha,
            1.0,
        )
    }

    /// Calculate the WCAG 2.1 relative luminance.
    ///
    /// Formula: L = 0.2126 * R + 0.7152 * G + 0.0722 * B
    /// where component C = (c <= 0.04045) ? c / 12.92 : ((c + 0.055) / 1.055) ^ 2.4
    pub fn relative_luminance(&self) -> f64 {
        fn adjust(c: f64) -> f64 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        let r = adjust(self.r);
        let g = adjust(self.g);
        let b = adjust(self.b);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Calculate WCAG 2.1 contrast ratio against another color.
    /// Returns a value between 1.0 and 21.0.
    pub fn contrast_ratio(&self, other: &Color) -> f64 {
        let fg = self.blend_over(other);
        let l1 = fg.relative_luminance();
        let l2 = other.relative_luminance();
        let lighter = l1.max(l2);
        let darker = l1.min(l2);
        (lighter + 0.05) / (darker + 0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex() {
        let black = Color::parse("#000000").unwrap();
        assert_eq!(black, Color::new(0.0, 0.0, 0.0, 1.0));

        let white = Color::parse("#ffffff").unwrap();
        assert_eq!(white, Color::new(1.0, 1.0, 1.0, 1.0));

        let short = Color::parse("#fff").unwrap();
        assert_eq!(short, Color::new(1.0, 1.0, 1.0, 1.0));

        let with_alpha = Color::parse("#00000080").unwrap();
        assert_eq!(with_alpha.a, 128.0 / 255.0);
    }

    #[test]
    fn test_wcag_contrast_black_white() {
        let black = Color::parse("#000000").unwrap();
        let white = Color::parse("#ffffff").unwrap();
        let ratio = black.contrast_ratio(&white);
        assert!((ratio - 21.0).abs() < 0.1);
    }

    #[test]
    fn test_wcag_contrast_identical() {
        let gray = Color::parse("#808080").unwrap();
        let ratio = gray.contrast_ratio(&gray);
        assert!((ratio - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_invalid_color() {
        assert!(Color::parse("not-a-color").is_err());
        assert!(Color::parse("#12").is_err());
        assert!(Color::parse("#12345").is_err());
    }
}
