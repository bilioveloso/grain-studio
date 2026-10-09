/**
 * Self-check for the circled-area fallback: `node scripts/check-mask-fallback.mjs`.
 *
 * The decision of whether to trust the model's cut or keep the shape someone drew is the one piece of real
 * logic here, and getting it wrong is what produced a mask on the sky instead of the crane. esbuild is already
 * a dependency, so the module is transformed and run rather than pulling in a framework for one file.
 */
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { transformSync } from 'esbuild';

const source = readFileSync(new URL('../src/utils/maskUtils.ts', import.meta.url), 'utf8');
// The file imports React-land modules it does not need for this; only the pure decision is under test.
const pure = source.slice(source.indexOf('export const DRAWN_SHAPE_CEILING'));
const js = transformSync(pure, { loader: 'ts', format: 'esm' }).code;
const file = join(tmpdir(), `mask-fallback-check-${process.pid}.mjs`);
writeFileSync(file, js);
const { shouldKeepDrawnShape, DRAWN_SHAPE_CEILING, DRAWN_SHAPE_MIN_REACH } = await import(
  `file://${file.replace(/\\/g, '/')}`
);
rmSync(file, { force: true });

// The real miss this was written for: a glint on open water. Small, and it sits in one corner of the circle --
// measured at coverage 0.044, reach 0.35. Keep what was drawn.
assert.ok(shouldKeepDrawnShape({ coverage: 0.044, reach: 0.35 }), 'a glint on the water is not an answer');
assert.ok(shouldKeepDrawnShape({ coverage: 0, reach: 0 }), 'an empty cut is not an answer');

// The whole box: no edges found, so intersecting gives the circle back with a softer edge. Keep what was drawn.
assert.ok(shouldKeepDrawnShape({ coverage: 1, reach: 1 }), 'the whole box is not an answer either');
assert.ok(shouldKeepDrawnShape({ coverage: 0.99, reach: 1 }), 'nearly the whole box is not an answer either');

// A crane is a lattice: it fills maybe a third of the circle thrown around it but runs the whole height of it.
// Trusting this is the entire feature.
assert.ok(!shouldKeepDrawnShape({ coverage: 0.33, reach: 0.92 }), 'a crane-shaped cut must be trusted');

// And the case area alone would get wrong: a mast, a wire, a railing. Barely any area, but it crosses the
// circle end to end, so it is the thing that was circled.
assert.ok(
  !shouldKeepDrawnShape({ coverage: 0.05, reach: 0.95 }),
  'a thin subject spanning the circle must be trusted, not mistaken for a speck',
);

// A failed measurement must not silently trust a cut nobody could measure.
assert.ok(shouldKeepDrawnShape({ coverage: NaN, reach: NaN }), 'an unmeasurable cut falls back');
assert.ok(shouldKeepDrawnShape({ coverage: 0.4, reach: Infinity }), 'a nonsense reach falls back');

assert.ok(DRAWN_SHAPE_CEILING < 1 && DRAWN_SHAPE_MIN_REACH > 0, 'thresholds leave a usable middle');

console.log('mask fallback: all checks passed');
