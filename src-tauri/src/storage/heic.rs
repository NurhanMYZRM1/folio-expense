//! HEIC/HEIF receipts (the iPhone camera default).
//!
//! WebViews cannot draw HEIC on every platform (WebView2 and WebKitGTK have no
//! decoder), and neither pdf-lib nor the OCR pipeline accept it. The original
//! is stored untouched; Rust decodes it once with a pure-Rust decoder and keeps
//! a JPEG rendition beside it that every other part of the app reads instead.
use crate::error::{AppError, Result};
use std::path::Path;

pub const MIME: &str = "image/heic";
/// Rendition file name inside `receipts/<id>/`.
pub const RENDITION: &str = "display.jpg";
/// Longest side of the rendition. Receipts stay sharp for OCR and print well;
/// larger photos are scaled down to keep the JPEG small.
const MAX_SIDE: u32 = 3000;
const MAX_PIXELS: u64 = 40_000_000;

/// True for the ISO-BMFF `ftyp` brands that mark HEIC/HEIF still images.
pub fn is_heif(header: &[u8]) -> bool {
    header.len() >= 12
        && &header[4..8] == b"ftyp"
        && matches!(
            &header[8..12],
            b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" | b"mif1" | b"msf1"
        )
}

/// Decodes a HEIC file and returns a JPEG rendition with the stored
/// orientation applied, scaled so the longest side is at most `MAX_SIDE`.
pub fn rendition(path: &Path) -> Result<Vec<u8>> {
    let unsupported = || AppError::new("FileUnsupported", "This HEIC image cannot be decoded.");
    // Check the declared size before decoding so a huge image is refused cheaply.
    let (w, h) = imagesize::size(path)
        .map(|s| (s.width as u64, s.height as u64))
        .map_err(|_| unsupported())?;
    if w == 0 || h == 0 || w * h > MAX_PIXELS {
        return Err(AppError::new(
            "FileTooLarge",
            "Receipt images must be under 40 megapixels.",
        ));
    }
    let bytes = std::fs::read(path)?;
    // The decoder reports malformed input as errors; a panic in it must still
    // only fail this one import, never the app.
    let decoded = std::panic::catch_unwind(|| heif_oxide::decode_bytes(&bytes))
        .map_err(|_| unsupported())?
        .map_err(|_| unsupported())?;
    if u64::from(decoded.width) * u64::from(decoded.height) > MAX_PIXELS {
        return Err(AppError::new(
            "FileTooLarge",
            "Receipt images must be under 40 megapixels.",
        ));
    }
    let (width, height) = (decoded.width, decoded.height);
    let rgb = match decoded.pixels {
        // Camera photos: 8-bit with no alpha, used as-is.
        heif_oxide::Pixels::Rgb8(pixels) => image::RgbImage::from_raw(width, height, pixels),
        _ => image::RgbaImage::from_raw(width, height, decoded.to_rgba8()).map(|p| flatten(&p)),
    }
    .ok_or_else(unsupported)?;
    let mut image = image::DynamicImage::ImageRgb8(rgb);
    if width.max(height) > MAX_SIDE {
        image = image.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle);
    }
    let rgb = image.into_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&rgb)
        .map_err(|_| unsupported())?;
    Ok(out.into_inner())
}

/// JPEG has no alpha; transparent areas become white like the viewer paper.
fn flatten(rgba: &image::RgbaImage) -> image::RgbImage {
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y);
        let a = u16::from(p[3]);
        let blend = |c: u8| ((u16::from(c) * a + 255 * (255 - a)) / 255) as u8;
        image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognises_heif_brands_only() {
        let header = |brand: &[u8; 4]| {
            let mut h = vec![0, 0, 0, 0x18];
            h.extend_from_slice(b"ftyp");
            h.extend_from_slice(brand);
            h
        };
        assert!(is_heif(&header(b"heic")));
        assert!(is_heif(&header(b"mif1")));
        assert!(!is_heif(&header(b"avif")));
        assert!(!is_heif(&header(b"isom")));
        assert!(!is_heif(b"\x89PNG\r\n\x1a\n\0\0\0\0"));
        assert!(!is_heif(b"ftyp"));
    }
    #[test]
    fn transparent_pixels_flatten_onto_white() {
        let mut rgba = image::RgbaImage::new(3, 1);
        rgba.put_pixel(0, 0, image::Rgba([0, 0, 0, 0]));
        rgba.put_pixel(1, 0, image::Rgba([0, 0, 0, 255]));
        rgba.put_pixel(2, 0, image::Rgba([255, 0, 0, 128]));
        let rgb = flatten(&rgba);
        assert_eq!(rgb.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(rgb.get_pixel(1, 0).0, [0, 0, 0]);
        assert_eq!(rgb.get_pixel(2, 0).0, [255, 127, 127]);
    }
}
