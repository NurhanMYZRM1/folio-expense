import { describe, it, expect } from 'vitest';
import {
  enhanceForOcr,
  lightingUnevenness,
  ocrScale,
  prepareForOcr,
  UNEVEN_LIGHTING,
} from './receiptRendering';
function pixel(r: number, g: number, b: number, a = 255): Uint8ClampedArray {
  return new Uint8ClampedArray([r, g, b, a]);
}
/**
 * A 600×800 gray "receipt": paper at `paper(x, y)` with dark text-like bars
 * (ink at 35% of the local paper brightness) every 40 px.
 */
function receipt(paper: (x: number, y: number) => number) {
  const width = 600,
    height = 800;
  const data = new Uint8ClampedArray(width * height * 4);
  const isInk = (x: number, y: number) => x >= 60 && x < 540 && y % 40 >= 14 && y % 40 < 24;
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      const p = paper(x, y);
      const v = isInk(x, y) ? p * 0.35 : p;
      const o = (y * width + x) * 4;
      data[o] = data[o + 1] = data[o + 2] = v;
      data[o + 3] = 255;
    }
  const at = (x: number, y: number) => data[(y * width + x) * 4];
  return { data, width, height, at, isInk };
}
// The lower-right quadrant sits in a hand shadow: paper there is 1/3 as bright.
const shadowed = (x: number, y: number) => (x > 300 && y > 400 ? 78 : 235);
// Each case filters a full receipt-sized image; on a slow CI runner the first
// one (which also warms up the JIT) can take several seconds.
describe('prepareForOcr', { timeout: 30_000 }, () => {
  it('keeps the contrast stretch for evenly lit scans, PDFs and screenshots', () => {
    const even = receipt(() => 235);
    const reference = Uint8ClampedArray.from(even.data);
    enhanceForOcr(reference);
    expect(prepareForOcr(even.data, even.width, even.height)).toBe('standard');
    expect(even.data).toEqual(reference);
  });
  it('flattens a shadow so shaded paper is as white as lit paper', () => {
    const r = receipt(shadowed);
    expect(prepareForOcr(r.data, r.width, r.height)).toBe('flattened');
    const litPaper = r.at(100, 30),
      shadePaper = r.at(450, 630);
    expect(litPaper).toBeGreaterThan(240);
    expect(shadePaper).toBeGreaterThan(240);
    // Ink stays dark (and readable) on both sides of the shadow edge.
    expect(r.isInk(100, 135)).toBe(true);
    expect(r.at(100, 135)).toBeLessThan(110);
    expect(r.isInk(450, 655)).toBe(true);
    expect(r.at(450, 655)).toBeLessThan(110);
    // Before the fix the shaded paper (78) was darker than lit ink (82).
    expect(shadePaper - r.at(450, 655)).toBeGreaterThan(130);
    // Paper right beside the shadow edge stays clearly lighter than ink: a
    // background that bleeds across the edge leaves a dark ring OCR reads as
    // ink (a hard-edged shadow still softens slightly, so not fully white).
    expect(r.at(290, 630)).toBeGreaterThan(200);
    expect(r.at(312, 630)).toBeGreaterThan(200);
  });
  it('joins dot-matrix dots into solid strokes', () => {
    // A row of 1 px dots, 3 px apart, inside the shadow.
    const r = receipt(shadowed);
    const dot = (x: number, y: number) => x % 3 === 0 && y >= 600 && y < 606;
    for (let y = 596; y < 610; y++)
      for (let x = 360; x < 520; x++) {
        const o = (y * r.width + x) * 4;
        r.data[o] = r.data[o + 1] = r.data[o + 2] = dot(x, y) ? 27 : 78;
      }
    prepareForOcr(r.data, r.width, r.height);
    // The gaps between dots are now ink too.
    expect(r.at(400, 603)).toBeLessThan(140);
    expect(r.at(401, 603)).toBeLessThan(140);
  });
  it('forces alpha to 255 and leaves pixels gray', () => {
    const r = receipt(shadowed);
    for (let i = 3; i < r.data.length; i += 4) r.data[i] = 7;
    prepareForOcr(r.data, r.width, r.height);
    let opaqueGray = true;
    for (let i = 0; i < r.data.length; i += 4)
      opaqueGray &&= r.data[i + 3] === 255 && r.data[i + 1] === r.data[i];
    expect(opaqueGray).toBe(true);
  });
});
describe('lightingUnevenness', () => {
  it('is 0 for even paper and large for a deep shadow', () => {
    expect(lightingUnevenness(new Float32Array(100).fill(230))).toBe(0);
    const half = new Float32Array(100).fill(230).fill(80, 50);
    expect(lightingUnevenness(half)).toBeGreaterThan(UNEVEN_LIGHTING);
    // A gentle vignette (10% falloff) is not a shadow.
    const vignette = Float32Array.from({ length: 100 }, (_, i) => 230 - i * 0.23);
    expect(lightingUnevenness(vignette)).toBeLessThan(UNEVEN_LIGHTING);
  });
});
describe('enhanceForOcr', () => {
  it('converts to grayscale using Rec. 601 luma and forces alpha to 255', () => {
    const white = pixel(255, 255, 255, 10);
    enhanceForOcr(white);
    expect([...white]).toEqual([255, 255, 255, 255]);
    const black = pixel(0, 0, 0);
    enhanceForOcr(black);
    expect([...black]).toEqual([0, 0, 0, 255]);
    const red = pixel(255, 0, 0);
    enhanceForOcr(red);
    expect([...red]).toEqual([76, 76, 76, 255]);
    const green = pixel(0, 255, 0);
    enhanceForOcr(green);
    expect([...green]).toEqual([150, 150, 150, 255]);
    const blue = pixel(0, 0, 255);
    enhanceForOcr(blue);
    expect([...blue]).toEqual([29, 29, 29, 255]);
  });
  it('stretches the 2nd/98th percentile of a low-contrast gradient to 0/255', () => {
    // 100 already-gray pixels, one at each level 0..99: a low-contrast gradient.
    const data = new Uint8ClampedArray(400);
    for (let i = 0; i < 100; i++) {
      const o = i * 4;
      data[o] = data[o + 1] = data[o + 2] = i;
      data[o + 3] = 255;
    }
    enhanceForOcr(data);
    const at = (i: number) => data[i * 4];
    // low percentile (level 1) clamps to 0, everything below it clamps to 0 too.
    expect(at(0)).toBe(0);
    expect(at(1)).toBe(0);
    // high percentile (level 97) maps to 255, and anything above clamps at 255.
    expect(at(97)).toBe(255);
    expect(at(99)).toBe(255);
    // a mid-range value is stretched proportionally.
    expect(at(50)).toBe(130);
    for (let i = 0; i < 100; i++) expect(data[i * 4 + 3]).toBe(255);
  });
  it('skips stretching a flat/near-flat image to avoid amplifying noise', () => {
    const flat = new Uint8ClampedArray([120, 120, 120, 0, 120, 120, 120, 0]);
    enhanceForOcr(flat);
    expect([...flat]).toEqual([120, 120, 120, 255, 120, 120, 120, 255]);
    // range of 10 (< 16) still counts as flat and is left unstretched.
    const nearFlat = new Uint8ClampedArray([100, 100, 100, 255, 110, 110, 110, 255]);
    enhanceForOcr(nearFlat);
    expect([...nearFlat]).toEqual([100, 100, 100, 255, 110, 110, 110, 255]);
  });
});
describe('ocrScale', () => {
  it('enlarges small receipts up to 2× and leaves large ones alone', () => {
    expect(ocrScale(400, 600)).toBe(2);
    expect(ocrScale(850, 1050)).toBeCloseTo(1600 / 1050);
    expect(ocrScale(1700, 2200)).toBe(1);
    expect(ocrScale(0, 0)).toBe(1);
  });
});
