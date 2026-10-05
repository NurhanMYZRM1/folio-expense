// ../src/lib/receiptRendering.ts for the receipt viewer: PDF pages are
// rendered by PDFKit on the native side instead of pdf.js.
import type { ReceiptContent } from "@folio/bindings/generated";
import { bridge } from "../bridge";

export interface PagedPdf {
  numPages: number;
  pages: string[];
  loadingTask: { destroy(): Promise<void> };
}

export async function loadPdf(content: ReceiptContent): Promise<PagedPdf> {
  const pages = JSON.parse(await bridge().renderPdf(content.receipt.relativePath)) as string[];
  return { numPages: pages.length, pages, loadingTask: { destroy: async () => undefined } };
}

export async function renderPdfPage(pdf: PagedPdf, pageNumber: number): Promise<HTMLCanvasElement> {
  const img = new Image();
  img.src = pdf.pages[pageNumber - 1];
  await img.decode();
  const canvas = document.createElement("canvas");
  canvas.width = img.naturalWidth;
  canvas.height = img.naturalHeight;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Unable to render the receipt.");
  context.drawImage(img, 0, 0);
  return canvas;
}
