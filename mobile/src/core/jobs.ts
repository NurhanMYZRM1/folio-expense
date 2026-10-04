// Native port of ../src/lib/jobs.ts: one runner for the whole app takes
// durable jobs from the Rust core and does the work the desktop does in its
// webview — receipt text (Apple Vision instead of tesseract.js), previews
// (PDFKit / expo-image-manipulator instead of canvas) and claim PDFs (the
// shared pdf-lib generator, unchanged).
import { Buffer } from "buffer";
import { Asset } from "expo-asset";
import { File } from "expo-file-system";
import { ImageManipulator, SaveFormat } from "expo-image-manipulator";
import { generateClaimPdf } from "@folio/lib/pdf";
import type { ExportSnapshot, JobLease, ReceiptContent, ReceiptFile, Settings } from "@folio/bindings/generated";
import FolioCore from "../../modules/folio-core";
import { call, dataRoot, errorText } from "./folio";

const MAX_PAGES = 30;
const fileUri = (path: string) => `file://${encodeURI(path)}`;

export type JobProgress = (message: string | null) => void;
export type JobNotice = (message: string, error?: boolean) => void;

let font: Uint8Array | null = null;
async function reportFont(): Promise<Uint8Array> {
  if (font) return font;
  const asset = Asset.fromModule(require("@fontsource/noto-sans/files/noto-sans-latin-400-normal.woff"));
  await asset.downloadAsync();
  font = new File(asset.localUri ?? asset.uri).bytesSync();
  return font;
}

const receiptPath = (r: ReceiptFile) => `${dataRoot()}/${r.relativePath}`;

/** Page images for OCR / online extraction: PDF pages via PDFKit, images as stored. */
async function pageImages(r: ReceiptFile): Promise<string[]> {
  if (r.mimeType === "application/pdf") return FolioCore.renderPdfPages(receiptPath(r), MAX_PAGES, 2200);
  return [receiptPath(r)];
}

async function resizedJpeg(path: string, maxSide: number, base64: true): Promise<string>;
async function resizedJpeg(path: string, maxSide: number, base64: false): Promise<string>;
async function resizedJpeg(path: string, maxSide: number, base64: boolean): Promise<string> {
  const source = await ImageManipulator.manipulate(fileUri(path)).renderAsync();
  const scale = Math.min(1, maxSide / Math.max(source.width, source.height));
  const ctx = ImageManipulator.manipulate(fileUri(path));
  if (scale < 1) ctx.resize({ width: Math.round(source.width * scale), height: Math.round(source.height * scale) });
  const saved = await (await ctx.renderAsync()).saveAsync({ format: SaveFormat.JPEG, compress: 0.85, base64 });
  return base64 ? saved.base64! : saved.uri;
}

async function thumbnail(r: ReceiptFile): Promise<string> {
  const first = r.mimeType === "application/pdf" ? (await FolioCore.renderPdfPages(receiptPath(r), 1, 600))[0] : receiptPath(r);
  if (!first) throw new Error("Unable to render the receipt.");
  return resizedJpeg(first, 560, true);
}

async function extract(lease: JobLease, progress: JobProgress) {
  const { job, token, receiptId } = lease;
  const receipt = (await call<ReceiptContent>("read_receipt", { id: receiptId })).receipt;
  const settings = await call<Settings>("get_settings");
  const pages = await pageImages(receipt);
  let completed = false;
  if (settings.onlineEnabled) {
    progress("Trying online AI extraction");
    const images: string[] = [];
    for (const page of pages) images.push(`data:image/jpeg;base64,${await resizedJpeg(page, 2200, true)}`);
    completed = await call<boolean>("try_online", { id: job.id, token, images });
  }
  if (completed) return;
  if (!settings.offlineOcrEnabled) {
    throw new Error("Automatic extraction is unavailable. The receipt was saved locally and can still be entered manually.");
  }
  let rawText = "";
  let weighted = 0;
  let length = 0;
  for (const [i, page] of pages.entries()) {
    progress(`Reading receipt · page ${i + 1}`);
    const result = await FolioCore.recognizeText(page);
    rawText += `${result.text}\n`;
    const n = result.text.trim().length;
    if (result.confidence != null) weighted += result.confidence * n;
    length += n;
  }
  await call("complete_ocr", { id: job.id, token, rawText, ocrConfidence: length ? weighted / length : null });
}

async function report(lease: JobLease) {
  const { job, token } = lease;
  const snapshot = await call<ExportSnapshot>("export_snapshot", { id: job.id, token });
  const receipts: ReceiptContent[] = [];
  if (snapshot.includeReceipts) {
    for (const r of snapshot.receipts) {
      receipts.push(await call<ReceiptContent>("read_receipt", { id: r.id }));
      if (receipts.reduce((sum, x) => sum + x.base64.length, 0) > 120 * 1024 * 1024) {
        throw new Error("Receipt appendix exceeds 90 MB. Split this claim or turn off the appendix.");
      }
    }
  }
  const bytes = await generateClaimPdf(snapshot, receipts, await reportFont());
  await call("complete_pdf", { id: job.id, token, base64: Buffer.from(bytes).toString("base64") });
}

export function startJobRunner(progress: JobProgress, notify: JobNotice): () => void {
  let stopped = false;
  let active = false;

  const tick = async () => {
    if (stopped || active) return;
    active = true;
    let lease: JobLease | null = null;
    let heartbeat: ReturnType<typeof setInterval> | undefined;
    try {
      lease = await call<JobLease | null>("take_job");
      if (!lease) return;
      const { job, token } = lease;
      heartbeat = setInterval(() => void call("heartbeat", { id: job.id, token }).catch(() => undefined), 20_000);
      progress(job.jobType === "generate_pdf" ? "Building claim report" : job.jobType === "generate_thumbnail" ? "Preparing receipt preview" : "Reading receipt");
      if (job.jobType === "generate_pdf") {
        await report(lease);
        notify("Claim PDF saved on this device. Open it from the claim’s generated reports.");
      } else if (job.jobType === "generate_thumbnail") {
        const receipt = (await call<ReceiptContent>("read_receipt", { id: lease.receiptId })).receipt;
        await call("complete_thumbnail", { id: job.id, token, base64: await thumbnail(receipt) });
      } else {
        await extract(lease, progress);
      }
    } catch (error) {
      const message = errorText(error);
      if (lease) await call("fail_job", { id: lease.job.id, token: lease.token, message }).catch(() => undefined);
      notify(message, true);
    } finally {
      if (heartbeat) clearInterval(heartbeat);
      active = false;
      progress(null);
    }
  };

  // Foreign receipts imported offline are converted once rates can be downloaded.
  const convert = () => void call<number>("convert_pending").catch(() => undefined);

  const jobs = setInterval(() => void tick(), 1500);
  const conversions = setInterval(convert, 120_000);
  void tick();
  convert();
  return () => {
    stopped = true;
    clearInterval(jobs);
    clearInterval(conversions);
  };
}
