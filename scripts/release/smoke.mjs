#!/usr/bin/env node
// Build-time smoke only: the distributed application itself never needs Node.
// Run only the extracted native executable, outside the checkout, with synthetic data.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { appendFile, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
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

function run(args, expectedStatus = 0) {
  const result = spawnSync(binary, args, { cwd: home, env, encoding: 'utf8', timeout: 10000 });
  assert.equal(result.error, undefined, String(result.error));
  assert.equal(result.status, expectedStatus, result.stderr);
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
  const setup = JSON.parse(run(['doctor']));
  assert.equal(setup.schemaVersion, 1);
  assert.equal(setup.application.version, version);
  assert.equal(setup.status, 'incomplete');
  assert.equal(setup.scope, 'read_only_setup_diagnostics');
  assert.equal(setup.database, null);
  assert.equal(setup.sourceHealth, null);
  assert(setup.checks.some(check => check.id === 'dashboard' && check.status === 'pass'));
  assert.deepEqual(await readdir(home), [], 'Setup-only doctor must not create local state');
  const absent = JSON.parse(run(['doctor', '--db', join(home, 'not-created.sqlite')], 1));
  assert.equal(absent.status, 'failed');
  assert(absent.checks.some(check => check.id === 'database' && check.status === 'fail'));
  assert.deepEqual(await readdir(home), [], 'Failed doctor must not create the missing store');
  const demo = JSON.parse(run(['status', '--demo']));
  assert(Array.isArray(demo.sources) && demo.sources.length > 0, 'Synthetic demo must seed SQLite');
  // Native SQLite must create, close, and reopen a disposable persistent store.
  const db = join(home, 'synthetic.sqlite');
  run(['source', '--db', db, '--source', 'release-smoke', '--mode', 'imported', '--name', 'Synthetic release smoke']);
  const persisted = JSON.parse(run(['status', '--db', db]));
  assert(persisted.sources.some(source => source.id === 'release-smoke'));
  const beforeQuery = { bytes: await readFile(db), modified: (await stat(db)).mtimeMs, files: await readdir(home) };
  const ready = JSON.parse(run(['doctor', '--db', db, '--source', 'release-smoke']));
  assert.equal(ready.status, 'ready');
  assert.equal(ready.database.schemaVersion, 2);
  assert.equal(ready.database.accessMode, 'read_only');
  assert.equal(ready.database.journalMode, 'rollback');
  const health = JSON.parse(run(['health', '--db', db, '--source', 'release-smoke']));
  assert.equal(health.schemaVersion, 1);
  assert.equal(health.source.id, 'release-smoke');
  assert.equal(health.coverage.completeness, 'partial');
  assert.equal(health.coverage.preCollectionHistory, 'unknown');
  assert(health.observations.every(observation => observation.availability === 'missing'));
  for (const kind of ['events', 'skills', 'responseTokens', 'imports']) {
    assert.equal(health.stored[kind].count, '0');
    assert.deepEqual(ready.sourceHealth.stored[kind], health.stored[kind]);
  }
  const trend = JSON.parse(run(['skill-summary', '--db', db, '--source', 'release-smoke', '--from', '2024-02-28', '--to', '2024-03-02', '--skill', 'synthetic-skill']));
  assert.equal(trend.basis, 'occurred_at_utc');
  assert.equal(trend.totalsScope, 'dated_range');
  assert.equal(trend.unknownOccurredAtCount, '0');
  assert.equal(trend.unknownOccurredAtScope, 'all_retained_source_matching_skill');
  assert.deepEqual(trend.daily, []);
  assert.deepEqual(trend.totals, {requested:'0', loaded:'0', invoked:'0', loadedEvidence:{mainRead:'0', instructionInjection:'0', unknown:'0'}});
  assert.equal(trend.skillsTruncated, false);
  run(['skill-summary', '--db', db, '--source', 'release-smoke', '--from', '2024-02-28'], 1);
  const wrongSource = JSON.parse(run(['doctor', '--db', db, '--source', 'missing-source'], 1));
  assert.equal(wrongSource.status, 'failed');
  assert.deepEqual(await readFile(db), beforeQuery.bytes, 'Read-only diagnostics changed database bytes');
  assert.equal((await stat(db)).mtimeMs, beforeQuery.modified, 'Read-only diagnostics changed database mtime');
  assert.deepEqual(await readdir(home), beforeQuery.files, 'Read-only diagnostics created a sidecar');

  // Explicit one-shot ingestion only. Never inspect a real transcript or client home.
  const rollout = join(home, 'synthetic-rollout.jsonl');
  const sourceVersion = 'a75987455a2879ca151cea5e118fa307be868583';
  const row = (type, payload) => JSON.stringify({ timestamp: '2026-10-03T00:00:00.000Z', type, payload });
  const firstPrefix = row('session_meta', { id: 'synthetic-thread', session_id: 'synthetic-session' }) + '\n';
  const message = row('response_item', { type: 'message', id: 'synthetic-message', role: 'user', content: [{ type: 'input_text', text: 'Synthetic local smoke only' }] });
  const importArgs = ['import-rollout-incremental', '--db', db, '--source', 'release-smoke', '--file', rollout, '--stream', 'synthetic-stream', '--source-version', sourceVersion];
  await writeFile(rollout, firstPrefix + message);
  const initialImport = JSON.parse(run(importArgs));
  assert.equal(initialImport.completeBytes, Buffer.byteLength(firstPrefix));
  assert.equal(initialImport.completeLines, 1);
  assert.equal(initialImport.deferredBytes, Buffer.byteLength(message));
  assert.equal(initialImport.eventsInserted, '0', 'Unterminated valid JSON must be deferred');
  await appendFile(rollout, '\n');
  const appended = JSON.parse(run(importArgs));
  assert.equal(appended.completeBytes, Buffer.byteLength(firstPrefix + message + '\n'));
  assert.equal(appended.completeLines, 2);
  assert.equal(appended.deferredBytes, 0);
  assert.equal(appended.eventsInserted, '1');
  assert.equal(appended.checkpointAdvanced, true);
  const beforeNoOp = await readFile(db);
  const repeated = JSON.parse(run(importArgs));
  assert.equal(repeated.eventsInserted, '0');
  assert.equal(repeated.responseTokensInserted, '0');
  assert.equal(repeated.checkpointAdvanced, false);
  assert.deepEqual(await readFile(db), beforeNoOp, 'Unchanged prefix must not rewrite store');
  const upgradedRead = { bytes: await readFile(db), modified: (await stat(db)).mtimeMs, files: await readdir(home) };
  const upgraded = JSON.parse(run(['doctor', '--db', db, '--source', 'release-smoke']));
  assert.equal(upgraded.database.schemaVersion, 3);
  assert.equal(upgraded.database.accessMode, 'read_only');
  const importedHealth = JSON.parse(run(['health', '--db', db, '--source', 'release-smoke']));
  assert.equal(importedHealth.stored.events.count, '1');
  run(['status', '--db', db]);
  run(['skill-summary', '--db', db, '--source', 'release-smoke']);
  assert.deepEqual(await readFile(db), upgradedRead.bytes, 'Schema-3 queries changed database bytes');
  assert.equal((await stat(db)).mtimeMs, upgradedRead.modified, 'Schema-3 queries changed database mtime');
  assert.deepEqual(await readdir(home), upgradedRead.files, 'Schema-3 queries created a sidecar');
  await writeFile(rollout, firstPrefix);
  run(importArgs, 1);
  assert.deepEqual(await readFile(db), upgradedRead.bytes, 'Truncation must not mutate records or progress');
  assert.equal(JSON.parse(run(['health', '--db', db, '--source', 'release-smoke'])).stored.events.count, '1');

  const server = start(['serve', '--demo', '--port', '0']);
  const line = await firstLine(server);
  const url = line.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0];
  assert(url, 'Expected explicit loopback startup URL');
  const response = await fetch(`${url}/api/status`);
  assert.equal(response.status, 200);
  const status = await response.json();
  assert(status.sources.length > 0, 'HTTP demo has no synthetic sources');
  const skillResponse = await fetch(`${url}/api/skill-summary?sourceId=demo&fromDate=2024-02-28&toDate=2024-03-02&skillName=spreadsheets`);
  assert.equal(skillResponse.status, 200);
  const skillSummary = await skillResponse.json();
  assert.equal(skillSummary.source.id, 'demo');
  assert.equal(skillSummary.basis, 'occurred_at_utc');
  assert.equal(skillSummary.skillName, 'spreadsheets');
  assert(Array.isArray(skillSummary.daily));
  assert.equal((await fetch(`${url}/api/skill-summary?sourceId=demo&fromDate=2024-02-28`)).status, 400);
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
  const expected = ['usage_status', 'usage_overview', 'usage_daily', 'usage_quota', 'usage_tools', 'usage_skills', 'usage_response_tokens', 'usage_health'];
  assert.deepEqual(tools.map(tool => tool.name).sort(), expected.sort());
  const result = await request('tools/call', { name: 'usage_status', arguments: {} });
  assert(!result.isError);
  const mcpStatus = JSON.parse(result.content[0].text);
  assert(mcpStatus.sources.length > 0);
  const mcpHealth = await request('tools/call', { name: 'usage_health', arguments: { sourceId: mcpStatus.sources[0].id } });
  assert(!mcpHealth.isError);
  const aggregate = JSON.parse(mcpHealth.content[0].text);
  assert.equal(aggregate.source.id, mcpStatus.sources[0].id);
  assert.equal(aggregate.coverage.completeness, 'partial');
  assert.equal(aggregate.provenance.estimated, false);
  assert(Array.isArray(aggregate.observations));
  const mcpSkills = await request('tools/call', { name: 'usage_skills', arguments: { sourceId: 'demo', fromDate:'2024-02-28', toDate:'2024-03-02', skillName:'spreadsheets' } });
  assert(!mcpSkills.isError);
  const mcpSkillSummary = JSON.parse(mcpSkills.content[0].text);
  assert.deepEqual(mcpSkillSummary.totals, skillSummary.totals);
  assert.deepEqual(mcpSkillSummary.daily, skillSummary.daily);
  assert.equal(mcpSkillSummary.unknownOccurredAtScope, 'all_retained_source_matching_skill');
  assert.equal((await request('tools/call', {name:'usage_skills', arguments:{sourceId:'demo',fromDate:'2024-01-01',toDate:'2025-01-01'}})).isError, true);
  lines.close();
  await stop(mcp);
  console.log('Extracted native artifact passed SQLite persistence, schema-2/3 read-only queries, incremental append/no-op/truncation, doctor/health/skill trends, CLI, embedded demo HTTP/UI, and eight-tool aggregate-only stdio MCP smoke');
} finally {
  clearTimeout(limit);
  await Promise.all([...children].map(stop));
  await rm(home, { recursive: true, force: true });
}
