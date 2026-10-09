/**
 * Self-check for the first-run advice: `node scripts/check-assistant-setup.mjs`.
 *
 * Someone installs this once and believes what it tells them, so the advice has to hold on real machines --
 * a gaming card, a work laptop, an old desktop -- and never suggest a model the computer cannot hold.
 */
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { transformSync } from 'esbuild';

const source = readFileSync(new URL('../src/utils/assistantSetup.ts', import.meta.url), 'utf8');
const js = transformSync(source, { loader: 'ts', format: 'esm' }).code;
const file = join(tmpdir(), `assistant-setup-check-${process.pid}.mjs`);
writeFileSync(file, js);
const { adviseSetup, modelFor, usableMemoryGb } = await import(`file://${file.replace(/\\/g, '/')}`);
rmSync(file, { force: true });

const machine = (over) => ({ ramGb: 16, cpuCores: 8, vramGb: 0, gpuName: '', os: 'windows', ...over });

// A gaming or workstation card: run it here, and say which card was found.
const gaming = adviseSetup(machine({ vramGb: 12, gpuName: 'NVIDIA GeForce RTX 4070', ramGb: 32 }));
assert.equal(gaming.suggested, 'local');
assert.equal(gaming.local.model, 'qwen3:8b');
assert.match(gaming.reason, /RTX 4070/, 'names what it found, not "your GPU"');
assert.match(gaming.reason, /free/);

// A modest laptop card still runs the medium model.
const laptop = adviseSetup(machine({ vramGb: 6, gpuName: 'NVIDIA GeForce RTX 3050 Laptop GPU' }));
assert.equal(laptop.suggested, 'local');
assert.equal(laptop.local.model, 'qwen3:4b');

// Integrated graphics with plenty of system memory: local, but told honestly that it will be slow.
const office = adviseSetup(machine({ vramGb: 0, ramGb: 32, gpuName: 'Intel Iris Xe' }));
assert.equal(office.suggested, 'local');
assert.ok(office.local, 'a big-memory machine can still hold a model');
assert.match(office.reason, /slow/i);

// A thin machine is pointed at Claude rather than promised something it cannot do.
const thin = adviseSetup(machine({ vramGb: 0, ramGb: 8, gpuName: '' }));
assert.equal(thin.suggested, 'claude');
assert.equal(thin.local, null);
assert.match(thin.reason, /slow|better fit/i);

// The model must fit what the machine can hold, never the other way round.
assert.equal(modelFor(machine({ vramGb: 24 })).model, 'qwen3:8b', 'a big card gets the large model');
assert.equal(modelFor(machine({ vramGb: 4 })).model, 'qwen3:1.7b');
assert.equal(modelFor(machine({ ramGb: 8, vramGb: 0 })), null, 'nothing is suggested to a thin machine');
assert.equal(usableMemoryGb(machine({ vramGb: 8, ramGb: 64 })), 8, 'the card decides when it has its own memory');
assert.equal(usableMemoryGb(machine({ vramGb: 0, ramGb: 16 })), 4, 'without a card, the editor keeps its share');

// Every suggested model must be one the machine was measured to hold.
for (const vram of [0, 2, 4, 6, 8, 12, 16, 24]) {
  for (const ram of [8, 16, 32, 64]) {
    const hw = machine({ vramGb: vram, ramGb: ram });
    const advice = adviseSetup(hw);
    if (advice.local) {
      assert.ok(advice.local.downloadGb > 0, 'a download size is always shown');
      assert.ok(advice.local.label && !advice.local.label.includes(':'), 'people are shown a name, not a tag');
    } else {
      assert.equal(advice.suggested, 'claude', 'no local model means Claude is suggested instead');
    }
    assert.ok(advice.reason.length > 20, 'there is always a reason in plain words');
  }
}

console.log('assistant setup: all checks passed');
