import { GlobalWorkerOptions, getDocument, type PDFDocumentProxy } from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import type { ReceiptContent } from '../bindings/generated';
GlobalWorkerOptions.workerSrc = workerUrl;
import { decodeBase64 } from './encoding';
export async function loadPdf(content: ReceiptContent): Promise<PDFDocumentProxy> {
  return getDocument({
    data: decodeBase64(content.base64),
    cMapUrl: '/pdf/cmaps/',
    cMapPacked: true,
    standardFontDataUrl: '/pdf/standard_fonts/',
    wasmUrl: '/pdf/wasm/',
    useSystemFonts: true,
  }).promise;
}
export async function renderPdfPage(
  pdf: PDFDocumentProxy,
  pageNumber: number,
  limit = 2200,
): Promise<HTMLCanvasElement> {
  const page = await pdf.getPage(pageNumber),
    original = page.getViewport({ scale: 1 });
  const viewport = page.getViewport({
    scale: Math.min(2.5, limit / Math.max(original.width, original.height)),
  });
  const canvas = document.createElement('canvas');
  canvas.width = Math.ceil(viewport.width);
  canvas.height = Math.ceil(viewport.height);
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Unable to render the receipt.');
  await page.render({ canvas, canvasContext: context, viewport }).promise;
  page.cleanup();
  return canvas;
}
export async function imageCanvas(
  content: ReceiptContent,
  limit = 2400,
): Promise<HTMLCanvasElement> {
  const bytes = decodeBase64(content.base64);
  const bitmap = await createImageBitmap(new Blob([bytes as BlobPart], { type: content.mimeType }));
  const scale = Math.min(1, limit / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(bitmap.width * scale));
  canvas.height = Math.max(1, Math.round(bitmap.height * scale));
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Unable to open this receipt image.');
  context.fillStyle = '#fff';
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
  return canvas;
}
function percentileLevel(histogram: Uint32Array, pixelCount: number, p: number): number {
  const target = Math.floor(p * (pixelCount - 1));
  let cumulative = 0;
  for (let level = 0; level < 256; level++) {
    cumulative += histogram[level];
    if (cumulative > target) return level;
  }
  return 255;
}
export function enhanceForOcr(data: Uint8ClampedArray): void {
  const pixelCount = data.length / 4;
  const gray = new Uint8ClampedArray(pixelCount);
  const histogram = new Uint32Array(256);
  for (let i = 0; i < pixelCount; i++) {
    const o = i * 4;
    const level = Math.round(0.299 * data[o] + 0.587 * data[o + 1] + 0.114 * data[o + 2]);
    gray[i] = level;
    histogram[gray[i]]++;
  }
  const low = percentileLevel(histogram, pixelCount, 0.02);
  const high = percentileLevel(histogram, pixelCount, 0.98);
  const range = high - low;
  const stretch = range >= 16;
  const scale = 255 / range;
  for (let i = 0; i < pixelCount; i++) {
    const o = i * 4;
    const value = stretch ? Math.round((gray[i] - low) * scale) : gray[i];
    data[o] = data[o + 1] = data[o + 2] = value < 0 ? 0 : value > 255 ? 255 : value;
    data[o + 3] = 255;
  }
}
// Tesseract reads best when letters are roughly 20–30 px tall, so small photos
// and screenshots are enlarged (at most 2×) before OCR.
const MIN_OCR_LONG_SIDE = 1600;
export function ocrScale(width: number, height: number): number {
  const longest = Math.max(width, height);
  return longest > 0 && longest < MIN_OCR_LONG_SIDE ? Math.min(2, MIN_OCR_LONG_SIDE / longest) : 1;
}
// Separable running max/min/mean over a (2r+1)² window, edges clamped to the
// image. Only used on the downsampled background grid, so O(n·r) is cheap.
function windowPass(
  src: Float32Array,
  width: number,
  height: number,
  radius: number,
  op: 'max' | 'min' | 'mean',
): Float32Array {
  const run = (read: (i: number) => number, write: (i: number, v: number) => void, n: number) => {
    for (let i = 0; i < n; i++) {
      const lo = Math.max(0, i - radius),
        hi = Math.min(n - 1, i + radius);
      let acc = op === 'min' ? Infinity : op === 'max' ? -Infinity : 0;
      for (let j = lo; j <= hi; j++) {
        const v = read(j);
        acc = op === 'min' ? Math.min(acc, v) : op === 'max' ? Math.max(acc, v) : acc + v;
      }
      write(i, op === 'mean' ? acc / (hi - lo + 1) : acc);
    }
  };
  const rows = new Float32Array(src.length),
    out = new Float32Array(src.length);
  for (let y = 0; y < height; y++)
    run(
      (j) => src[y * width + j],
      (i, v) => (rows[y * width + i] = v),
      width,
    );
  for (let x = 0; x < width; x++)
    run(
      (j) => rows[j * width + x],
      (i, v) => (out[i * width + x] = v),
      height,
    );
  return out;
}
/** Paper brightness on a coarse grid (`factor` px per cell). */
interface PaperLight {
  grid: Float32Array;
  gw: number;
  gh: number;
  factor: number;
}
/**
 * Estimates paper brightness as a morphological closing of the gray image
 * (max then min, so gaps between letters fill but shadow edges stay put)
 * with a window wider than any glyph, then smoothed. It runs on a grid
 * ~550 px long (each cell = its brightest pixel) to keep the wide window cheap.
 */
