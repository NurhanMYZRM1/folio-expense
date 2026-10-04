import { describe, expect, it } from 'vitest';
import { importSequentially, MAX_IMPORT } from './importQueue';
import type { ImportOutcome } from '../bindings/generated';

const ok = (filename: string): ImportOutcome => ({
  filename,
  expenseId: `id-${filename}`,
  error: null,
});

describe('importSequentially', () => {
  it('imports one file at a time and reports progress after each', async () => {
    const calls: string[][] = [];
    const progress: [number, number][] = [];
    const result = await importSequentially(
      ['a.png', 'b.png', 'c.pdf'],
      async (paths) => {
        calls.push(paths);
        return paths.map(ok);
      },
      (done, total) => progress.push([done, total]),
    );
    expect(calls).toEqual([['a.png'], ['b.png'], ['c.pdf']]);
    expect(progress).toEqual([
      [0, 3],
      [1, 3],
      [2, 3],
      [3, 3],
    ]);
    expect(result.map((r) => r.filename)).toEqual(['a.png', 'b.png', 'c.pdf']);
  });

  it('keeps going after a per-file failure and records it as an outcome', async () => {
    const result = await importSequentially(
      ['a.png', 'bad.png', 'c.png'],
      async ([p]) => {
        if (p === 'bad.png') throw { code: 'Io', message: 'Could not read file' };
        return [ok(p)];
      },
      () => {},
    );
    expect(result.map((r) => r.expenseId)).toEqual(['id-a.png', null, 'id-c.png']);
    expect(result[1]).toEqual({
      filename: 'bad.png',
      expenseId: null,
      error: { code: 'Io', message: 'Could not read file', existingExpenseId: null },
    });
  });

  it('uses the basename for failures on Windows and POSIX paths', async () => {
    const result = await importSequentially(
      ['C:\\Users\\me\\r1.jpg', '/tmp/r2.jpg'],
      async () => {
        throw new Error('boom');
      },
      () => {},
    );
    expect(result.map((r) => r.filename)).toEqual(['r1.jpg', 'r2.jpg']);
  });

  it('rejects batches above the backend limit before importing anything', async () => {
    let called = false;
    const paths = Array.from({ length: MAX_IMPORT + 1 }, (_, i) => `${i}.png`);
    await expect(
      importSequentially(
        paths,
        async () => {
          called = true;
          return [];
        },
        () => {},
      ),
    ).rejects.toMatchObject({ message: 'Import up to 100 receipts at a time.' });
    expect(called).toBe(false);
  });
});
