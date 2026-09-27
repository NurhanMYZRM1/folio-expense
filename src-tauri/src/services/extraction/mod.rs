pub mod normalizer;
pub mod offline;
pub mod online;
use crate::{domain::extraction::Extraction, error::Result};
pub struct ExtractionInput<'a> {
    pub raw_text: &'a str,
    pub images: &'a [String],
    pub default_currency: &'a str,
}
pub trait ReceiptExtractor {
    fn extract(&self, input: &ExtractionInput<'_>) -> Result<Extraction>;
}
