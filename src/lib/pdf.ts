import { PDFDocument, rgb, PDFName, PDFString, type PDFFont, type PDFPage } from 'pdf-lib';
import fontkit from '@pdf-lib/fontkit';
import type { ExportSnapshot, ReceiptContent } from '../bindings/generated';
import { formatMoney } from './money';
export type ReceiptReference = { label: string; uri: string };
export function receiptReference(id: string, filename: string, base?: string): ReceiptReference {
  return {
    label: `${filename} · ${id}`,
    uri: base
      ? `${base.replace(/\/$/, '')}/${encodeURIComponent(id)}`
      : `expenseapp://receipt/${encodeURIComponent(id)}`,
  };
}
const ink = rgb(0.12, 0.17, 0.2),
  gray = rgb(0.42, 0.47, 0.49),
  green = rgb(0.15, 0.39, 0.32),
  line = rgb(0.86, 0.89, 0.9);
export async function generateClaimPdf(
  snapshot: ExportSnapshot,
  receipts: ReceiptContent[],
  fontBytes: Uint8Array,
): Promise<Uint8Array> {
  const doc = await PDFDocument.create();
  doc.registerFontkit(fontkit);
  const font = await doc.embedFont(fontBytes, { subset: true });
  doc.setTitle(`${snapshot.claim.claimNumber} — ${snapshot.claim.title}`);
  doc.setAuthor(snapshot.employee || 'Folio');
  doc.setCreator('Folio · Local expense workspace');
  doc.setCreationDate(new Date(snapshot.generatedAt));
  const width = 595.28,
    height = 841.89,
    margin = 42;
  let page!: PDFPage,
    y = height - margin;
  const text = (value: string, x: number, top: number, size = 10, color = ink) =>
    page.drawText(value, { x, y: top, size, font, color });
  function shorten(value: string, maxWidth: number, size: number, face: PDFFont = font) {
    let s = value.replace(/[\r\n\t]/g, ' ');
    while (s.length > 0 && face.widthOfTextAtSize(s, size) > maxWidth) s = s.slice(0, -1);
    return s.length < value.length ? s.slice(0, -1) + '…' : s;
  }
  function wrap(value: string, maxWidth: number, size: number): string[] {
    const result: string[] = [];
    for (const paragraph of value.split('\n')) {
      let current = '';
      for (const word of paragraph.split(/\s+/)) {
        const candidate = current ? `${current} ${word}` : word;
        if (font.widthOfTextAtSize(candidate, size) > maxWidth && current) {
          result.push(current);
          current = word;
        } else current = candidate;
      }
      if (current) result.push(shorten(current, maxWidth, size));
    }
    return result;
  }
  function newPage(title: string) {
    page = doc.addPage([width, height]);
    y = height - margin;
    text('folio', margin, y, 20, green);
    text(title, width - margin - font.widthOfTextAtSize(title, 9), y + 2, 9, gray);
    page.drawLine({
      start: { x: margin, y: y - 16 },
      end: { x: width - margin, y: y - 16 },
      thickness: 0.7,
      color: line,
    });
    y -= 43;
  }
  function tableHead() {
    page.drawRectangle({
      x: margin,
      y: y - 24,
      width: width - margin * 2,
      height: 27,
      color: rgb(0.95, 0.96, 0.96),
    });
    [
      ['DATE', margin + 8],
      ['MERCHANT / CATEGORY', margin + 84],
      ['AMOUNT', margin + 326],
      ['RECEIPT', margin + 434],
    ].forEach(([s, x]) => text(s as string, x as number, y - 14, 8, gray));
    y -= 39;
  }
  newPage('EXPENSE CLAIM');
  text(
    shorten(snapshot.company || 'Company / department: __________________', width - margin * 2, 10),
    margin,
    y,
    10,
  );
  y -= 20;
  text(
    shorten(`Employee: ${snapshot.employee || '__________________'}`, width - margin * 2, 10),
    margin,
    y,
    10,
  );
  y -= 34;
  for (const s of wrap(snapshot.claim.title, width - margin * 2, 20)) {
    text(s, margin, y, 20);
    y -= 26;
  }
  text(
    `${snapshot.claim.claimNumber}   ·   ${snapshot.claim.status.toUpperCase()}   ·   ${snapshot.generatedAt.slice(0, 10)}`,
    margin,
    y,
    9,
    gray,
  );
  y -= 23;
  for (const s of wrap(snapshot.claim.description, width - margin * 2, 9)) {
    if (y < 85) newPage('CLAIM DETAILS');
    text(s, margin, y, 9, gray);
    y -= 14;
  }
  if (y < 180) newPage('CLAIM SUMMARY');
  page.drawRectangle({
    x: margin,
    y: y - 66,
    width: width - margin * 2,
    height: 66,
    color: rgb(0.94, 0.97, 0.95),
  });
  text('TOTAL CLAIM AMOUNT', margin + 16, y - 20, 8, green);
  text(
    formatMoney(snapshot.claim.totalAmountMinor, snapshot.claim.currency),
    margin + 16,
    y - 47,
    22,
    green,
  );
  text(`${snapshot.expenses.length} EXPENSES`, width - margin - 112, y - 23, 9, green);
  text(`CURRENCY  ${snapshot.claim.currency}`, width - margin - 112, y - 44, 9, gray);
  y -= 94;
  tableHead();
  snapshot.expenses.forEach((e, index) => {
    if (y < 90) {
      newPage('EXPENSE CLAIM · CONTINUED');
      tableHead();
    }
    text(e.occurredAt ?? '—', margin + 8, y, 8);
    const merchant = wrap(e.merchantName || 'Expense', 230, 9).slice(0, 2);
    merchant.forEach((s, i) => text(s, margin + 84, y - i * 12, 9));
    text(e.category, margin + 84, y - merchant.length * 12 - 2, 8, gray);
    const amount = formatMoney(e.totalAmountMinor, e.currency ?? snapshot.claim.currency);
    text(amount, margin + 422 - font.widthOfTextAtSize(amount, 9), y, 9);
    text(
      e.receiptId ? `R${String(index + 1).padStart(3, '0')}` : 'Manual',
      margin + 438,
      y,
      8,
      e.receiptId ? green : gray,
    );
    if (e.receiptId) {
      const ref = receiptReference(e.receiptId, e.receiptFilename || 'Receipt');
      const annotation = doc.context.register(
        doc.context.obj({
          Type: 'Annot',
          Subtype: 'Link',
          Rect: [margin + 434, y - 4, width - margin, y + 12],
          Border: [0, 0, 0],
          A: { Type: 'Action', S: 'URI', URI: PDFString.of(ref.uri) },
        }),
      );
      page.node.addAnnot(annotation);
    }
    y -= Math.max(42, merchant.length * 12 + 25);
    page.drawLine({
      start: { x: margin, y: y + 12 },
      end: { x: width - margin, y: y + 12 },
      thickness: 0.4,
      color: line,
    });
  });
  newPage('RECEIPT REFERENCES');
  text('Receipt register', margin, y, 18);
  y -= 27;
  text(
    'Links open in Folio on the source computer. IDs and filenames remain portable.',
    margin,
    y,
    8,
    gray,
  );
  y -= 27;
  snapshot.expenses.forEach((e, index) => {
    if (y < 110) newPage('RECEIPT REFERENCES · CONTINUED');
    text(
      `R${String(index + 1).padStart(3, '0')}   ${shorten(e.merchantName || 'Expense', 390, 10)}`,
      margin,
      y,
      10,
    );
    y -= 16;
    if (e.receiptId) {
      for (const s of wrap(e.receiptFilename || 'Receipt', 500, 8)) {
        text(s, margin + 30, y, 8, gray);
        y -= 12;
      }
      text(`Receipt ID: ${e.receiptId}`, margin + 30, y, 8, gray);
      y -= 13;
    } else {
      text('No receipt attached — manually entered expense.', margin + 30, y, 8, gray);
      y -= 13;
    }
    y -= 16;
  });
  if (snapshot.includeReceipts) {
    for (let index = 0; index < snapshot.expenses.length; index++) {
      const e = snapshot.expenses[index];
      if (!e.receiptId) continue;
      const receipt = receipts.find((r) => r.receipt.id === e.receiptId);
      if (!receipt)
        throw new Error(
          `Receipt is missing for ${e.merchantName}. Restore it or export without the appendix.`,
        );
      const bytes = Uint8Array.from(atob(receipt.base64), (c) => c.charCodeAt(0));
      const label = `R${String(index + 1).padStart(3, '0')} · ${e.merchantName || 'Expense'}`;
      if (receipt.receipt.mimeType === 'application/pdf') {
        const original = await PDFDocument.load(bytes, { updateMetadata: false });
        if (original.getPageCount() > 30)
          throw new Error('A PDF receipt exceeds the 30 page appendix limit.');
        // Embed original page content without copying interactive actions or annotations.
        for (let p = 0; p < original.getPageCount(); p++) {
          newPage('RECEIPT APPENDIX');
          text(shorten(`${label} · page ${p + 1}`, 500, 11), margin, y, 11);
          y -= 22;
          if (!original.getPage(p).node.Contents()) {
            text('Blank receipt page', margin, y, 9, gray);
            continue;
          }
          const [embedded] = await doc.embedPdf(original, [p]);
          const scale = Math.min((width - margin * 2) / embedded.width, (y - 60) / embedded.height);
          page.drawPage(embedded, {
            x: (width - embedded.width * scale) / 2,
            y: y - embedded.height * scale,
            width: embedded.width * scale,
            height: embedded.height * scale,
          });
        }
      } else {
        const image =
          receipt.receipt.mimeType === 'image/png'
            ? await doc.embedPng(bytes)
            : await doc.embedJpg(bytes);
        newPage('RECEIPT APPENDIX');
        text(shorten(label, 500, 11), margin, y, 11);
        y -= 22;
        const scale = Math.min((width - margin * 2) / image.width, (y - 60) / image.height);
        page.drawImage(image, {
          x: (width - image.width * scale) / 2,
          y: y - image.height * scale,
          width: image.width * scale,
          height: image.height * scale,
        });
      }
    }
  }
  doc.getPages().forEach((p, i) => {
    p.drawLine({
      start: { x: margin, y: 43 },
      end: { x: width - margin, y: 43 },
      color: line,
      thickness: 0.5,
    });
    p.drawText(`${snapshot.claim.claimNumber} · Generated locally by Folio`, {
      x: margin,
      y: 29,
      size: 7,
      font,
      color: gray,
    });
    p.drawText(`${i + 1} / ${doc.getPageCount()}`, {
      x: width - margin - 35,
      y: 29,
      size: 8,
      font,
      color: gray,
    });
    p.node.set(PDFName.of('Tabs'), PDFName.of('S'));
  });
  return doc.save();
}
