#!/usr/bin/env node
// Build-time smoke only: the distributed application itself never needs Node.
// Run only the extracted native executable, outside the checkout, with synthetic data.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

const [binaryArg, version] = process.argv.slice(2);
assert(binaryArg && version, 'Expected extracted executable and version');
const binary = resolve(binaryArg);
const home = await mkdtemp(join(tmpdir(), 'usage-lens-isolated-smoke-'));
const env = Object.fromEntries(['PATH', 'SystemRoot', 'SYSTEMROOT', 'TEMP', 'TMP', 'TMPDIR', 'LANG'].filter(key => process.env[key]).map(key => [key, process.env[key]]));
Object.assign(env, { HOME: home, USERPROFILE: home, CODEX_HOME: join(home, 'unused-codex-home') });
const children = new Set();
const limit = setTimeout(() => { throw new Error('Packaged smoke timed out'); }, 45000);

function run(args) {
  const result = spawnSync(binary, args, { cwd: home, env, encoding: 'utf8', timeout: 10000 });
  assert.equal(result.error, undefined, String(result.error));
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}
function start(args) {
  const child = spawn(binary, args, { cwd: home, env, stdio: ['pipe', 'pipe', 'pipe'] });
  children.add(child);
  child.on('error', error => { throw error; });
  return child;
}
async function stop(child) {
  if (child.exitCode !== null || child.signalCode !== null) { children.delete(child); return; }
  const exited = new Promise(resolve => child.once('exit', resolve));
  child.kill();
  await exited;
  children.delete(child);
}
async function firstLine(child) {
  const lines = createInterface({ input: child.stdout });
  try {
    for await (const line of lines) return line;
    throw new Error('Executable exited without startup message');
  } finally { lines.close(); }
}

try {
  assert.match(run(['--help']), new RegExp(version.replaceAll('.', '\\.')));
  const demo = JSON.parse(run(['status', '--demo']));
  assert(Array.isArray(demo.sources) && demo.sources.length > 0, 'Synthetic demo must seed SQLite');
  // Native SQLite must create, close, and reopen a disposable persistent store.
  const db = join(home, 'synthetic.sqlite');
  run(['source', '--db', db, '--source', 'release-smoke', '--mode', 'imported', '--name', 'Synthetic release smoke']);
  const persisted = JSON.parse(run(['status', '--db', db]));
  assert(persisted.sources.some(source => source.id === 'release-smoke'));

  const server = start(['serve', '--demo', '--port', '0']);
  const line = await firstLine(server);
  const url = line.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0];
  assert(url, 'Expected explicit loopback startup URL');
  const response = await fetch(`${url}/api/status`);
  assert.equal(response.status, 200);
  const status = await response.json();
  assert(status.sources.length > 0, 'HTTP demo has no synthetic sources');
  const page = await fetch(url);
  assert.equal(page.status, 200);
  const html = await page.text();
  assert.match(html, /<html/i);
  const assetPaths = [...new Set([...html.matchAll(/(?:src|href)="([^"\s]+\.(?:js|css|wasm))"/g), ...html.matchAll(/from\s+['"]([^'"]+\.js)['"]/g)].map(match => match[1]))];
  assert(assetPaths.some(path => path.endsWith('.js')), 'Built embedded UI JS is missing');
  for (const path of assetPaths) {
    const resource = new URL(path, url);
    assert.equal(resource.origin, url, 'UI must use same-origin local assets');
    const asset = await fetch(resource);
    assert.equal(asset.status, 200, `Missing embedded UI asset ${path}`);
    assert((await asset.arrayBuffer()).byteLength > 0);
  }
  const provenance = await fetch(`${url}/frontend-build.json`);
  assert.equal(provenance.status, 200, 'Embedded Leptos build manifest is missing');
  const frontend = await provenance.json();
  assert.equal(frontend.schemaVersion, 1);
  assert.equal(frontend.framework, 'leptos', 'Release cannot contain stale non-Leptos assets');
  assert.equal(frontend.frameworkVersion, '0.8.21');
  assert.equal(frontend.wasmBindgenVersion, '0.2.129');
  assert(Object.keys(frontend.assets).some(path => path.endsWith('.wasm')), 'WebAssembly is missing');
  assert(Object.keys(frontend.assets).some(path => path.endsWith('.js')), 'WebAssembly loader is missing');
  for (const [path, expected] of Object.entries(frontend.assets)) {
    assert(!path.includes('..') && !path.includes('\\') && !path.startsWith('/'), 'Unsafe frontend asset');
    const resource = new URL(path, `${url}/`);
    assert.equal(resource.origin, url);
    const asset = await fetch(resource);
    assert.equal(asset.status, 200);
    const hash = createHash('sha256').update(Buffer.from(await asset.arrayBuffer())).digest('hex');
    assert.equal(hash, expected, `Embedded asset hash mismatch: ${path}`);
  }
  await stop(server);

  const mcp = start(['mcp', '--demo']);
  const lines = createInterface({ input: mcp.stdout });
  const pending = new Map();
  lines.on('line', line => {
    const message = JSON.parse(line);
    if (pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) reject(new Error(JSON.stringify(message.error))); else resolve(message.result);
    }
  });
  let id = 0;
  function request(method, params) {
    return new Promise((resolve, reject) => {
      const current = ++id;
      pending.set(current, { resolve, reject });
      mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: current, method, params }) + '\n');
    });
  }
  const initialized = await request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'synthetic-release-smoke', version: '1.0.0' } });
  assert.equal(initialized.serverInfo.name, 'usage-lens');
  assert.equal(initialized.serverInfo.version, version);
  mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
  const { tools } = await request('tools/list', {});
  const expected = ['usage_status', 'usage_overview', 'usage_daily', 'usage_quota', 'usage_tools', 'usage_skills', 'usage_response_tokens'];
  assert.deepEqual(tools.map(tool => tool.name).sort(), expected.sort());
  const result = await request('tools/call', { name: 'usage_status', arguments: {} });
  assert(!result.isError);
  assert(JSON.parse(result.content[0].text).sources.length > 0);
  lines.close();
  await stop(mcp);
  console.log('Extracted native artifact passed SQLite persistence, CLI, embedded demo HTTP/UI, and aggregate-only stdio MCP smoke');
} finally {
  clearTimeout(limit);
  await Promise.all([...children].map(stop));
  await rm(home, { recursive: true, force: true });
}
