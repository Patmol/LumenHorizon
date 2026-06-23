use image::{codecs::png::PngEncoder, ColorType, ImageEncoder, Rgba};
use thiserror::Error;

use crate::science::classify_dark_sky;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderPixel {
    pub radiance: Option<f32>,
    pub rejected: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderError {
    #[error("failed to encode PNG tile")]
    EncodePng,

    #[error("invalid dark-sky class color '{color_hex}'")]
    InvalidClassColor { color_hex: &'static str },

    #[error("tile size must be greater than zero")]
    InvalidTileSize,

    #[error("expected {expected} pixels for tile size {tile_size}, got {actual}")]
    PixelCountMismatch {
        tile_size: u16,
        expected: usize,
        actual: usize,
    },
}

pub fn render_png_tile(tile_size: u16, pixels: &[RenderPixel]) -> Result<Vec<u8>, RenderError> {
    if tile_size == 0 {
        return Err(RenderError::InvalidTileSize);
    }

    let width = u32::from(tile_size);
    let height = u32::from(tile_size);
    let expected = usize::from(tile_size) * usize::from(tile_size);

    if pixels.len() != expected {
        return Err(RenderError::PixelCountMismatch {
            tile_size,
            expected,
            actual: pixels.len(),
        });
    }

    let mut rgba = Vec::with_capacity(expected * 4);

    for pixel in pixels {
        let color = render_pixel(*pixel)?;
        rgba.extend_from_slice(&color.0);
    }

    let mut png = Vec::new();
    let encoder = PngEncoder::new(&mut png);
    encoder
        .write_image(&rgba, width, height, ColorType::Rgba8.into())
        .map_err(|_| RenderError::EncodePng)?;

    Ok(png)
}

pub fn renderable_pixel_count(pixels: &[RenderPixel]) -> u32 {
    pixels
        .iter()
        .filter(|pixel| pixel.radiance.is_some() && !pixel.rejected)
        .count() as u32
}

pub fn render_pixel(pixel: RenderPixel) -> Result<Rgba<u8>, RenderError> {
    if pixel.rejected {
        return Ok(Rgba([0, 0, 0, 0]));
    }

    let Some(radiance) = pixel.radiance else {
        return Ok(Rgba([0, 0, 0, 0]));
    };

    let Some(class) = classify_dark_sky(radiance) else {
        return Ok(Rgba([0, 0, 0, 0]));
    };

    let [red, green, blue] = parse_color_hex(class.color_hex)?;
    Ok(Rgba([red, green, blue, 255]))
}

fn parse_color_hex(color_hex: &'static str) -> Result<[u8; 3], RenderError> {
    let value = color_hex
        .strip_prefix('#')
        .ok_or(RenderError::InvalidClassColor { color_hex })?;

    if value.len() != 6 {
        return Err(RenderError::InvalidClassColor { color_hex });
    }

    let red = parse_hex_byte(&value[0..2], color_hex)?;
    let green = parse_hex_byte(&value[2..4], color_hex)?;
    let blue = parse_hex_byte(&value[4..6], color_hex)?;

    Ok([red, green, blue])
}

fn parse_hex_byte(value: &str, color_hex: &'static str) -> Result<u8, RenderError> {
    u8::from_str_radix(value, 16).map_err(|_| RenderError::InvalidClassColor { color_hex })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_rejected_and_nodata_pixels_as_transparent() {
        assert_eq!(
            render_pixel(RenderPixel {
                radiance: Some(1.0),
                rejected: true,
            })
            .unwrap(),
            Rgba([0, 0, 0, 0])
        );

        assert_eq!(
            render_pixel(RenderPixel {
                radiance: None,
                rejected: false,
            })
            .unwrap(),
            Rgba([0, 0, 0, 0])
        );
    }

    #[test]
    fn counts_renderable_pixels() {
        let pixels = [
            RenderPixel {
                radiance: Some(0.1),
                rejected: false,
            },
            RenderPixel {
                radiance: Some(0.5),
                rejected: true,
            },
            RenderPixel {
                radiance: None,
                rejected: false,
            },
        ];

        assert_eq!(renderable_pixel_count(&pixels), 1);
    }

    #[test]
    fn renders_valid_radiance_to_class_color() {
        assert_eq!(
            render_pixel(RenderPixel {
                radiance: Some(0.1),
                rejected: false,
            })
            .unwrap(),
            Rgba([0x05, 0x07, 0x0d, 255])
        );

        assert_eq!(
            render_pixel(RenderPixel {
                radiance: Some(50.0),
                rejected: false,
            })
            .unwrap(),
            Rgba([0xf2, 0xef, 0xe8, 255])
        );
    }

    #[test]
    fn rejects_mismatched_pixel_count() {
        assert!(matches!(
            render_png_tile(
                2,
                &[RenderPixel {
                    radiance: Some(0.1),
                    rejected: false,
                }]
            ),
            Err(RenderError::PixelCountMismatch {
                tile_size: 2,
                expected: 4,
                actual: 1,
            })
        ));
    }

    #[test]
    fn renders_deterministic_png_bytes_for_same_pixels() {
        let pixels = [
            RenderPixel {
                radiance: Some(0.1),
                rejected: false,
            },
            RenderPixel {
                radiance: Some(0.5),
                rejected: false,
            },
            RenderPixel {
                radiance: None,
                rejected: false,
            },
            RenderPixel {
                radiance: Some(50.0),
                rejected: true,
            },
        ];

        let first = render_png_tile(2, &pixels).unwrap();
        let second = render_png_tile(2, &pixels).unwrap();

        assert_eq!(first, second);
        assert!(first.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn rendered_png_dimensions_match_tile_size() {
        let pixels = vec![
            RenderPixel {
                radiance: Some(0.1),
                rejected: false,
            };
            9
        ];

        let png = render_png_tile(3, &pixels).unwrap();
        let image = image::load_from_memory(&png).unwrap();

        assert_eq!(image.width(), 3);
        assert_eq!(image.height(), 3);
    }
}
