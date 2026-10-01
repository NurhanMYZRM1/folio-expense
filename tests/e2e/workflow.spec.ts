import { test, expect } from '@playwright/test';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { PDFDocument, PDFName, PDFArray, StandardFonts } from 'pdf-lib';
import { RustBridge, installBridge } from '../bridge';
import type { Expense, Claim, Job, CsvExport } from '../../src/bindings/generated';
test('offline receipt → real Rust/SQLite → OCR → review → claim → PDF → reopen', async ({
  page,
}, testInfo) => {
  page.on('pageerror', (e) => console.log('browser error:', e.message));
  const directory = await mkdtemp(path.join(tmpdir(), 'folio-e2e-'));
  const receiptPath = path.join(directory, 'client-lunch.png');
  // Generate a readable receipt in the browser; the real OCR worker must extract it.
  await page.goto('/');
  const data = await page.evaluate(() => {
    const c = document.createElement('canvas');
    c.width = 850;
    c.height = 1050;
    const x = c.getContext('2d')!;
    x.fillStyle = 'white';
    x.fillRect(0, 0, 850, 1050);
    x.fillStyle = '#111';
    x.textAlign = 'center';
    x.font = 'bold 44px Arial';
    x.fillText('KOPI HOUSE', 425, 100);
    x.font = '26px Arial';
    x.fillText('Kuala Lumpur, Malaysia', 425, 152);
    x.fillText('RECEIPT', 425, 230);
    x.textAlign = 'left';
    x.fillText('Date: 2026-09-27', 80, 310);
    x.fillText('Receipt No: 00827', 80, 360);
    x.fillText('Business lunch', 80, 460);
    x.fillText('Subtotal', 80, 570);
    x.fillText('79.72', 620, 570);
    x.fillText('Tax', 80, 630);
    x.fillText('4.78', 620, 630);
    x.fillRect(80, 680, 680, 2);
    x.font = 'bold 34px Arial';
    x.fillText('TOTAL MYR 84.50', 80, 750);
    x.textAlign = 'center';
    x.font = '25px Arial';
    x.fillText('Thank you for dining with us', 425, 900);
    return c.toDataURL('image/png').split(',')[1];
  });
  await writeFile(receiptPath, Buffer.from(data, 'base64'));
  const original = await readFile(receiptPath);
  let bridge = new RustBridge(path.join(directory, 'app'));
  // Dynamic delegate allows closing/reopening the actual Rust service during the test.
  const proxy = {
    call: <T>(cmd: string, args: Record<string, unknown>) => bridge.call<T>(cmd, args),
  } as RustBridge;
  await installBridge(page, proxy, [receiptPath]);
  await page.goto('about:blank');
  const external: string[] = [];
  await page.route('**/*', (route) => {
    if (
      !route.request().url().startsWith('http://127.0.0.1:1420') &&
      !route.request().url().startsWith('data:')
    ) {
      external.push(route.request().url());
      return route.abort();
    }
    return route.continue();
  });
  try {
    await page.goto('/#/import');
    await page.getByRole('button', { name: /Drag receipts into your workspace/ }).click();
    await expect(page.getByText('Original saved. Extraction and preview queued.')).toBeVisible();
    await expect
      .poll(async () => (await bridge.call<Expense[]>('list_expenses'))[0]?.status, {
        timeout: 80_000,
      })
      // Subtotal + tax = total on a clear scan, so no manual review is needed.
      .toBe('ready');
    let [expense] = await bridge.call<Expense[]>('list_expenses');
    expect(expense.totalAmountMinor).toBe(8450);
    expect(expense.taxAmountMinor).toBe(478);
    expect(expense.occurredAt).toBe('2026-09-27');
    expect(expense.currency).toBe('MYR');
    expect(expense.category).toBe('Meals');
    expect(expense.merchantName).toContain('KOPI');
    const receiptId = expense.receiptId!;
    await page.getByRole('button', { name: /Drag receipts into your workspace/ }).click();
    await expect(
      page.getByText('This receipt appears to have already been imported.'),
    ).toBeVisible();
    await page.getByRole('link', { name: 'Open existing expense' }).click();
    await page.getByLabel('Merchant', { exact: false }).fill('Kopi House · client lunch');
    await page
      .getByLabel('Business purpose / notes', { exact: false })
      .fill('Project kickoff meeting with client team');
    await page.getByLabel('Category', { exact: false }).selectOption('Meals');
    await page.getByRole('button', { name: 'Mark as ready' }).click();
    await expect(page.getByText('Expense reviewed and ready to claim.')).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath('expense-editor.png'), fullPage: true });
    await page.getByRole('link', { name: 'Claims', exact: true }).click();
    await page.getByRole('button', { name: 'New claim' }).click();
    await page.getByLabel('Claim title').fill('September client meetings');
    await page.getByRole('button', { name: 'Create claim', exact: true }).click();
    await page.getByRole('button', { name: 'Add expenses' }).click();
    await page.getByRole('button', { name: 'Add', exact: true }).click();
    await page.getByRole('button', { name: 'Done', exact: true }).click();
    await page.getByRole('button', { name: 'Export PDF' }).click();
    await expect(page.getByRole('button', { name: 'Open PDF' })).toBeVisible({ timeout: 50_000 });
    const [claim] = await bridge.call<Claim[]>('list_claims');
    expect(claim.totalAmountMinor).toBe(8450);
    const exports = (await bridge.call<Job[]>('list_jobs')).filter(
      (j) => j.jobType === 'generate_pdf',
    );
    expect(exports[0].status).toBe('completed');
    const pdfPath = await bridge.call<string>('open_export', { id: exports[0].id });
    const bytes = await readFile(pdfPath);
    const pdf = await PDFDocument.load(bytes);
    expect(pdf.getPageCount()).toBe(3);
    expect(pdf.getPage(0).node.lookup(PDFName.of('Annots'), PDFArray).size()).toBeGreaterThan(0);
    await writeFile(testInfo.outputPath('claim.pdf'), bytes);
    await writeFile(testInfo.outputPath('receipt.png'), original);
    await page.getByRole('button', { name: 'Export CSV' }).click();
    await expect(page.getByRole('button', { name: 'Open CSV' })).toBeVisible();
    await expect(
      page.locator('.inline-export-note', { hasText: `Folio-${claim.claimNumber}.csv` }),
    ).toBeVisible();
    const csvExport = await bridge.call<CsvExport>('export_claim_csv', { id: claim.id });
    expect(csvExport.fileName).toBe(`Folio-${claim.claimNumber}.csv`);
    const csvText = (await readFile(csvExport.path)).toString('utf-8');
    expect(csvText).toContain('Kopi House · client lunch');
    expect(csvText).toContain('84.50');
    await writeFile(testInfo.outputPath('claim.csv'), csvText);
    while (await page.getByRole('button', { name: 'Dismiss notification' }).count())
      await page.getByRole('button', { name: 'Dismiss notification' }).first().click();
    await page.screenshot({ path: testInfo.outputPath('claim-detail.png'), fullPage: true });
    await page.getByRole('link', { name: 'Overview', exact: true }).click();
    await expect(
      page.getByRole('heading', { name: 'Expense overview', exact: true }),
    ).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath('dashboard.png'), fullPage: true });
    await page.getByRole('button', { name: 'Toggle color theme' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await page.screenshot({ path: testInfo.outputPath('dashboard-dark.png'), fullPage: true });
    expect(external).toEqual([]);
    expect(await readFile(receiptPath)).toEqual(original);
    await page.goto('about:blank');
    await bridge.close();
    bridge = new RustBridge(path.join(directory, 'app'));
    await page.goto('/#/expenses');
    await expect(
      page.getByRole('link', { name: 'Open Kopi House · client lunch', exact: true }),
    ).toBeVisible();
    [expense] = await bridge.call<Expense[]>('list_expenses');
    expect(expense.receiptId).toBe(receiptId);
    expect(expense.fieldMeta.merchantName?.source).toBe('manual');
    expect((await bridge.call<Claim[]>('list_claims'))[0].id).toBe(claim.id);
    await page.getByLabel('Search expenses').fill('does not exist');
    await expect(page.getByText('No matching expenses')).toBeVisible();
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});

