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
