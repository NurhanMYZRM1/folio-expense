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
  const bitmap = await createImageBitmap(
    new Blob([bytes as BlobPart], { type: content.receipt.mimeType }),
  );
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
export function preprocessCanvas(source: HTMLCanvasElement): HTMLCanvasElement {
  const scale = ocrScale(source.width, source.height);
  let canvas = source;
  if (scale > 1) {
    canvas = document.createElement('canvas');
    canvas.width = Math.round(source.width * scale);
    canvas.height = Math.round(source.height * scale);
    const enlarge = canvas.getContext('2d');
    if (!enlarge) throw new Error('Unable to preprocess this receipt image.');
    enlarge.imageSmoothingQuality = 'high';
    enlarge.drawImage(source, 0, 0, canvas.width, canvas.height);
  }
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Unable to preprocess this receipt image.');
  const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
  enhanceForOcr(imageData.data);
  context.putImageData(imageData, 0, 0);
  return canvas;
}
export async function* receiptPages(
  content: ReceiptContent,
  limit = 2200,
): AsyncGenerator<HTMLCanvasElement> {
  if (content.receipt.mimeType !== 'application/pdf') {
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
