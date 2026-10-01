import { describe, it, expect } from 'vitest';
import { enhanceForOcr, ocrScale } from './receiptRendering';
function pixel(r: number, g: number, b: number, a = 255): Uint8ClampedArray {
  return new Uint8ClampedArray([r, g, b, a]);
}
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
