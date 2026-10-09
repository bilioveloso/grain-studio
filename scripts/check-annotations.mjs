/**
 * Self-check for the marking geometry: `node scripts/check-annotations.mjs`.
 *
 * The repo has no test runner, and a circled area is the one part of marking with real logic in it -- which
 * points count as inside a ring, and what the assistant is told about them. esbuild is already a dependency,
 * so the module is transformed and run here rather than pulling in a framework for one file.
 */
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { transformSync } from 'esbuild';

const source = readFileSync(new URL('../src/utils/annotations.ts', import.meta.url), 'utf8');
const js = transformSync(source, { loader: 'ts', format: 'esm' }).code;
const file = join(tmpdir(), `annotations-check-${process.pid}.mjs`);
writeFileSync(file, js);
const { newPin, newRing, ringBox, padBox, inRing, interiorPoints, composeAnnotatedMessage } =
  await import(
  `file://${file.replace(/\\/g, '/')}`
);
rmSync(file, { force: true });

// A square ring around the middle of the frame.
const square = [
  { x: 0.2, y: 0.2 },
  { x: 0.6, y: 0.2 },
  { x: 0.6, y: 0.8 },
  { x: 0.2, y: 0.8 },
];

assert.deepEqual(ringBox(square), [0.2, 0.2, 0.6, 0.8], 'box hugs the drawn ring');
assert.ok(inRing(square, { x: 0.4, y: 0.5 }), 'a point in the middle is inside');
assert.ok(!inRing(square, { x: 0.9, y: 0.5 }), 'a point beyond the ring is outside');
assert.ok(!inRing(square, { x: 0.4, y: 0.05 }), 'a point above the ring is outside');

// Seeds must land inside the shape, and spread rather than bunch on one row.
const seeds = interiorPoints(square);
assert.ok(seeds.length > 1 && seeds.length <= 7, `expected a few seeds, got ${seeds.length}`);
assert.ok(
  seeds.every((p) => inRing(square, p)),
  'every seed point is inside the ring',
);
assert.ok(new Set(seeds.map((p) => p.y.toFixed(3))).size > 1, 'seeds spread over more than one row');

// A C shape: the notch is outside the ring even though it is inside the box.
const cShape = [
  { x: 0.1, y: 0.1 },
  { x: 0.9, y: 0.1 },
  { x: 0.9, y: 0.3 },
  { x: 0.3, y: 0.3 },
  { x: 0.3, y: 0.7 },
  { x: 0.9, y: 0.7 },
  { x: 0.9, y: 0.9 },
  { x: 0.1, y: 0.9 },
];
assert.ok(!inRing(cShape, { x: 0.6, y: 0.5 }), 'the notch of a C is not inside it');
assert.ok(
  interiorPoints(cShape).every((p) => inRing(cShape, p)),
  'seeds avoid the notch',
);

// A ring too thin to catch a grid point still yields something usable.
const sliver = [
  { x: 0.5, y: 0.5 },
  { x: 0.5001, y: 0.5 },
  { x: 0.5001, y: 0.5001 },
];
assert.equal(interiorPoints(sliver).length, 1, 'a sliver falls back to one point');

// What the assistant is told.
const ring = { ...newRing(square), note: 'make this darker' };
const message = composeAnnotatedMessage('', [ring]);
assert.match(message, /I circled an area/);
assert.match(message, /box \[0\.200, 0\.200, 0\.600, 0\.800\]/);
assert.match(message, /make this darker/);
assert.match(message, /Leave every other part of the photo untouched/);

// When the app has already built the mask from the drawn shape, the assistant is told to use it, not re-cut it.
const ready = composeAnnotatedMessage('', [ring], { [ring.id]: 'abc123' });
assert.match(ready, /already a mask \(id abc123\)/);
assert.match(ready, /do not cut them again/);
assert.match(ready, /update_mask/);
assert.doesNotMatch(ready, /create_smart_mask/, 'a ready-made mask needs no model to find it');
assert.doesNotMatch(ready, /box \[/, 'no need to describe a box the app already turned into a mask');

const pin = { ...newPin(0.25, 0.75), note: 'warm this up' };
assert.match(composeAnnotatedMessage('', [pin]), /1\. \(0\.250, 0\.750\) — warm this up/);

// Free text with no marks goes through untouched; a mark with no note borrows the typed text.
assert.equal(composeAnnotatedMessage('  just brighten it  ', []), 'just brighten it');
assert.match(composeAnnotatedMessage('darker here', [newPin(0.5, 0.5)]), /darker here/);

// The seed box is loosened past the ring, so the mask model ends on the subject's edge rather than cutting a
// straight line down the ring; it never leaves the frame.
const [px1, py1, px2, py2] = padBox([0.2, 0.2, 0.6, 0.8]);
assert.ok(px1 < 0.2 && py1 < 0.2 && px2 > 0.6 && py2 > 0.8, 'the seed box is looser than the drawn ring');
assert.deepEqual(padBox([0, 0, 1, 1]), [0, 0, 1, 1], 'a full-frame ring cannot be padded off the frame');
assert.deepEqual(padBox([0.2, 0.2, 0.6, 0.8], 0), [0.2, 0.2, 0.6, 0.8], 'no padding means the ring itself');

// A loose circle round a tall thin subject is mostly background. The seeds that say "this" come from the middle
// outwards, and the seeds that say "not this" sit just outside the drawn line.
const tall = [
  { x: 0.3, y: 0.1 },
  { x: 0.7, y: 0.1 },
  { x: 0.7, y: 0.9 },
  { x: 0.3, y: 0.9 },
];
const inside = interiorPoints(tall, 5);
assert.ok(inside.every((p) => inRing(tall, p)), 'every include seed is inside the ring');
assert.ok(
  inside.filter((p) => Math.abs(p.x - 0.5) < 1e-9).length >= inside.length - 2,
  'a ring taller than it is wide is walked down its middle, not spread across the background',
);
assert.ok(
  inside.some((p) => Math.abs(p.x - 0.5) > 1e-9),
  'and it is given some width, so the model reads a shape rather than a line',
);
assert.ok(
  Math.max(...inside.map((p) => p.y)) - Math.min(...inside.map((p) => p.y)) > 0.4,
  'the seeds straddle the subject end to end, not a cluster at its centre',
);
// A wide ring is walked the other way.
const wide = tall.map((p) => ({ x: p.y, y: p.x }));
const acrossSeeds = interiorPoints(wide, 5);
assert.ok(
  acrossSeeds.filter((p) => Math.abs(p.y - 0.5) < 1e-9).length >= acrossSeeds.length - 2,
  'a ring wider than it is tall is walked across its middle',
);

console.log('annotations: all checks passed');
