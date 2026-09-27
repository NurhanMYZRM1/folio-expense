import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { inflateSync } from 'node:zlib';
import {
  PDFDocument,
  PDFArray,
  PDFDict,
  PDFName,
  PDFRawStream,
  PDFRef,
  type PDFPage,
} from 'pdf-lib';
import { generateClaimPdf, receiptReference } from './pdf';
import type { ExportSnapshot, Expense, ReceiptContent } from '../bindings/generated';

// --- Test-only PDF introspection helpers -----------------------------------
// pdf-lib embeds our subset font as a CID font, so drawn text shows up in the
// content stream as glyph-index bytes, not literal ASCII. To assert on the
// actual rendered text we decode each Tj/TJ run using the font's embedded
// ToUnicode CMap (the same one PDF viewers use for copy/paste).

function decodeStream(raw: Uint8Array): Buffer {
  const buf = Buffer.from(raw);
  try {
    return inflateSync(buf);
  } catch {
    return buf;
  }
}

function parseToUnicode(cmapText: string): Map<string, string> {
  const map = new Map<string, string>();
  const hexToChars = (hex: string) => {
    let out = '';
    for (let i = 0; i < hex.length; i += 4)
      out += String.fromCharCode(parseInt(hex.slice(i, i + 4), 16));
    return out;
  };
  for (const block of cmapText.match(/beginbfchar([\s\S]*?)endbfchar/g) ?? []) {
    const re = /<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(block))) map.set(m[1].toUpperCase().padStart(4, '0'), hexToChars(m[2]));
  }
  for (const block of cmapText.match(/beginbfrange([\s\S]*?)endbfrange/g) ?? []) {
    const re = /<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(block))) {
      const start = parseInt(m[1], 16),
        end = parseInt(m[2], 16),
        dstStart = parseInt(m[3], 16);
      for (let cid = start; cid <= end; cid++)
        map.set(
          cid.toString(16).toUpperCase().padStart(4, '0'),
          String.fromCharCode(dstStart + (cid - start)),
        );
    }
  }
  return map;
}

function pageText(pdf: PDFDocument, pageIndex: number): string {
  const page = pdf.getPage(pageIndex);
  const contentsObj = pdf.context.lookup(page.node.get(PDFName.of('Contents')));
  const streamRefs: unknown[] =
    contentsObj instanceof PDFArray ? contentsObj.asArray() : [contentsObj];
  let raw = '';
  for (const ref of streamRefs) {
    const stream = pdf.context.lookup(ref as PDFRef) as PDFRawStream;
    raw += decodeStream(stream.contents).toString('latin1');
  }
  const resources = pdf.context.lookup(page.node.get(PDFName.of('Resources'))) as PDFDict;
  const fontDict = pdf.context.lookup(resources?.get(PDFName.of('Font'))) as PDFDict | undefined;
  const cmaps = new Map<string, Map<string, string>>();
  if (fontDict) {
    for (const [key, ref] of fontDict.entries()) {
      const fontObj = pdf.context.lookup(ref) as PDFDict;
      const tuRef = fontObj.get(PDFName.of('ToUnicode'));
      if (!tuRef) continue;
      const tuStream = pdf.context.lookup(tuRef) as PDFRawStream;
      cmaps.set(key.asString(), parseToUnicode(decodeStream(tuStream.contents).toString('latin1')));
    }
  }
  let result = '';
  let currentMap: Map<string, string> | undefined;
  const tokenRe = /\/(\S+)\s+[\d.]+\s+Tf|<([0-9A-Fa-f]*)>\s*Tj/g;
  let m: RegExpExecArray | null;
  while ((m = tokenRe.exec(raw))) {
    if (m[1] !== undefined) {
      currentMap = cmaps.get(`/${m[1]}`);
    } else if (m[2] !== undefined) {
      const hex = m[2];
      for (let i = 0; i < hex.length; i += 4)
        result += currentMap?.get(hex.slice(i, i + 4).toUpperCase()) ?? '';
      result += ' ';
    }
  }
  return result;
}

function linkAnnotations(page: PDFPage): PDFDict[] {
  const annots = page.node.Annots();
  if (!annots) return [];
  const dicts: PDFDict[] = [];
  for (let i = 0; i < annots.size(); i++) dicts.push(annots.lookup(i, PDFDict));
  return dicts;
}

function destPageRef(annot: PDFDict): PDFRef | undefined {
  const dest = annot.lookup(PDFName.of('Dest'), PDFArray);
  const first = dest.get(0);
  return first instanceof PDFRef ? first : undefined;
}

function uriOf(annot: PDFDict): string | undefined {
  if (!annot.has(PDFName.of('A'))) return undefined;
  const action = annot.lookup(PDFName.of('A'), PDFDict);
  const uri = action.get(PDFName.of('URI'));
  // @ts-expect-error PDFString has asString()
  return uri?.asString?.();
}

