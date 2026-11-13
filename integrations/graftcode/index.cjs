/* Only this deliberate public surface is hosted. Never run project code. */
const { execFileSync } = require('node:child_process');
const { timingSafeEqual } = require('node:crypto');
function authorize(sessionToken) {
  const expected = process.env.VERONICA_GRAFT_TOKEN || '';
  const actual = Buffer.from(typeof sessionToken === 'string' ? sessionToken : ''), wanted = Buffer.from(expected);
  if (expected.length < 32 || actual.length !== wanted.length || !timingSafeEqual(actual, wanted)) throw new Error('Project Doctor requires its session bearer token.');
}
function report(sessionToken) {
  authorize(sessionToken);
  const binary = process.env.VERONICA_CLI;
  const root = process.env.VERONICA_PROJECT_ROOT;
  if (!binary?.startsWith('/') || !root?.startsWith('/')) throw new Error('Start this module with the Veronica launcher.');
  const args = ['workspace', 'doctor', '--root', root, '--json'];
  if (process.env.VERONICA_PROJECT_PORTS) args.push('--ports', process.env.VERONICA_PROJECT_PORTS);
  if (process.env.VERONICA_PROJECT_CONTAINERS) args.push('--containers', process.env.VERONICA_PROJECT_CONTAINERS);
  let value;
  try { value = JSON.parse(execFileSync(binary, args, { encoding: 'utf8', timeout: 12000, maxBuffer: 128 * 1024, stdio: ['ignore', 'pipe', 'pipe'], env: process.env })); }
  catch { throw new Error('Project check failed or timed out. Run Project Doctor locally for details.'); }
  // Absolute paths never leave the receiver. Only the selected project's checks.
  return { project: value.project, checks: value.checks };
}
class ProjectDoctor {
  /** Read-only checks for the one project selected when the Gateway was started. */
  static inspect(sessionToken) { try { return JSON.stringify(report(sessionToken)); } catch { return JSON.stringify({ error: 'Access denied or project check failed. Verify the session token and run Project Doctor locally for details.' }); } }
  /** Names of checks that need attention. Never starts or stops services. */
  static attention(sessionToken) { try { return JSON.stringify(report(sessionToken).checks.filter(check => check.status !== 'pass')); } catch { return JSON.stringify({ error: 'Access denied or project check failed. Verify the session token and run Project Doctor locally for details.' }); } }
}
module.exports = { ProjectDoctor };
