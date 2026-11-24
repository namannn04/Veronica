import assert from 'node:assert/strict';
import { test } from 'node:test';

import { cpuPercent, memoryPercent, parseCpuTicks } from './procStats.js';

const STAT = `cpu  1000 20 300 8000 100 0 40 0 0 0
cpu0 500 10 150 4000 50 0 20 0 0 0
intr 12345
ctxt 98765
`;

test('the aggregate cpu line is used, not the first core', () => {
    const ticks = parseCpuTicks(STAT);
    assert.equal(ticks.total, 9460);
    assert.equal(ticks.idle, 8100);
});

test('busy share is the non-idle part of the delta', () => {
    const before = { total: 1000, idle: 900 };
    const after = { total: 1100, idle: 950 };
    assert.equal(cpuPercent(before, after), 50);
});

test('a fully idle interval reads as zero, a fully busy one as a hundred', () => {
    assert.equal(cpuPercent({ total: 0, idle: 0 }, { total: 100, idle: 100 }), 0);
    assert.equal(cpuPercent({ total: 0, idle: 0 }, { total: 100, idle: 0 }), 100);
});

test('a stalled or rewound counter reports nothing rather than zero', () => {
    assert.equal(cpuPercent({ total: 100, idle: 50 }, { total: 100, idle: 50 }), null);
    assert.equal(cpuPercent({ total: 500, idle: 400 }, { total: 100, idle: 50 }), null);
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
    const tricky = `MemTotalHuge:   99 kB
MemTotal:       1000 kB
MemAvailableFoo: 1 kB
MemAvailable:    400 kB
`;
    assert.equal(memoryPercent(tricky), 60);
});

test('percentages are clamped into range', () => {
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

test('guest time is not counted twice', () => {
    const ticks = parseCpuTicks('cpu  100 0 0 300 0 0 0 0 60 0\n');
    assert.equal(ticks.total, 400);
    assert.equal(ticks.idle, 300);
});
