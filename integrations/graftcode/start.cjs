#!/usr/bin/env node
/* Foreground, user-owned optional Gateway. No daemon or auto-download. */
const { spawn, spawnSync } = require('node:child_process');
const { randomBytes } = require('node:crypto');
const { realpathSync, mkdtempSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { resolve, join } = require('node:path');
const args = process.argv.slice(2);
const options = {};
for (let i=0; i<args.length; i+=2) {
  if (!['--root','--ports','--containers','--port','--http-port','--gg','--vr'].includes(args[i]) || !args[i+1] || options[args[i]]) throw new Error('Use --root DIR [--ports 3000,5432] [--containers app-db] [--port 18771] [--http-port 18772] [--gg /path/to/gg] [--vr /path/to/vr]');
  options[args[i]] = args[i+1];
}
const root = realpathSync(options['--root'] || '');
function binary(value, fallback) {
  const result = spawnSync('/usr/bin/which', [value || fallback], { encoding: 'utf8' });
  if (result.status !== 0 || !result.stdout.trim().startsWith('/')) throw new Error(`Install ${fallback} first, or provide its absolute path.`);
  return realpathSync(result.stdout.trim());
}
const vr = binary(options['--vr'], 'vr'), gg = binary(options['--gg'], 'gg');
const port = Number(options['--port'] || 18771), httpPort = Number(options['--http-port'] || 18772);
if (![port,httpPort].every(p => Number.isInteger(p) && p>1024 && p<65536) || port===httpPort) throw new Error('Choose two different unprivileged ports.');
const env = { ...process.env, VERONICA_CLI: vr, VERONICA_PROJECT_ROOT: root,
  VERONICA_PROJECT_PORTS: options['--ports'] || '', VERONICA_PROJECT_CONTAINERS: options['--containers'] || '', VERONICA_GRAFT_TOKEN: randomBytes(32).toString('hex') };
const preflight = spawnSync(vr,['workspace','doctor','--root',root,...(env.VERONICA_PROJECT_PORTS ? ['--ports',env.VERONICA_PROJECT_PORTS] : []),...(env.VERONICA_PROJECT_CONTAINERS ? ['--containers',env.VERONICA_PROJECT_CONTAINERS] : []),'--json'], { env, encoding: 'utf8', timeout: 15000, maxBuffer: 128*1024 });
if (preflight.status !== 0) throw new Error('Project preflight failed: '+(preflight.stderr || preflight.error?.message || 'unknown error').slice(0,512));
const directory = mkdtempSync(join(tmpdir(),'veronica-graft-'));
console.log(`Gateway endpoint: http://127.0.0.1:${httpPort}/mcp\nSession access token (keep private): ${env.VERONICA_GRAFT_TOKEN}\nKeep this terminal open. Ctrl+C stops the Gateway. Metadata upload is disabled.\nGateway may bind all interfaces; exposed doctor methods require the sessionToken argument. Use trusted local projects only.`);
const child = spawn(gg,['--runtime','nodejs','--modules',resolve(__dirname,'package.json'),'--types','ProjectDoctor','--methods','inspect,attention','--port',String(port),'--httpPort',String(httpPort),'--GSMU=0','--noVersioning'], { env, cwd: directory, stdio: 'inherit', detached: true });
let stopping=false;
const stop=()=>{ if(stopping)return; stopping=true; try{process.kill(-child.pid,'SIGTERM');}catch{}; const timer=setTimeout(()=>{try{process.kill(-child.pid,'SIGKILL');}catch{}},2000);timer.unref(); };
process.on('SIGINT',stop);process.on('SIGTERM',stop);
child.on('error',error=>{console.error(error.message);rmSync(directory,{recursive:true,force:true});process.exitCode=1;});
child.on('exit',code=>{rmSync(directory,{recursive:true,force:true});process.exitCode=stopping?0:(code??1);});