const fixture = (): ExportSnapshot => ({
  claim: {
    id: 'claim-1',
    claimNumber: 'CL-20260927-TEST',
    title: 'September client visits',
    description: 'Client meetings and travel',
    currency: 'MYR',
    totalAmountMinor: 8450,
    expenseCount: 1,
    status: 'draft',
    createdAt: '2026-09-27T12:00:00Z',
    updatedAt: '2026-09-27T12:00:00Z',
    version: 1,
    syncState: 'local_only',
  },
  expenses: [
    {
      id: 'e1',
      receiptId: 'r1',
      occurredAt: '2026-09-27',
      merchantName: 'Kopi House',
      totalAmountMinor: 8450,
      taxAmountMinor: 478,
      currency: 'MYR',
      category: 'Meals',
      description: '',
      status: 'ready',
      fieldMeta: {},
      extractionConfidence: 0.9,
      createdAt: '',
      updatedAt: '',
      version: 1,
      syncState: 'local_only',
      receiptFilename: 'receipt.pdf',
      claimId: 'claim-1',
    },
  ],
  receipts: [
    {
      id: 'r1',
      sha256: 'hash',
      originalFilename: 'receipt.pdf',
      mimeType: 'application/pdf',
      relativePath: 'receipts/r1/original.pdf',
      sizeBytes: 100,
      createdAt: '',
    },
  ],
  company: 'Acme Corporation',
  employee: 'Sample Employee',
  includeReceipts: false,
  generatedAt: '2026-09-27T12:00:00Z',
});
const loadFont = () => readFile('public/fonts/noto-sans-latin-400-normal.woff');
async function makeImageReceipt(): Promise<ReceiptContent> {
  // A 1x1 PNG, base64-encoded.
  const png =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=';
  return {
    receipt: {
      id: 'r1',
      sha256: 'hash',
      originalFilename: 'receipt.png',
      mimeType: 'image/png',
      relativePath: 'receipts/r1/original.png',
      sizeBytes: 100,
      createdAt: '',
    },
    base64: png,
  };
}
async function makePdfReceipt(pageCount = 2): Promise<ReceiptContent> {
  const original = await PDFDocument.create();
  for (let i = 0; i < pageCount; i++) original.addPage([300, 400]);
  return {
    receipt: {
      id: 'r1',
      sha256: 'hash',
      originalFilename: 'receipt.pdf',
      mimeType: 'application/pdf',
      relativePath: 'receipts/r1/original.pdf',
      sizeBytes: 100,
      createdAt: '',
    },
    base64: Buffer.from(await original.save()).toString('base64'),
  };
}