function estimatePaper(gray: Float32Array, width: number, height: number): PaperLight {
  const longest = Math.max(width, height);
  const factor = Math.max(1, Math.floor(longest / 550));
  const gw = Math.ceil(width / factor),
    gh = Math.ceil(height / factor);
  const cells = new Float32Array(gw * gh);
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      const g = ((y / factor) | 0) * gw + ((x / factor) | 0);
      cells[g] = Math.max(cells[g], gray[y * width + x]);
    }
  const radius = Math.max(1, Math.round(longest / 60 / factor));
  const closed = windowPass(windowPass(cells, gw, gh, radius, 'max'), gw, gh, radius, 'min');
  return { grid: windowPass(closed, gw, gh, radius, 'mean'), gw, gh, factor };
}
/**
 * How uneven the lighting is: 0 for evenly lit paper (scans, PDFs,
 * screenshots), approaching 1 when part of the receipt is in deep shadow.
 * Compares the 5th and 95th percentile of paper brightness.
 */
export function lightingUnevenness(paper: Float32Array): number {
  const sorted = Float32Array.from(paper).sort();
  const bright = sorted[Math.floor(0.95 * (sorted.length - 1))];
  const dim = sorted[Math.floor(0.05 * (sorted.length - 1))];
  return bright > 0 ? 1 - dim / bright : 0;
}
/** Above this, a shadow is darkening part of the paper: flatten before OCR. */
export const UNEVEN_LIGHTING = 0.25;
/**
 * Removes uneven lighting (a hand or phone shadow across a photographed
 * receipt) and thickens strokes so faint dot-matrix digits join up. Dividing
 * by the estimated paper brightness makes shaded and lit paper equally white.
 */
function flatten(
  gray: Float32Array,
  width: number,
  height: number,
  { grid, gw, gh, factor }: PaperLight,
): Float32Array {
  for (let y = 0; y < height; y++) {
    const fy = Math.min(gh - 1, Math.max(0, (y + 0.5) / factor - 0.5));
    const y0 = fy | 0,
      y1 = Math.min(gh - 1, y0 + 1),
      ty = fy - y0;
    for (let x = 0; x < width; x++) {
      const fx = Math.min(gw - 1, Math.max(0, (x + 0.5) / factor - 0.5));
      const x0 = fx | 0,
        x1 = Math.min(gw - 1, x0 + 1),
        tx = fx - x0;
      const top = grid[y0 * gw + x0] * (1 - tx) + grid[y0 * gw + x1] * tx;
      const bottom = grid[y1 * gw + x0] * (1 - tx) + grid[y1 * gw + x1] * tx;
      const paper = top * (1 - ty) + bottom * ty;
      const i = y * width + x;
      gray[i] = Math.min(255, (gray[i] * 255) / Math.max(8, paper));
    }
  }
  // No percentile stretch: paper is already ~255 everywhere, and on a mostly
  // blank receipt the low percentile is paper noise, so stretching would
  // amplify that noise several times over. A ~2 px min filter (at the
  // 2200 px OCR render size) joins dot-matrix dots into solid strokes.
  return windowPass(
    gray,
    width,
    height,
    Math.max(1, Math.round(Math.max(width, height) / 1100)),
    'min',
  );
}
/**
 * Prepares receipt pixels for OCR in place. Evenly lit images get a
 * grayscale contrast stretch (`enhanceForOcr`); photos with a shadow across
 * the paper are flattened first. Returns which treatment was used.
 */
