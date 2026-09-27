import { cp, mkdir, readdir, copyFile } from 'node:fs/promises';
import { resolve } from 'node:path';
const out = resolve('public/ocr');
await mkdir(out, { recursive: true });
await copyFile('node_modules/tesseract.js/dist/worker.min.js', `${out}/worker.min.js`);
for (const f of await readdir('node_modules/tesseract.js-core')) {
  if (f.endsWith('.wasm') || f.endsWith('.wasm.js'))
    await copyFile(`node_modules/tesseract.js-core/${f}`, `${out}/${f}`);
}
await cp(
  'node_modules/@tesseract.js-data/eng/4.0.0_best_int/eng.traineddata.gz',
  `${out}/eng.traineddata.gz`,
);
await mkdir('public/pdf', { recursive: true });
await cp('node_modules/pdfjs-dist/cmaps', 'public/pdf/cmaps', { recursive: true });
await cp('node_modules/pdfjs-dist/standard_fonts', 'public/pdf/standard_fonts', {
  recursive: true,
});
await cp('node_modules/pdfjs-dist/wasm', 'public/pdf/wasm', { recursive: true });
console.log('Offline OCR and PDF assets are ready. No runtime download is needed.');

await mkdir('public/fonts', { recursive: true });
await copyFile(
  'node_modules/@fontsource/noto-sans/files/noto-sans-latin-400-normal.woff',
  'public/fonts/noto-sans-latin-400-normal.woff',
);
await mkdir('public/licenses', { recursive: true });
for (const [source, destination] of [
  ['node_modules/tesseract.js/LICENSE.md', 'tesseract-js.txt'],
  ['node_modules/tesseract.js/dist/worker.min.js.LICENSE.txt', 'tesseract-worker.txt'],
  ['node_modules/tesseract.js-core/LICENSE', 'tesseract-core.txt'],
  ['node_modules/@fontsource/noto-sans/LICENSE', 'noto-sans.txt'],
  ['node_modules/pdfjs-dist/LICENSE', 'pdfjs.txt'],
  ['node_modules/pdf-lib/LICENSE.md', 'pdf-lib.txt'],
])
  await copyFile(source, `public/licenses/${destination}`);
