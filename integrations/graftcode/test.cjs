const { test } = require('node:test');
const assert = require('node:assert/strict');
const childProcess = require('node:child_process');
const original = childProcess.execFileSync;
let calls = 0;
childProcess.execFileSync = (binary, args, options) => {
  calls++;
  assert.equal(binary, '/trusted/vr');
  assert.equal(args[0], 'workspace');
  assert.ok(!options.shell);
  return JSON.stringify({ project: 'fixture', root: '/private/project', checks: [{ name: 'Port 3000', status: 'warning', detail: 'not listening' }] });
};
const { ProjectDoctor } = require('./index.cjs');
process.env.VERONICA_GRAFT_TOKEN = 'a'.repeat(64);
process.env.VERONICA_CLI = '/trusted/vr';
process.env.VERONICA_PROJECT_ROOT = '/private/project';
test('unauthorized calls never invoke the CLI or expose paths', () => {
  for (const token of [undefined, null, {}, 'wrong', 'b'.repeat(64), 'x'.repeat(100000)]) {
    const result = ProjectDoctor.inspect(token);
    assert.ok(JSON.parse(result).error);
    assert.ok(!result.includes('/private'));
  }
  assert.equal(calls, 0);
});
test('authorized methods project only checks and never return absolute paths', () => {
  const report = JSON.parse(ProjectDoctor.inspect('a'.repeat(64)));
  assert.equal(report.project, 'fixture');
  assert.equal(report.root, undefined);
  const attention = JSON.parse(ProjectDoctor.attention('a'.repeat(64)));
  assert.equal(attention.length, 1);
  assert.equal(calls, 2);
  childProcess.execFileSync = original;
});
