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
const MAX_PARALLEL = 3;
type Slot = { busy: boolean; ocr?: OcrWorker; progress: string | null };
export function startJobRunner(callbacks: Callbacks): () => void {
  let stopped = false,
    pumping = false,
    pumpAgain = false,
    onlineUnavailableNotified = false,
    pdfQueue: Promise<unknown> = Promise.resolve();
  // Receipts are read a few at a time: online requests mostly wait on the
  // network, and each slot owns its own (lazily created) OCR worker.
  const slots: Slot[] = Array.from({ length: MAX_PARALLEL }, () => ({
    busy: false,
    progress: null,
  }));
  const publish = () => {
    if (stopped) return;
    const lines = slots.map((s) => s.progress).filter((p): p is string => !!p);
    callbacks.progress(
      lines.length > 1 ? `${lines[0]} (+${lines.length - 1} more)` : (lines[0] ?? null),
    );
  };
  const runJob = async (slot: Slot, current: JobLease) => {
    const progress = (text: string | null) => {
      slot.progress = text;
      publish();
    };
    let heartbeat: ReturnType<typeof setInterval> | undefined;
    try {
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
        // Report building is memory-heavy: one at a time even while receipts are read in parallel.
        const buildReport = async () => {
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
        };
        const turn = pdfQueue.then(buildReport, buildReport);
        pdfQueue = turn.catch(() => {});
        await turn;
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
          if (await api.onlineAvailable()) {
            onlineUnavailableNotified = false;
            progress('Trying online AI extraction');
            const images: string[] = [];
            for await (const canvas of receiptPages(receipt)) {
              images.push(preprocessCanvas(canvas).toDataURL('image/jpeg', 0.85));
              canvas.width = canvas.height = 0;
            }
            completed = await api.tryOnline(id, token, images);
          } else if (settings.onlineEnabled && !onlineUnavailableNotified) {
            onlineUnavailableNotified = true;
            callbacks.notify(
              'Online AI is unavailable right now (a recent provider error, or no key), so receipts are read locally. It is retried automatically.',
            );
          }
          if (!completed) {
            if (!settings.offlineOcrEnabled)
              throw new Error(
                'Automatic extraction is unavailable. The receipt was saved locally and can still be entered manually.',
              );
            if (!slot.ocr) {
              const created = await createWorker('eng', OEM.LSTM_ONLY, {
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
              try {
                // The page layout mode is set per pass below.
                await created.setParameters({ preserve_interword_spaces: '1' });
              } catch (error) {
                void created.terminate();
                throw error;
              }
              slot.ocr = created;
            }
            let rawText = '',
              page = 0,
              // Tesseract's 0–100 confidence, averaged over pages by how much
              // text each page holds (a blank back page shouldn't count).
              weightedConfidence = 0,
              textLength = 0;
            const worker = slot.ocr;
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
      await api.failJob(current.job.id, current.token, message).catch(() => {});
      if (slot.ocr) {
        await slot.ocr.terminate();
        slot.ocr = undefined;
      }
      callbacks.notify(message, true);
    } finally {
      if (heartbeat) clearInterval(heartbeat);
      progress(null);
      if (!stopped) await callbacks.refresh();
      else if (slot.ocr) {
        await slot.ocr.terminate();
        slot.ocr = undefined;
      }
    }
  };
  // Hands queued jobs to free slots. Calls that arrive while it is already
  // running are folded into one more pass.
  const pump = async () => {
    if (pumping) {
      pumpAgain = true;
      return;
    }
    pumping = true;
    try {
      do {
        pumpAgain = false;
        for (const slot of slots) {
          if (stopped) return;
          if (slot.busy) continue;
          slot.busy = true;
          let lease: JobLease | null = null;
          try {
            lease = await api.takeJob();
          } catch (error) {
            callbacks.notify(errorMessage(error), true);
          }
          if (!lease) {
            slot.busy = false;
            // Nothing queued and nothing running: free the extra OCR workers' memory.
            if (slots.every((s) => !s.busy))
              for (const extra of slots.slice(1))
                if (extra.ocr) {
                  void extra.ocr.terminate();
                  extra.ocr = undefined;
                }
            break;
          }
          void runJob(slot, lease).finally(() => {
            slot.busy = false;
            void pump();
          });
        }
      } while (pumpAgain && !stopped);
    } finally {
      pumping = false;
    }
  };
  const interval = setInterval(() => void pump(), 1500);
  void pump();
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
    for (const slot of slots) if (!slot.busy && slot.ocr) void slot.ocr.terminate();
  };
}
