import type { ImportOutcome } from '../bindings/generated';
import { errorMessage } from './errors';

/** Mirrors the backend limit in `ReceiptService::import_receipts`. */
export const MAX_IMPORT = 100;

function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/**
 * Import receipts one file per IPC call so the UI can show real progress.
 * The backend still stages, hashes, and de-duplicates each file; a failure on
 * one file is recorded as that file's outcome and the rest continue.
 */
export async function importSequentially(
  paths: string[],
  importBatch: (paths: string[]) => Promise<ImportOutcome[]>,
  onProgress: (done: number, total: number) => void,
): Promise<ImportOutcome[]> {
  if (paths.length > MAX_IMPORT)
    throw { code: 'InvalidInput', message: `Import up to ${MAX_IMPORT} receipts at a time.` };
  const outcomes: ImportOutcome[] = [];
  onProgress(0, paths.length);
  for (const [index, path] of paths.entries()) {
    try {
      outcomes.push(...(await importBatch([path])));
    } catch (e) {
      outcomes.push({
        filename: basename(path),
        expenseId: null,
        error: {
          code:
            typeof e === 'object' && e !== null && 'code' in e && typeof e.code === 'string'
              ? e.code
              : 'ImportFailed',
          message: errorMessage(e),
          existingExpenseId: null,
        },
      });
    }
    onProgress(index + 1, paths.length);
  }
  return outcomes;
}
