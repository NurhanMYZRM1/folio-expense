// Local OCR reads a receipt in up to three passes and keeps the best one.
// The first pass (single-column layout) is right for most receipts, but on a
// tilted photo Tesseract sometimes treats the right-aligned price column as a
// separate column and drops it, leaving `TOTAL` with no amount. Faded thermal
// paper can come back empty because the gray paper reads as a picture. The
// later passes (uniform-block layout, then a black-and-white image) recover
// those cases; they only run when the earlier result looks incomplete.

// Same total labels the Rust extractor recognizes (`offline.rs`).
const TOTAL_LABEL = /T[O0]TA[L1I]|AMOUNT DUE|BALANCE DUE|AMOUNT PAYABLE|\bJUMLAH\b/i;
const SUBTOTAL_LABEL = /SUB\s*-?\s*T[O0]TA[L1I]/i;
const AMOUNT = /\d[.,]\d{2}\b/;
const AMOUNT_ONLY = /^[^\p{L}\d]*[\p{L}$€£¥₹฿₩:]{0,4}\s*[\d,.\s()-]*\d[.,]\d{2}\s*$/u;

export type OcrPass = { text: string; confidence: number };

/** Whether a pass found a total label with its amount (same or next line). */
export function hasTotalAmount(text: string): boolean {
  const lines = text
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);
  return lines.some(
    (line, i) =>
      TOTAL_LABEL.test(line) &&
      !SUBTOTAL_LABEL.test(line) &&
      (AMOUNT.test(line) || AMOUNT_ONLY.test(lines[i + 1] ?? '')),
  );
}

/**
 * Higher is better: a readable total matters most, then priced lines, then
 * confidence. Tesseract can report high confidence for an empty page (it was
 * sure there was no text), so confidence only counts in proportion to how
 * much text the pass actually read.
 */
export function scorePass({ text, confidence }: OcrPass): number {
  const lines = text.split('\n').filter((l) => l.trim());
  const pricedLines = lines.filter((l) => AMOUNT.test(l)).length;
  return (
    (hasTotalAmount(text) ? 1000 : 0) +
    Math.min(pricedLines, 40) * 10 +
    confidence * Math.min(1, lines.length / 5)
  );
}

/** A pass this good needs no retry. */
export function isGoodPass(pass: OcrPass): boolean {
  return hasTotalAmount(pass.text) && pass.confidence >= 75;
}

export function bestPass(passes: OcrPass[]): OcrPass {
  return passes.reduce((best, pass) => (scorePass(pass) > scorePass(best) ? pass : best));
}
