import { test, expect } from '@playwright/test';
import { copyFile, mkdtemp, readFile, writeFile } from 'node:fs/promises';
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
    await page.getByRole('button', { name: 'Export Excel' }).click();
    await expect(page.getByRole('button', { name: 'Open Excel' })).toBeVisible();
    await expect(
      page.locator('.inline-export-note', { hasText: `Folio-${claim.claimNumber}.xlsx` }),
    ).toBeVisible();
    const xlsxExport = await bridge.call<CsvExport>('export_claim_xlsx', { id: claim.id });
    expect(xlsxExport.fileName).toBe(`Folio-${claim.claimNumber}.xlsx`);
    expect(await bridge.call<string>('open_xlsx_export', { id: xlsxExport.id })).toBe(
      xlsxExport.path,
    );
    const xlsxBytes = await readFile(xlsxExport.path);
    expect(xlsxBytes.subarray(0, 2).toString('latin1')).toBe('PK');
    await writeFile(testInfo.outputPath('claim.xlsx'), xlsxBytes);
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

    // Values stay correctable once the claim is submitted, and only a real
    // content change (not the submission itself) outdates an exported report.
    await page.goto(`/#/claims/${claim.id}`);
    await page.getByRole('button', { name: 'Mark submitted' }).click();
    await expect(
      page.getByText('Claim marked as submitted locally. No data was sent.'),
    ).toBeVisible();
    await expect(page.getByText('Saved locally · snapshot of claim at export time')).toBeVisible();
    await expect(page.getByText('Outdated', { exact: false })).toHaveCount(0);
    await page.goto(`/#/expenses/${expense.id}`);
    await expect(page.getByText('You can still correct its values')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Mark as ready' })).toHaveCount(0);
    await page.getByLabel('Merchant', { exact: false }).fill('Kopi House · corrected after submit');
    await page.getByRole('button', { name: 'Save changes' }).click();
    await expect(
      page.getByText('Changes saved. Export the claim report again to include them.'),
    ).toBeVisible();
    [expense] = await bridge.call<Expense[]>('list_expenses');
    expect(expense.merchantName).toBe('Kopi House · corrected after submit');
    expect(expense.status).toBe('submitted');
    await page.goto(`/#/claims/${claim.id}`);
    await expect(
      page.getByText('Outdated · the claim changed after this report was generated'),
    ).toBeVisible();
    await expect(page.getByText('Export a new PDF to include the latest values.')).toBeVisible();
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

test('imported receipts are added to a claim in bulk and deleted with their files', async ({
  page,
}) => {
  const directory = await mkdtemp(path.join(tmpdir(), 'folio-bulk-e2e-'));
  await page.goto('/');
  const receipts: string[] = [];
  for (const [name, merchant, lines] of [
    ['cafe.png', 'KOPI HOUSE', ['Subtotal 79.72', 'Tax 4.78', 'TOTAL MYR 84.50']],
    ['parking.png', 'CITY PARKING', ['Parking 2 hours', 'TOTAL MYR 6.00', 'CASH 6.00']],
  ] as const) {
    const data = await page.evaluate(
      ([merchant, lines]) => {
        const c = document.createElement('canvas');
        c.width = 850;
        c.height = 900;
        const x = c.getContext('2d')!;
        x.fillStyle = 'white';
        x.fillRect(0, 0, 850, 900);
        x.fillStyle = '#111';
        x.font = 'bold 44px Arial';
        x.fillText(merchant, 80, 100);
        x.font = '30px Arial';
        x.fillText('Date: 2026-09-27', 80, 220);
        lines.forEach((line, i) => x.fillText(line, 80, 340 + i * 80));
        return c.toDataURL('image/png').split(',')[1];
      },
      [merchant, lines] as [string, readonly string[]],
    );
    const file = path.join(directory, name);
    await writeFile(file, Buffer.from(data, 'base64'));
    receipts.push(file);
  }
  const bridge = new RustBridge(path.join(directory, 'app'));
  await installBridge(page, bridge, receipts);
  // A full navigation so the bridge's init script runs on the app page.
  await page.goto('about:blank');
  try {
    await page.goto('/#/import');
    await page.getByRole('button', { name: /Drag receipts into your workspace/ }).click();
    await expect
      .poll(
        async () =>
          (await bridge.call<Expense[]>('list_expenses'))
            .map((e) => e.status)
            .sort()
            .join(','),
        { timeout: 90_000 },
      )
      .toBe('ready,ready');
    // Add both imported receipts to a new claim at once.
    await page.getByRole('link', { name: 'Claims', exact: true }).click();
    await page.getByRole('button', { name: 'New claim' }).click();
    await page.getByLabel('Claim title').fill('Bulk claim');
    await page.getByRole('button', { name: 'Create claim', exact: true }).click();
    await page.getByRole('button', { name: 'Add expenses' }).click();
    await page.getByRole('button', { name: 'Add all 2' }).click();
    await expect
      .poll(async () => (await bridge.call<Claim[]>('list_claims'))[0]?.expenseCount)
      .toBe(2);
    // Select one expense in the register and delete it.
    const parking = (await bridge.call<Expense[]>('list_expenses')).find((e) =>
      e.merchantName?.includes('PARKING'),
    )!;
    await page.goto('/#/expenses');
    await page.getByRole('checkbox', { name: `Select ${parking.merchantName}` }).check();
    await page.getByRole('button', { name: 'Delete', exact: true }).click();
    await page.getByRole('button', { name: 'Delete', exact: true }).click();
    await expect(page.getByText('Deleted 1 expense and their receipts.')).toBeVisible();
    const remaining = await bridge.call<Expense[]>('list_expenses');
    expect(remaining.map((e) => e.id)).not.toContain(parking.id);
    expect((await bridge.call<Claim[]>('list_claims'))[0].expenseCount).toBe(1);
    await expect(
      readFile(path.join(directory, 'app', 'receipts', parking.receiptId!, 'original.png')),
    ).rejects.toThrow();
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});

test('an iPhone HEIC receipt is read, previewed and kept as the original', async ({ page }) => {
  const directory = await mkdtemp(path.join(tmpdir(), 'folio-heic-e2e-'));
  // A synthetic receipt saved as HEIC by macOS (no real photo is committed).
  const receiptPath = path.join(directory, 'IMG_0427.HEIC');
  await copyFile(path.resolve('tests/fixtures/lunch.heic'), receiptPath);
  const original = await readFile(receiptPath);
  const bridge = new RustBridge(path.join(directory, 'app'));
  await installBridge(page, bridge, [receiptPath]);
  await page.goto('about:blank');
  try {
    await page.goto('/#/import');
    await page.getByRole('button', { name: /Drag receipts into your workspace/ }).click();
    await expect(page.getByText('Original saved. Extraction and preview queued.')).toBeVisible();
    // Local OCR reads the JPEG rendition, since Chromium cannot decode HEIC.
    await expect
      .poll(async () => (await bridge.call<Expense[]>('list_expenses'))[0]?.status, {
        timeout: 80_000,
      })
      .toBe('ready');
    const [expense] = await bridge.call<Expense[]>('list_expenses');
    expect(expense.totalAmountMinor).toBe(2500);
    expect(expense.taxAmountMinor).toBe(142);
    expect(expense.receiptFilename).toBe('IMG_0427.HEIC');
    const receiptDir = path.join(directory, 'app', 'receipts', expense.receiptId!);
    expect(Buffer.compare(await readFile(path.join(receiptDir, 'original.heic')), original)).toBe(
      0,
    );
    expect((await readFile(path.join(receiptDir, 'display.jpg'))).subarray(0, 3)).toEqual(
      Buffer.from([0xff, 0xd8, 0xff]),
    );
    // The thumbnail job ran on the rendition too.
    await expect
      .poll(async () => {
        try {
          return (await readFile(path.join(receiptDir, 'preview.webp'))).subarray(8, 12).toString();
        } catch {
          return '';
        }
      })
      .toBe('WEBP');
    // The detail viewer draws the rendition at its real size.
    await page.goto(`/#/expenses/${expense.id}`);
    const preview = page.getByRole('img', { name: 'Preview of IMG_0427.HEIC' });
    await expect(preview).toBeVisible();
    await expect
      .poll(() => preview.evaluate((img: HTMLImageElement) => img.naturalWidth))
      .toBe(850);
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});

test('a phone photo with a hand shadow over the totals still reads the total', async ({ page }) => {
  const directory = await mkdtemp(path.join(tmpdir(), 'folio-shadow-e2e-'));
  // Synthetic receipt (no real photo is committed): the lower right, where
  // Total/Paid/Change are printed, sits in a shadow at ~1/3 brightness, the
  // way a hand or phone shades a receipt photographed on a table.
  const receiptPath = path.join(directory, 'IMG_7808.HEIC');
  await copyFile(path.resolve('tests/fixtures/shadowed.heic'), receiptPath);
  const bridge = new RustBridge(path.join(directory, 'app'));
  await installBridge(page, bridge, [receiptPath]);
  await page.goto('about:blank');
  try {
    await page.goto('/#/import');
    await page.getByRole('button', { name: /Drag receipts into your workspace/ }).click();
    await expect(page.getByText('Original saved. Extraction and preview queued.')).toBeVisible();
    // Extraction is finished once the expense leaves `extracting`; before the
    // fix it landed in needs_review with no total and a ¥/JPY guess.
    await expect
      .poll(async () => (await bridge.call<Expense[]>('list_expenses'))[0]?.status, {
        timeout: 80_000,
      })
      .toMatch(/^(ready|needs_review)$/);
    const [expense] = await bridge.call<Expense[]>('list_expenses');
    expect(expense.merchantName).toBe('KEDAI RUNCIT AMAN');
    expect(expense.occurredAt).toBe('2026-09-27');
    expect(expense.currency).toBe('MYR');
    expect(expense.totalAmountMinor).toBe(28300);
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});