export function prepareForOcr(
  data: Uint8ClampedArray,
  width: number,
  height: number,
): 'standard' | 'flattened' {
  const pixelCount = width * height;
  const gray = new Float32Array(pixelCount);
  for (let i = 0; i < pixelCount; i++) {
    const o = i * 4;
    gray[i] = 0.299 * data[o] + 0.587 * data[o + 1] + 0.114 * data[o + 2];
  }
  const paper = estimatePaper(gray, width, height);
  if (lightingUnevenness(paper.grid) <= UNEVEN_LIGHTING) {
    enhanceForOcr(data);
    return 'standard';
  }
  const flat = flatten(gray, width, height, paper);
  for (let i = 0; i < pixelCount; i++) {
    const o = i * 4;
    const v = Math.round(flat[i]);
    data[o] = data[o + 1] = data[o + 2] = v < 0 ? 0 : v > 255 ? 255 : v;
    data[o + 3] = 255;
  }
  return 'flattened';
}
/** Flattened brightness below this fraction of the paper counts as ink. */
export const INK_THRESHOLD = 0.72;
/**
 * Turns receipt pixels pure black and white in place, for an OCR retry when
 * the gray image read badly. Faded thermal paper is gray rather than white,
 * and Tesseract can mistake a large gray area for a picture and skip it
 * entirely. A light blur first keeps paper grain from turning into specks.
 */
export function binarizeForOcr(data: Uint8ClampedArray, width: number, height: number): void {
  const pixelCount = width * height;
  const gray = new Float32Array(pixelCount);
  for (let i = 0; i < pixelCount; i++) {
    const o = i * 4;
    gray[i] = 0.299 * data[o] + 0.587 * data[o + 1] + 0.114 * data[o + 2];
  }
  const smooth = windowPass(gray, width, height, 1, 'mean');
  const flat = flatten(smooth, width, height, estimatePaper(smooth, width, height));
  for (let i = 0; i < pixelCount; i++) {
    const o = i * 4;
    data[o] = data[o + 1] = data[o + 2] = flat[i] < 255 * INK_THRESHOLD ? 0 : 255;
    data[o + 3] = 255;
  }
}
/** Returns a black-and-white copy of an OCR-prepared canvas. */
export function binarizedCanvas(source: HTMLCanvasElement): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  canvas.width = source.width;
  canvas.height = source.height;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) throw new Error('Unable to preprocess this receipt image.');
  context.drawImage(source, 0, 0);
  const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
  binarizeForOcr(imageData.data, canvas.width, canvas.height);
  context.putImageData(imageData, 0, 0);
  return canvas;
}
/** Returns a new canvas prepared for OCR; `source` is left untouched. */
export function preprocessCanvas(source: HTMLCanvasElement): HTMLCanvasElement {
  const scale = ocrScale(source.width, source.height);
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(source.width * scale);
  canvas.height = Math.round(source.height * scale);
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) throw new Error('Unable to preprocess this receipt image.');
  context.imageSmoothingQuality = 'high';
  context.drawImage(source, 0, 0, canvas.width, canvas.height);
  const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
  prepareForOcr(imageData.data, canvas.width, canvas.height);
  context.putImageData(imageData, 0, 0);
  return canvas;
}
export async function* receiptPages(
  content: ReceiptContent,
  limit = 2200,
): AsyncGenerator<HTMLCanvasElement> {
  if (content.mimeType !== 'application/pdf') {
    yield await imageCanvas(content, limit);
    return;
  }
  const pdf = await loadPdf(content);
  try {
    if (pdf.numPages > 30)
      throw new Error(
        'PDF receipts support up to 30 pages. Split this PDF or enter the expense manually.',
      );
    for (let p = 1; p <= pdf.numPages; p++) yield await renderPdfPage(pdf, p, limit);
  } finally {
    await pdf.loadingTask.destroy();
  }
}
export async function thumbnail(content: ReceiptContent): Promise<string> {
  for await (const canvas of receiptPages(content, 400)) {
    const data = canvas.toDataURL('image/webp', 0.8).split(',')[1];
    canvas.width = canvas.height = 0;
    return data;
  }
  throw new Error('The receipt has no pages.');
}
