/**
 * Self-check for the plain-edit table: `node scripts/check-verb-edits.mjs`.
 *
 * The risk here is not a missed match -- that just sends the request to the assistant, as before. The risk is a
 * wrong match, which edits the photo without anyone reading the sentence. So most of this file is about what
 * must NOT match.
 */
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { transformSync } from 'esbuild';

const source = readFileSync(new URL('../src/utils/verbEdits.ts', import.meta.url), 'utf8');
const js = transformSync(source, { loader: 'ts', format: 'esm' }).code;
const file = join(tmpdir(), `verb-edits-check-${process.pid}.mjs`);
writeFileSync(file, js);
const { parseVerbEdit, applyVerbEdit } = await import(`file://${file.replace(/\\/g, '/')}`);
rmSync(file, { force: true });

// --- what the table handles ---
for (const phrase of [
  'darker',
  'make this darker',
  'Make it darker.',
  'please make this area darker',
  'brighten',
  'warmer',
  'make the sky warmer'.replace('the sky ', ''), // a bare verb with filler around it
  'punchier',
  'more contrast',
  'less saturated',
  'sharper',
  'lift the shadows',
  'tame the highlights',
]) {
  const edit = parseVerbEdit(phrase);
  assert.ok(edit, `should be handled without the assistant: ${phrase}`);
  assert.ok(Object.keys(edit.deltas).length > 0, `${phrase} moves something`);
}

// --- what must go to the assistant instead ---
for (const phrase of [
  'make this darker but keep the sky as it is', // a condition
  'darker, and warmer', // two verbs
  'make it cinematic',
  'match the other photo',
  'darker like the reference',
  'make the crane read as a silhouette',
  'remove the person on the left',
  'why is this so noisy?',
  'crop this square',
  'darken only the water', // "only" narrows it, and the table cannot narrow
  '',
  'hmm',
]) {
  assert.equal(parseVerbEdit(phrase), null, `should reach the assistant: ${phrase}`);
}

// --- how hard the nudge lands ---
const plain = parseVerbEdit('darker');
const gentle = parseVerbEdit('slightly darker');
const strong = parseVerbEdit('much darker');
assert.ok(Math.abs(gentle.deltas.exposure) < Math.abs(plain.deltas.exposure));
assert.ok(Math.abs(strong.deltas.exposure) > Math.abs(plain.deltas.exposure));
assert.match(gentle.label, /slightly/);
assert.match(strong.label, /much/);
assert.equal(parseVerbEdit('a touch warmer').deltas.temperature, 6);

// --- applying it ---
const after = applyVerbEdit({ exposure: 0, blacks: 0, contrast: 5 }, plain.deltas);
assert.equal(after.exposure, -0.35);
assert.equal(after.blacks, -6);
assert.equal(after.contrast, 5, 'what the verb does not touch is left alone');

// Repeated nudges add up, and stop at the end of the slider rather than running past it.
let stacked = { exposure: 0 };
for (let i = 0; i < 40; i++) stacked = applyVerbEdit(stacked, { exposure: -0.35 });
assert.equal(stacked.exposure, -5, 'exposure stops at the bottom of its range');
assert.equal(applyVerbEdit({ sharpness: 0 }, { sharpness: -20 }).sharpness, 0, 'sharpness has no negative side');

// A missing value counts as zero, since a fresh mask has nothing set yet.
assert.equal(applyVerbEdit({}, { exposure: 0.35 }).exposure, 0.35);

console.log('verb edits: all checks passed');
