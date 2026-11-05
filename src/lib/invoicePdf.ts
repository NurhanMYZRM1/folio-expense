import { PDFDocument, rgb, type PDFPage } from 'pdf-lib';
import fontkit from '@pdf-lib/fontkit';
import type { Invoice } from '../bindings/generated';
import { formatMoney } from './money';
import { api } from './ipc';
import { encodeBase64, decodeBase64 } from './encoding';

/** A PDF of the persisted invoice snapshot; totals have already been calculated
 * in Rust. Font bytes may be supplied by the native asset bridge. */
export async function generateInvoicePdf(
  invoice: Invoice,
  fontBytes: Uint8Array,
): Promise<Uint8Array> {
  const doc = await PDFDocument.create();
  doc.registerFontkit(fontkit);
  const font = await doc.embedFont(fontBytes, { subset: true });
  doc.setTitle(invoice.number ?? 'Draft invoice');
  doc.setCreator('Folio');
  let page: PDFPage;
  let y = 0;
  const fresh = () => {
    page = doc.addPage([595.28, 841.89]);
    y = 793;
  };
  fresh();
  const text = (value: string, size = 11) => {
    // Wrap by characters too, so unbroken names/URLs stay inside the page.
    for (const paragraph of value.split(/\r?\n/)) {
      let line = '';
      for (const ch of paragraph) {
        if (font.widthOfTextAtSize(line + ch, size) > 499) {
          if (y < 50) fresh();
          page.drawText(line, { x: 48, y, size, font, color: rgb(0.12, 0.17, 0.2) });
          y -= size + 6;
          line = '';
        }
        // Remove PDF control characters, preserving Unicode text.
        if (ch.charCodeAt(0) >= 32) line += ch;
      }
      if (y < 50) fresh();
      page.drawText(line, { x: 48, y, size, font, color: rgb(0.12, 0.17, 0.2) });
      y -= size + 6;
    }
  };
  text(`INVOICE ${invoice.number ?? '(draft)'}`, 20);
  text(`Status: ${invoice.status} · Issued ${invoice.issuedOn} · Due ${invoice.dueOn}`);
  text(invoice.supplier?.legalName ?? 'Business profile incomplete');
  if (invoice.supplier) {
    text(`Registration: ${invoice.supplier.registrationNumber} · TIN: ${invoice.supplier.tin}`);
    text(`SST registration: ${invoice.supplier.sstRegistrationNumber || 'Not provided'}`);
    text(invoice.supplier.address);
  }
  y -= 12;
  text(`Bill to: ${invoice.customer.name}`, 13);
  text(invoice.customer.address);
  text(`Registration: ${invoice.customer.registrationNumber} · TIN: ${invoice.customer.tin}`);
  y -= 12;
  for (const [index, line] of invoice.lines.entries()) {
    text(`${index + 1}. ${line.description}`, 12);
    text(
      `${line.quantityMilli / 1000} × ${formatMoney(line.unitPriceMinor, invoice.currency)} · Discount ${formatMoney(line.discountMinor, invoice.currency)}`,
    );
    text(
      `Tax ${line.taxBasisPoints / 100}%: ${formatMoney(line.taxMinor, invoice.currency)} · Line total ${formatMoney(line.totalMinor, invoice.currency)}`,
    );
    y -= 8;
  }
  text(`Subtotal: ${formatMoney(invoice.subtotalMinor, invoice.currency)}`, 13);
  text(`Tax: ${formatMoney(invoice.taxMinor, invoice.currency)}`, 13);
  text(`Total: ${formatMoney(invoice.totalMinor, invoice.currency)}`, 16);
  text(
    `Paid: ${formatMoney(invoice.paidMinor, invoice.currency)} · Balance: ${formatMoney(invoice.totalMinor - invoice.paidMinor, invoice.currency)}`,
  );
  if (invoice.note) text(invoice.note);
  text('Prepared in Folio. This document has not been submitted to MyInvois.', 9);
  return doc.save();
}

export async function exportInvoicePdf(invoice: Invoice) {
  const native = (
    window as unknown as {
      __folio?: { invoke: (command: string, args: string) => Promise<string> };
    }
  ).__folio;
  let font: Uint8Array;
  if (native) {
    const result = JSON.parse(await native.invoke('read_invoice_font', '{}')) as {
      result?: string;
      error?: { message: string };
    };
    if (!result.result)
      throw new Error(result.error?.message ?? 'The invoice font is unavailable.');
    font = decodeBase64(result.result);
  } else {
    const response = await fetch('/fonts/noto-sans-latin-400-normal.woff');
    if (!response.ok)
      throw new Error('The invoice font is unavailable. Run npm run assets and try again.');
    font = new Uint8Array(await response.arrayBuffer());
  }
  const bytes = await generateInvoicePdf(invoice, font);
  const result = await api.saveInvoicePdf(
    invoice.workspaceId,
    invoice.id,
    invoice.version,
    encodeBase64(bytes),
  );
  await api.openInvoicePdf(result.id);
}
