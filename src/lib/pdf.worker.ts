import { generateClaimPdf } from './pdf';
import type { ExportSnapshot, ReceiptContent } from '../bindings/generated';
self.onmessage = async (
  event: MessageEvent<{ snapshot: ExportSnapshot; receipts: ReceiptContent[]; font: Uint8Array }>,
) => {
  try {
    const bytes = await generateClaimPdf(event.data.snapshot, event.data.receipts, event.data.font);
    self.postMessage({ bytes }, { transfer: [bytes.buffer] });
  } catch (error) {
    self.postMessage({ error: error instanceof Error ? error.message : 'PDF generation failed.' });
  }
};