describe('claim PDF', () => {
  it('embeds the local font, exact totals, and a deep link', async () => {
    const font = await loadFont();
    const data = await generateClaimPdf(fixture(), [], font);
    const pdf = await PDFDocument.load(data);
    expect(pdf.getPageCount()).toBe(2);
    const annotations = pdf.getPage(0).node.lookup(PDFName.of('Annots'), PDFArray);
    expect(annotations.size()).toBe(1);
    expect(pdf.getTitle()).toContain('CL-20260927-TEST');
  });
  it('supports portable cloud references without changing report layout', () => {
    expect(receiptReference('r1', 'receipt.png').uri).toBe('expenseapp://receipt/r1');
    expect(receiptReference('r1', 'receipt.png', 'https://company.example/receipt/').uri).toBe(
      'https://company.example/receipt/r1',
    );
  });
  it('paginates 80 expenses and includes every page of a PDF receipt', async () => {
    const s = fixture();
    const base = s.expenses[0];
    s.expenses = Array.from({ length: 80 }, (_, i): Expense => ({
      ...base,
      id: `e${i}`,
      receiptId: i ? null : 'r1',
    }));
    s.claim.expenseCount = 80;
    s.claim.totalAmountMinor = 8450 * 80;
    s.includeReceipts = true;
    const receipt = await makePdfReceipt(2);
    const data = await generateClaimPdf(s, [receipt], await loadFont());
    const pdf = await PDFDocument.load(data);
    expect(pdf.getPageCount()).toBeGreaterThan(9);
  });
  it('fails explicitly instead of silently omitting missing appendix receipts', async () => {
    const s = fixture();
    s.includeReceipts = true;
    await expect(generateClaimPdf(s, [], await loadFont())).rejects.toThrow('Receipt is missing');
  });
  it('renders five separate summary-table column headers', async () => {
    const data = await generateClaimPdf(fixture(), [], await loadFont());
    const pdf = await PDFDocument.load(data);
    const heading = pageText(pdf, 0);
    for (const header of ['DATE', 'MERCHANT', 'CATEGORY', 'AMOUNT', 'RECEIPT'])
      expect(heading).toContain(header);
  });
  it('links the summary RECEIPT cell to the register page when receipts are not included', async () => {
    const s = fixture();
    s.includeReceipts = false;
    const data = await generateClaimPdf(s, [], await loadFont());
    const pdf = await PDFDocument.load(data);
    expect(pdf.getPageCount()).toBe(2);
    const registerPage = pdf.getPage(1);
    const destLinks = linkAnnotations(pdf.getPage(0)).filter((a) => a.has(PDFName.of('Dest')));
    expect(destLinks).toHaveLength(1);
    expect(destPageRef(destLinks[0])?.toString()).toBe(registerPage.ref.toString());
  });
  it('links the summary RECEIPT cell to the first appendix page for an image receipt', async () => {
    const s = fixture();
    s.receipts[0].mimeType = 'image/png';
    s.expenses[0].receiptFilename = 'receipt.png';
    s.includeReceipts = true;
    const data = await generateClaimPdf(s, [await makeImageReceipt()], await loadFont());
    const pdf = await PDFDocument.load(data);
    expect(pdf.getPageCount()).toBe(3); // summary, register, one appendix page
    const appendixPage = pdf.getPage(2);
    expect(pageText(pdf, 2)).toContain('RECEIPT APPENDIX');
    const destLinks = linkAnnotations(pdf.getPage(0)).filter((a) => a.has(PDFName.of('Dest')));
    expect(destLinks).toHaveLength(1);
    expect(destPageRef(destLinks[0])?.toString()).toBe(appendixPage.ref.toString());
  });
  it('links the summary RECEIPT cell to the first page of a multi-page PDF receipt appendix', async () => {
    const s = fixture();
    s.includeReceipts = true;
    const data = await generateClaimPdf(s, [await makePdfReceipt(2)], await loadFont());
    const pdf = await PDFDocument.load(data);
    expect(pdf.getPageCount()).toBe(4); // summary, register, appendix p1, appendix p2
    const firstAppendixPage = pdf.getPage(2);
    expect(pageText(pdf, 2)).toContain('RECEIPT APPENDIX');
    expect(pageText(pdf, 3)).toContain('RECEIPT APPENDIX');
    const destLinks = linkAnnotations(pdf.getPage(0)).filter((a) => a.has(PDFName.of('Dest')));
    expect(destLinks).toHaveLength(1);
    expect(destPageRef(destLinks[0])?.toString()).toBe(firstAppendixPage.ref.toString());
  });
  it('carries a hidden deep-link URI on the register filename', async () => {
    const s = fixture();
    s.includeReceipts = false;
    const data = await generateClaimPdf(s, [], await loadFont());
    const pdf = await PDFDocument.load(data);
    const uriLinks = linkAnnotations(pdf.getPage(1))
      .map((a) => uriOf(a))
      .filter((u): u is string => !!u);
    expect(uriLinks).toContain('expenseapp://receipt/r1');
    // Hidden: no visible border.
    const withUri = linkAnnotations(pdf.getPage(1)).find(
      (a) => uriOf(a) === 'expenseapp://receipt/r1',
    );
    expect(
      withUri
        ?.lookup(PDFName.of('Border'), PDFArray)
        .asArray()
        .map((n) => n.toString()),
    ).toEqual(['0', '0', '0']);
  });
  it('carries a hidden deep-link URI and a "Back to summary" link on the appendix heading', async () => {
    const s = fixture();
    s.includeReceipts = true;
    const data = await generateClaimPdf(s, [await makePdfReceipt(1)], await loadFont());
    const pdf = await PDFDocument.load(data);
    const appendixPage = pdf.getPage(2);
    const annots = linkAnnotations(appendixPage);
    const uriLinks = annots.map((a) => uriOf(a)).filter((u): u is string => !!u);
    expect(uriLinks).toContain('expenseapp://receipt/r1');
    const backLinks = annots.filter((a) => a.has(PDFName.of('Dest')));
    expect(backLinks).toHaveLength(1);
    expect(destPageRef(backLinks[0])?.toString()).toBe(pdf.getPage(0).ref.toString());
    expect(pageText(pdf, 2)).toContain('Back to summary');
  });
  it('links each expense to its own receipt appendix, not another expense in the same claim', async () => {
    const s = fixture();
    s.includeReceipts = true;
    s.expenses = [
      { ...s.expenses[0], id: 'e1', receiptId: 'r1', merchantName: 'Kopi House' },
      { ...s.expenses[0], id: 'e2', receiptId: 'r2', merchantName: 'Grocer' },
    ];
    s.receipts = [
      { ...s.receipts[0], id: 'r1' },
      { ...s.receipts[0], id: 'r2' },
    ];
    const r1 = await makeImageReceipt();
    const r2 = await makeImageReceipt();
    r2.receipt.id = 'r2';
    const data = await generateClaimPdf(s, [r1, r2], await loadFont());
    const pdf = await PDFDocument.load(data);
    expect(pdf.getPageCount()).toBe(4); // summary, register, appendix r1, appendix r2
    const appendixR1 = pdf.getPage(2),
      appendixR2 = pdf.getPage(3);
    const destLinks = linkAnnotations(pdf.getPage(0))
      .filter((a) => a.has(PDFName.of('Dest')))
      .map((a) => destPageRef(a)?.toString());
    expect(destLinks).toEqual([appendixR1.ref.toString(), appendixR2.ref.toString()]);
    // Each appendix page's "Back to summary" link still points at the one
    // summary page (both rows fit on page 0), not at each other's page.
    for (const page of [appendixR1, appendixR2]) {
      const back = linkAnnotations(page).filter((a) => a.has(PDFName.of('Dest')));
      expect(back).toHaveLength(1);
      expect(destPageRef(back[0])?.toString()).toBe(pdf.getPage(0).ref.toString());
    }
  });
});
