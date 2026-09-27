import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { PDFDocument, PDFArray, PDFName } from 'pdf-lib';
import { generateClaimPdf, receiptReference } from './pdf';
import type { ExportSnapshot, Expense, ReceiptContent } from '../bindings/generated';
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
describe('claim PDF', () => {
  it('embeds the local font, exact totals, and a deep link', async () => {
    const font = await readFile('public/fonts/noto-sans-latin-400-normal.woff');
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
    const original = await PDFDocument.create();
    original.addPage([300, 400]);
    original.addPage([300, 400]);
    const receipt: ReceiptContent = {
      receipt: s.receipts[0],
      base64: Buffer.from(await original.save()).toString('base64'),
    };
    const data = await generateClaimPdf(
      s,
      [receipt],
      await readFile('public/fonts/noto-sans-latin-400-normal.woff'),
    );
    const pdf = await PDFDocument.load(data);
    expect(pdf.getPageCount()).toBeGreaterThan(9);
  });
  it('fails explicitly instead of silently omitting missing appendix receipts', async () => {
    const s = fixture();
    s.includeReceipts = true;
    await expect(
      generateClaimPdf(s, [], await readFile('public/fonts/noto-sans-latin-400-normal.woff')),
    ).rejects.toThrow('Receipt is missing');
  });
});
