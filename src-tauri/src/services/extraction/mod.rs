pub mod normalizer;
pub mod offline;
pub mod online;
pub mod sanitize;
use crate::{domain::extraction::Extraction, error::Result};
pub struct ExtractionInput<'a> {
    pub raw_text: &'a str,
    pub images: &'a [String],
    pub default_currency: &'a str,
    /// Tesseract's mean page confidence (0–100) for `raw_text`, when known.
    /// Used to keep fields from a blurry/unreadable scan in review.
    pub ocr_confidence: Option<f64>,
}
pub trait ReceiptExtractor {
    fn extract(&self, input: &ExtractionInput<'_>) -> Result<Extraction>;
}