test('multi-page PDF receipt renders and extracts entirely offline', async ({ page }, testInfo) => {
  const directory = await mkdtemp(path.join(tmpdir(), 'folio-pdf-e2e-'));
  const receiptPath = path.join(directory, 'hotel-receipt.pdf');
  const receipt = await PDFDocument.create();
  const font = await receipt.embedFont(StandardFonts.Helvetica);
  const first = receipt.addPage([600, 800]);
  for (const [index, text] of [
    'CITY HOTEL',
    'Date: 2026-09-27',
    'Accommodation',
    'Tax MYR 18.00',
    'TOTAL MYR 318.00',
  ].entries())
    first.drawText(text, { x: 70, y: 700 - index * 100, size: 26, font });
  receipt
    .addPage([600, 800])
    .drawText('Thank you for your stay', { x: 70, y: 700, size: 26, font });
  await writeFile(receiptPath, await receipt.save());
  const bridge = new RustBridge(path.join(directory, 'app'));
  await installBridge(page, bridge, [receiptPath]);
  const external: string[] = [];
  await page.route('**/*', (route) => {
    if (
      !route.request().url().startsWith('http://127.0.0.1:1420') &&
      !route.request().url().startsWith('data:')
    ) {
      external.push(route.request().url());
      return route.abort();
    }
    return route.continue();
  });
  try {
    await page.goto('/#/import');
    await page.getByRole('button', { name: /Drag receipts into your workspace/ }).click();
    await expect
      .poll(async () => (await bridge.call<Expense[]>('list_expenses'))[0]?.status, {
        timeout: 80_000,
      })
      // The total is the largest amount on a clearly read page.
      .toBe('ready');
    const [expense] = await bridge.call<Expense[]>('list_expenses');
    expect(expense.totalAmountMinor).toBe(31800);
    expect(expense.category).toBe('Accommodation');
    expect(expense.merchantName).toContain('CITY HOTEL');
    expect((await bridge.call<Job[]>('list_jobs')).every((job) => job.status === 'completed')).toBe(
      true,
    );
    await page.goto(`/#/expenses/${expense.id}`);
    // Rendering waits for the pdf.js worker, which cold CI dev servers can take seconds to serve.
    await expect(page.getByRole('img', { name: 'Preview of hotel-receipt.pdf' })).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByText('1 / 2', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Next receipt page' }).click();
    await expect(page.getByText('2 / 2', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Mark as ready' }).click();
    await expect(page.getByText('Expense reviewed and ready to claim.')).toBeVisible();
    const claim = await bridge.call<Claim>('create_claim', {
      title: 'Hotel stay',
      currency: 'MYR',
    });
    await bridge.call('set_claim_expense', { claimId: claim.id, expenseId: expense.id, add: true });
    await page.goto(`/#/claims/${claim.id}`);
    await page.getByRole('button', { name: 'Export PDF' }).click();
    await expect(page.getByRole('button', { name: 'Open PDF' })).toBeVisible({ timeout: 50_000 });
    const job = (await bridge.call<Job[]>('list_jobs')).find(
      (job) => job.jobType === 'generate_pdf',
    )!;
    const bytes = await readFile(await bridge.call<string>('open_export', { id: job.id }));
    expect((await PDFDocument.load(bytes)).getPageCount()).toBe(4);
    await writeFile(testInfo.outputPath('pdf-receipt-claim.pdf'), bytes);
    expect(external).toEqual([]);
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});
