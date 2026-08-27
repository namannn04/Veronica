/* Tests for the top bar's procfs arithmetic.
 *
 * Run with `node --test extension/`. The module under test imports nothing from
 * `gi://`, which is the whole reason it is a separate file from the widget.
 */

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { cpuPercent, memoryPercent, parseCpuTicks } from './procStats.js';

/** A real /proc/stat, trimmed to the lines that matter. */
const STAT = `cpu  1000 20 300 8000 100 0 40 0 0 0
cpu0 500 10 150 4000 50 0 20 0 0 0
intr 12345
ctxt 98765
`;

test('the aggregate cpu line is used, not the first core', () => {
    const ticks = parseCpuTicks(STAT);
    // 1000+20+300+8000+100+0+40 = 9460 across every field present.
    assert.equal(ticks.total, 9460);
    // idle + iowait.
    assert.equal(ticks.idle, 8100);
});

test('busy share is the non-idle part of the delta', () => {
    const before = { total: 1000, idle: 900 };
    const after = { total: 1100, idle: 950 };
    // 100 jiffies passed, 50 of them idle.
    assert.equal(cpuPercent(before, after), 50);
});

test('a fully idle interval reads as zero, a fully busy one as a hundred', () => {
    assert.equal(cpuPercent({ total: 0, idle: 0 }, { total: 100, idle: 100 }), 0);
    assert.equal(cpuPercent({ total: 0, idle: 0 }, { total: 100, idle: 0 }), 100);
});

test('a stalled or rewound counter reports nothing rather than zero', () => {
    // Two identical readings: no time passed, so there is no load to report.
    // Zero here would render an idle machine.
    assert.equal(cpuPercent({ total: 100, idle: 50 }, { total: 100, idle: 50 }), null);
    // Counters reset across a suspend/resume.
    assert.equal(cpuPercent({ total: 500, idle: 400 }, { total: 100, idle: 50 }), null);
    // Idle going backwards while total rises is equally incoherent.
    assert.equal(cpuPercent({ total: 100, idle: 90 }, { total: 200, idle: 80 }), null);
});

test('a missing reading is not a reading', () => {
    assert.equal(cpuPercent(null, { total: 1, idle: 0 }), null);
    assert.equal(cpuPercent({ total: 1, idle: 0 }, null), null);
});

test('a stat file without an aggregate line or with junk yields nothing', () => {
    assert.equal(parseCpuTicks(''), null);
    assert.equal(parseCpuTicks(null), null);
    assert.equal(parseCpuTicks('cpu0 1 2 3 4 5\nintr 9'), null, 'cpu0 is not the aggregate');
    assert.equal(parseCpuTicks('cpu  1 2 3'), null, 'too few fields to find idle');
    assert.equal(parseCpuTicks('cpu  a b c d e'), null, 'non-numeric fields');
});

test('memory uses MemAvailable against MemTotal', () => {
    const meminfo = `MemTotal:       16000000 kB
MemFree:          500000 kB
MemAvailable:    8000000 kB
Buffers:          200000 kB
`;
    // Half of it is available, so half is used — not the 97% MemFree implies.
    assert.equal(memoryPercent(meminfo), 50);
});

test('MemFree is the fallback only when MemAvailable is absent', () => {
    const old = `MemTotal:       1000 kB
MemFree:         250 kB
`;
    assert.equal(memoryPercent(old), 75);
});

test('a meminfo without a usable total yields nothing', () => {
    assert.equal(memoryPercent(''), null);
    assert.equal(memoryPercent(null), null);
    assert.equal(memoryPercent('MemFree: 100 kB'), null, 'no total to divide by');
    assert.equal(memoryPercent('MemTotal: 0 kB\nMemFree: 0 kB'), null, 'a zero total');
    assert.equal(memoryPercent('MemTotal: 1000 kB'), null, 'nothing available reported');
});

test('a prefix does not match a different field', () => {
    // "MemTotal" must not be satisfied by "MemTotalHuge", nor MemAvailable by a
    // similarly named field, or the percentage would be of the wrong quantity.
    const tricky = `MemTotalHuge:   99 kB
MemTotal:       1000 kB
MemAvailableFoo: 1 kB
MemAvailable:    400 kB
`;
    assert.equal(memoryPercent(tricky), 60);
});

test('percentages are clamped into range', () => {
    // A kernel reporting more available than total would otherwise go negative.
    assert.equal(memoryPercent('MemTotal: 100 kB\nMemAvailable: 200 kB'), 0);
});

test('the real files on this machine parse', async () => {
    const { readFile } = await import('node:fs/promises');
    const [stat, meminfo] = await Promise.all([
        readFile('/proc/stat', 'utf8'),
        readFile('/proc/meminfo', 'utf8'),
    ]);
    const ticks = parseCpuTicks(stat);
    assert.ok(ticks && ticks.total > 0, 'no cpu ticks read');
    assert.ok(ticks.idle > 0 && ticks.idle <= ticks.total);
    const memory = memoryPercent(meminfo);
    assert.ok(memory !== null && memory > 0 && memory < 100, `memory was ${memory}`);
});
