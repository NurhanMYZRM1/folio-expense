import { createWorker, OEM, PSM, type Worker as OcrWorker } from 'tesseract.js';
import { api } from './ipc';
import { encodeBase64 } from './encoding';
import { errorMessage } from './errors';
import { bestPass, isGoodPass, type OcrPass } from './ocrPasses';
import type { ExportSnapshot, JobLease, ReceiptContent } from '../bindings/generated';
type Callbacks = {
  refresh: () => Promise<void>;
  notify: (message: string, error?: boolean) => void;
  progress: (message: string | null) => void;
};
let active = false;
async function renderReport(
  snapshot: ExportSnapshot,
  receipts: ReceiptContent[],
): Promise<Uint8Array> {
  const font = new Uint8Array(
    await (await fetch('/fonts/noto-sans-latin-400-normal.woff')).arrayBuffer(),
  );
  const worker = new Worker(new URL('./pdf.worker.ts', import.meta.url), { type: 'module' });
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      worker.terminate();
      reject(new Error('PDF generation timed out. Try a smaller claim.'));
    }, 180_000);
    const finish = () => {
      clearTimeout(timer);
      worker.terminate();
    };
    worker.onmessage = (e) => {
      finish();
      if (e.data.error) reject(new Error(e.data.error));
      else resolve(e.data.bytes as Uint8Array);
    };
    worker.onerror = () => {
      finish();
      reject(new Error('PDF generation was interrupted. Your claim is still saved.'));
    };
    worker.postMessage({ snapshot, receipts, font }, [font.buffer]);
  });
}
export function startJobRunner(callbacks: Callbacks): () => void {
  let stopped = false,
    ocr: OcrWorker | undefined,
    current: JobLease | null = null;
  const progress = (text: string | null) => {
    if (!stopped) callbacks.progress(text);
  };
  const tick = async () => {
    if (stopped || active) return;
    active = true;
    let heartbeat: ReturnType<typeof setInterval> | undefined;
    let didWork = false;
    try {
      current = await api.takeJob();
      if (!current) return;
      didWork = true;
      const { job, token, receiptId } = current;
      const id = job.id;
      heartbeat = setInterval(() => void api.heartbeat(id, token).catch(() => {}), 20_000);
      progress(
        job.jobType === 'generate_pdf'
          ? 'Building claim report'
          : job.jobType === 'generate_thumbnail'
            ? 'Preparing receipt preview'
            : 'Reading receipt',
      );
      await callbacks.refresh();
      if (job.jobType === 'generate_pdf') {
        const snapshot = await api.exportSnapshot(id, token),
          receipts: ReceiptContent[] = [];
        if (snapshot.includeReceipts) {
          for (const receipt of snapshot.receipts) {
            receipts.push(await api.receipt(receipt.id));
            if (receipts.reduce((sum, r) => sum + r.base64.length, 0) > 120 * 1024 * 1024)
              throw new Error(
                'Receipt appendix exceeds 90 MB. Split this claim or turn off the appendix.',
              );
          }
        }
        const bytes = await renderReport(snapshot, receipts);
        await api.completePdf(id, token, encodeBase64(bytes));
        callbacks.notify('Claim PDF saved locally. Open it from the claim’s generated reports.');
      } else {
        if (!receiptId) throw new Error('The receipt for this job is missing.');
        const receipt = await api.receipt(receiptId);
        const { receiptPages, thumbnail, preprocessCanvas, binarizedCanvas } =
          await import('./receiptRendering');
        if (job.jobType === 'generate_thumbnail') {
          await api.completeThumbnail(id, token, await thumbnail(receipt));
        } else {
          const settings = await api.settings();
          let completed = false;
          if (settings.onlineEnabled) {
            progress('Trying online AI extraction');
            const images: string[] = [];
            for await (const canvas of receiptPages(receipt)) {
              images.push(preprocessCanvas(canvas).toDataURL('image/jpeg', 0.85));
              canvas.width = canvas.height = 0;
            }
            completed = await api.tryOnline(id, token, images);
          }
          if (!completed) {
            if (!settings.offlineOcrEnabled)
              throw new Error(
                'Automatic extraction is unavailable. The receipt was saved locally and can still be entered manually.',
              );
            if (!ocr) {
              ocr = await createWorker('eng', OEM.LSTM_ONLY, {
                workerPath: `${location.origin}/ocr/worker.min.js`,
                corePath: `${location.origin}/ocr`,
                langPath: `${location.origin}/ocr`,
                gzip: true,
                cacheMethod: 'none',
                workerBlobURL: false,
                logger: (status) => {
                  if (status.status === 'recognizing text')
                    progress(`Local OCR · ${Math.round(status.progress * 100)}%`);
                },
              });
              // The page layout mode is set per pass below.
              await ocr.setParameters({ preserve_interword_spaces: '1' });
            }
            let rawText = '',
              page = 0,
              // Tesseract's 0–100 confidence, averaged over pages by how much
              // text each page holds (a blank back page shouldn't count).
              weightedConfidence = 0,
              textLength = 0;
            const worker = ocr;
            const read = async (image: HTMLCanvasElement, mode: PSM): Promise<OcrPass> => {
              await worker.setParameters({ tessedit_pageseg_mode: mode });
              let timer: ReturnType<typeof setTimeout> | undefined;
              const result = await Promise.race([
                worker.recognize(image),
                new Promise<never>((_, reject) => {
                  timer = setTimeout(
                    () =>
                      reject(
                        new Error(
                          'Local OCR took too long. The receipt is available for manual entry.',
                        ),
                      ),
                    120_000,
                  );
                }),
              ]).finally(() => clearTimeout(timer));
              return { text: result.data.text, confidence: result.data.confidence };
            };
            for await (const canvas of receiptPages(receipt)) {
              progress(`Local OCR · page ${++page}`);
              const prepared = preprocessCanvas(canvas);
              // See ocrPasses.ts: retry only when the first reading looks incomplete.
              const passes = [await read(prepared, PSM.SINGLE_COLUMN)];
              if (!isGoodPass(passes[0])) {
                progress(`Local OCR · page ${page} · second look`);
                passes.push(await read(prepared, PSM.SINGLE_BLOCK));
              }
              if (!passes.some(isGoodPass)) {
                progress(`Local OCR · page ${page} · black-and-white`);
                passes.push(await read(binarizedCanvas(prepared), PSM.SINGLE_BLOCK));
              }
              const result = bestPass(passes);
              rawText += `${result.text}\n`;
              const length = result.text.trim().length;
              weightedConfidence += result.confidence * length;
              textLength += length;
              canvas.width = canvas.height = prepared.width = prepared.height = 0;
            }
            await api.completeOcr(
              id,
              token,
              rawText,
              textLength ? weightedConfidence / textLength : null,
            );
          }
        }
      }
    } catch (error) {
      const message = errorMessage(error);
      if (current) {
        await api.failJob(current.job.id, current.token, message).catch(() => {});
        if (ocr) {
          await ocr.terminate();
          ocr = undefined;
        }
      }
      callbacks.notify(message, true);
    } finally {
      if (heartbeat) clearInterval(heartbeat);
      current = null;
      active = false;
      progress(null);
      if (!stopped && didWork) await callbacks.refresh();
      else if (stopped && ocr) {
        await ocr.terminate();
        ocr = undefined;
      }
    }
  };
  const interval = setInterval(() => void tick(), 1500);
  void tick();
  // Foreign receipts imported while offline are converted once rates can be
  // downloaded. Failures (still offline) are silent; the next round retries.
  const convert = async () => {
    if (stopped) return;
    try {
      if (await api.convertPending()) await callbacks.refresh();
    } catch {
      /* retried on the next round */
    }
  };
  const conversions = setInterval(() => void convert(), 120_000);
  void convert();
  return () => {
    stopped = true;
    clearInterval(interval);
    clearInterval(conversions);
    if (!current && ocr) void ocr.terminate();
  };
}
