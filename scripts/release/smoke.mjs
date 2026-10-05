#!/usr/bin/env node
// Build-time smoke only: the distributed application itself never needs Node.
// Run only the extracted native executable, outside the checkout, with synthetic data.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { appendFile, mkdir, mkdtemp, readFile, readdir, realpath, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

const [binaryArg, version] = process.argv.slice(2);
assert(binaryArg && version, 'Expected extracted executable and version');
const binary = resolve(binaryArg);
// Canonicalize only our newly created synthetic directory: macOS /var is a
// symlink, while the importer deliberately rejects linked path ancestors.
const home = await realpath(await mkdtemp(join(tmpdir(), 'usage-lens-isolated-smoke-')));
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
  assert.deepEqual(setup.compatibility.readableSchemas, [2, 3, 4]);
  assert.equal(setup.compatibility.selectedSchema, null);
  assert.equal(setup.compatibility.wouldUpgrade, null);
  assert.equal(setup.compatibility.backupStatus, 'not_verified');
  assert.equal(setup.compatibility.traceImportTargetSchema, 4);
  assert.match(setup.compatibility.rollbackWarning, /v0\.5\.0 and older cannot read schema 4/);
  assert.equal(setup.compatibility.backupSteps.length, 4);
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
  assert.equal(ready.compatibility.selectedSchema, 2);
  assert.equal(ready.compatibility.wouldUpgrade, true);
  assert.equal(ready.compatibility.backupStatus, 'not_verified');
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
  assert.equal(JSON.parse(run(['trace-attempts', '--db', db, '--source', 'release-smoke'])).coverage.capture, 'not_captured');
  assert.equal(JSON.parse(run(['trace-summary', '--db', db, '--source', 'release-smoke'])).attemptCount, '0');
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
  assert.equal(upgraded.compatibility.selectedSchema, 3);
  assert.equal(upgraded.compatibility.wouldUpgrade, true);
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

  // One closed, explicitly selected synthetic RolloutTrace bundle. This does not
  // enable recording, discover a client, or launch any model/runtime process.
  const traceDirectory = join(home, 'selected-synthetic-trace');
  await mkdir(join(traceDirectory, 'payloads'), { recursive: true });
  const traceVersion = 'a956835d020762cb2b570053af06f643a11c0ecc';
  const traceStart = Date.parse('2026-10-03T00:00:00.000Z');
  const traceManifest = { schema_version: 1, trace_id: 'synthetic-trace', rollout_id: 'synthetic-rollout', root_thread_id: 'synthetic-thread', started_at_unix_ms: traceStart, raw_event_log: 'trace.jsonl', payloads_dir: 'payloads' };
  const payloadRef = (ordinal, type) => ({ raw_payload_id: `raw_payload:${ordinal}`, path: `payloads/${ordinal}.json`, kind: { type } });
  const traceEvent = (seq, payload) => JSON.stringify({ schema_version: 1, rollout_id: traceManifest.rollout_id, seq, wall_time_unix_ms: traceStart + seq, thread_id: 'synthetic-thread', codex_turn_id: 'synthetic-turn', payload });
  const visibleMessage = (role, text, extra = {}) => ({ type: 'message', role, content: [{ type: role === 'assistant' ? 'output_text' : 'input_text', text }], ...extra });
  const secretCanary = 'sk-proj-SyntheticOnlyNeverARealCredential123456';
  const traceRequest = {
    model: 'synthetic-model', reasoning: { effort: 'synthetic-effort' }, service_tier: 'synthetic-tier',
    instructions: 'EXCLUDED_SYSTEM_INSTRUCTIONS_CANARY',
    input: [
      visibleMessage('user', `Synthetic trace visible user ${secretCanary}`, { internal_chat_message_metadata_passthrough: { content_item_kinds: ['user.text'] } }),
      visibleMessage('user', 'EXCLUDED_UNCLASSIFIED_USER_CANARY'),
      visibleMessage('system', 'EXCLUDED_SYSTEM_CANARY'),
      visibleMessage('developer', 'EXCLUDED_DEVELOPER_CANARY'),
      visibleMessage('assistant', 'EXCLUDED_ANALYSIS_CANARY', { channel: 'analysis' }),
      { type: 'function_call', arguments: 'EXCLUDED_TOOL_CANARY' },
    ],
  };
  const traceResponse = {
    output_items: [visibleMessage('assistant', 'Synthetic trace visible answer'), { type: 'reasoning', text: 'EXCLUDED_REASONING_CANARY' }],
    response_id: 'synthetic-trace-response', upstream_request_id: 'synthetic-upstream-request',
    token_usage: { input_tokens: 80, cached_input_tokens: 10, cache_write_input_tokens: 0, output_tokens: 20, reasoning_output_tokens: 5, total_tokens: 100 },
  };
  const started = { type: 'inference_started', inference_call_id: 'synthetic-inference', thread_id: 'synthetic-thread', codex_turn_id: 'synthetic-turn', model: 'synthetic-model', request_payload: payloadRef(1, 'inference_request') };
  const completed = { type: 'inference_completed', inference_call_id: 'synthetic-inference', response_id: traceResponse.response_id, upstream_request_id: traceResponse.upstream_request_id, response_payload: payloadRef(2, 'inference_response') };
  const failedStarted = { ...started, inference_call_id: 'synthetic-failed-inference', model: 7, request_payload: payloadRef(3, 'inference_request') };
  const failed = { type: 'inference_failed', inference_call_id: 'synthetic-failed-inference', upstream_request_id: null, partial_response_payload: null };
  const traceRows = [traceEvent(1, started), traceEvent(2, completed), traceEvent(3, failedStarted), traceEvent(4, failed)].join('\n') + '\n';
  await writeFile(join(traceDirectory, 'manifest.json'), JSON.stringify(traceManifest));
  await writeFile(join(traceDirectory, 'trace.jsonl'), traceRows);
  await writeFile(join(traceDirectory, 'payloads/1.json'), JSON.stringify(traceRequest));
  await writeFile(join(traceDirectory, 'payloads/2.json'), JSON.stringify(traceResponse));
  await writeFile(join(traceDirectory, 'payloads/3.json'), JSON.stringify({ model: 7, reasoning: [], service_tier: null, input: [] }));
  const traceImportArgs = ['import-trace-bundle', '--db', db, '--source', 'release-smoke', '--directory', traceDirectory, '--source-version', traceVersion];
  const schema3BeforeTrace = await readFile(db);
  const schema3BeforePreview = { modified: (await stat(db)).mtimeMs, files: await readdir(home) };
  const tracePreviewText = run([...traceImportArgs, '--dry-run']);
  const tracePreview = JSON.parse(tracePreviewText);
  assert.equal(tracePreview.schemaVersion, 1);
  assert.equal(tracePreview.operation, 'trace_import_preflight');
  assert.equal(tracePreview.dryRun, true);
  assert.equal(tracePreview.status, 'ready');
  assert.equal(tracePreview.attemptsInBundle, '2');
  assert.equal(tracePreview.attemptsWouldInsert, '2');
  assert.equal(tracePreview.attemptsAlreadyPresent, '0');
  assert.equal(tracePreview.contentsWouldRetain, '0');
  assert.equal(tracePreview.importAlreadyPresent, false);
  assert.equal(tracePreview.contentCaptureEnabled, false);
  assert.deepEqual(tracePreview.database, { accessMode: 'read_only', schemaVersion: 3, targetSchemaVersion: 4, wouldUpgrade: true });
  assert(Array.isArray(tracePreview.warningCodes));
  assert(Array.isArray(tracePreview.warnings));
  assert(Array.isArray(tracePreview.nextSteps));
  for (const privateValue of [traceDirectory, db, 'synthetic-inference', 'Synthetic trace visible user', secretCanary, 'EXCLUDED_SYSTEM_INSTRUCTIONS_CANARY']) {
    assert(!tracePreviewText.includes(privateValue), 'Preflight leaked private path, identity or content');
  }
  assert.deepEqual(await readFile(db), schema3BeforeTrace, 'Trace preflight changed database bytes or schema');
  assert.equal((await stat(db)).mtimeMs, schema3BeforePreview.modified, 'Trace preflight changed database mtime');
  assert.deepEqual(await readdir(home), schema3BeforePreview.files, 'Trace preflight created state or sidecars');
  const missingPreviewArgs = [...traceImportArgs, '--dry-run'];
  missingPreviewArgs[missingPreviewArgs.indexOf('--db') + 1] = join(home, 'absent-preview.sqlite');
  assert.equal(run(missingPreviewArgs, 1), '');
  assert.deepEqual(await readdir(home), schema3BeforePreview.files, 'Trace preflight created a missing database');
  const schema2Db = join(home, 'synthetic-schema2-preview.sqlite');
  run(['source', '--db', schema2Db, '--source', 'release-smoke', '--mode', 'imported']);
  const schema2PreviewArgs = [...traceImportArgs, '--dry-run'];
  schema2PreviewArgs[schema2PreviewArgs.indexOf('--db') + 1] = schema2Db;
  const schema2BeforePreview = { bytes: await readFile(schema2Db), modified: (await stat(schema2Db)).mtimeMs, files: await readdir(home) };
  const schema2Preview = JSON.parse(run(schema2PreviewArgs));
  assert.equal(schema2Preview.database.schemaVersion, 2);
  assert.equal(schema2Preview.database.wouldUpgrade, true);
  assert.equal(schema2Preview.attemptsWouldInsert, '2');
  assert.deepEqual(await readFile(schema2Db), schema2BeforePreview.bytes, 'Schema-2 preflight changed database bytes or schema');
  assert.equal((await stat(schema2Db)).mtimeMs, schema2BeforePreview.modified, 'Schema-2 preflight changed database mtime');
  assert.deepEqual(await readdir(home), schema2BeforePreview.files, 'Schema-2 preflight created sidecars');
  assert.equal(JSON.parse(run(['trace-attempts', '--db', db, '--source', 'release-smoke'])).coverage.capture, 'not_captured');
  assert.equal(JSON.parse(run(['trace-summary', '--db', db, '--source', 'release-smoke'])).attemptCount, '0');
  run([...traceImportArgs.slice(0, -1), 'unsupported-version'], 1);
  run([...traceImportArgs.slice(0, -1), 'unsupported-version', '--dry-run'], 1);
  assert.deepEqual(await readFile(db), schema3BeforeTrace, 'Rejected trace import or trace query migrated schema 3');
  const traced = JSON.parse(run(traceImportArgs));
  assert.equal(traced.attemptsInserted, '2');
  assert.equal(traced.contentsRetained, '0');
  assert.equal(traced.importAlreadyPresent, false);
  const traceReadState = { bytes: await readFile(db), modified: (await stat(db)).mtimeMs, files: await readdir(home) };
  const traceDoctor = JSON.parse(run(['doctor', '--db', db, '--source', 'release-smoke']));
  assert.equal(traceDoctor.database.schemaVersion, 4);
  assert.equal(traceDoctor.compatibility.selectedSchema, 4);
  assert.equal(traceDoctor.compatibility.wouldUpgrade, false);
  const tracePage = JSON.parse(run(['trace-attempts', '--db', db, '--source', 'release-smoke', '--limit', '1']));
  assert.equal(tracePage.attempts.length, 1);
  assert(tracePage.nextCursor);
  const traceNext = JSON.parse(run(['trace-attempts', '--db', db, '--source', 'release-smoke', '--limit', '1', '--cursor', tracePage.nextCursor]));
  const traceAttempt = traceNext.attempts[0];
  assert.equal(traceAttempt.status, 'completed');
  assert.equal(traceAttempt.evidence, 'prepared_request');
  assert.equal(traceAttempt.request.model.value, 'synthetic-model');
  assert.equal(traceAttempt.request.reasoningEffort.state, 'reported');
  assert.equal(traceAttempt.request.reasoningEffort.value, 'synthetic-effort');
  assert.equal(traceAttempt.request.serviceTier.state, 'reported');
  assert.equal(traceAttempt.request.serviceTier.value, 'synthetic-tier');
  assert.equal(traceAttempt.observed.model.state, 'omitted');
  assert.equal(traceAttempt.observed.serviceTier.state, 'omitted');
  assert.equal(tracePage.attempts[0].request.model.state, 'invalid');
  assert.equal(tracePage.attempts[0].request.reasoningEffort.state, 'invalid');
  assert.equal(tracePage.attempts[0].request.serviceTier.state, 'not_reported');
  assert.equal(tracePage.attempts[0].tokens, null);
  const traceDetailArgs = ['trace-detail', '--db', db, '--source', 'release-smoke', '--attempt', traceAttempt.attemptId];
  assert.equal(JSON.parse(run(traceDetailArgs)).content, null, 'Capture-off import retained visible text');
  const traceSummary = JSON.parse(run(['trace-summary', '--db', db, '--source', 'release-smoke', '--from', '2026-10-03', '--to', '2026-10-03']));
  assert.equal(traceSummary.attemptCount, '2');
  assert.equal(traceSummary.tokenAttemptCount, '1');
  assert.equal(traceSummary.totals.totalTokens, '100');
  assert.equal(traceSummary.totals.inputTokens, '80');
  assert.equal(traceSummary.coverage.dateBasis, 'attempt_start_utc');
  assert.equal(traceSummary.coverage.completeness, 'partial');
  assert.equal(traceSummary.coverage.accountTotalRelationship, 'not_combined');
  assert.equal(traceSummary.source.id, 'release-smoke');
  assert.equal(traceSummary.timestampAnomalyCount, '0');
  assert.equal(traceSummary.byThread.length, 1);
  assert.equal(traceSummary.byThread[0].threadId, 'synthetic-thread');
  assert.deepEqual(traceSummary.byThread[0].statusCounts, { completed: '1', failed: '1', cancelled: '0', incomplete: '0' });
  assert.equal(traceSummary.byThread[0].count, '2');
  assert.equal(traceSummary.byThread[0].totals.totalTokens, '100');
  assert.equal(traceSummary.byRequestedSettings.length, 2);
  const reportedSettings = traceSummary.byRequestedSettings.find(row => row.request.model.state === 'reported');
  assert.equal(reportedSettings.request.reasoningEffort.value, 'synthetic-effort');
  assert.equal(reportedSettings.request.serviceTier.value, 'synthetic-tier');
  assert.equal(reportedSettings.totals.totalTokens, '100');
  assert.equal(traceSummary.byDay.length, 1);
  assert.equal(traceSummary.byDay[0].date, '2026-10-03');
  assert.equal(traceSummary.byDay[0].count, '2');
  for (const key of ['threadsTruncated', 'requestedSettingsTruncated', 'daysTruncated']) assert.equal(traceSummary[key], false);
  assert.equal(tracePage.order, 'newest_first');
  assert.equal(traceAttempt.timestampAnomaly, false);
  const oldestArgs = ['trace-attempts', '--db', db, '--source', 'release-smoke', '--order', 'oldest_first', '--limit', '1'];
  const oldestPage = JSON.parse(run(oldestArgs));
  assert.equal(oldestPage.order, 'oldest_first');
  assert.equal(oldestPage.attempts[0].attemptId, traceAttempt.attemptId);
  assert.equal(JSON.parse(run([...oldestArgs, '--cursor', oldestPage.nextCursor])).attempts[0].attemptId, tracePage.attempts[0].attemptId);
  run(['trace-attempts', '--db', db, '--source', 'release-smoke', '--cursor', oldestPage.nextCursor], 1);
  const oldCursor = JSON.parse(Buffer.from(tracePage.nextCursor, 'base64url').toString('utf8'));
  delete oldCursor.order;
  run(['trace-attempts', '--db', db, '--source', 'release-smoke', '--cursor', Buffer.from(JSON.stringify(oldCursor)).toString('base64url')], 1);
  run(['trace-summary', '--db', db, '--source', 'release-smoke', '--order', 'oldest_first'], 1);
  const exactFilters = ['--thread', 'synthetic-thread', '--status', 'completed', '--requested-model', 'synthetic-model', '--requested-effort', 'synthetic-effort', '--requested-tier', 'synthetic-tier'];
  const filteredList = JSON.parse(run(['trace-attempts', '--db', db, '--source', 'release-smoke', ...exactFilters]));
  const filteredSummary = JSON.parse(run(['trace-summary', '--db', db, '--source', 'release-smoke', ...exactFilters]));
  assert.equal(filteredList.attempts.length, 1);
  assert.equal(filteredList.attempts[0].attemptId, traceAttempt.attemptId);
  assert.equal(filteredSummary.attemptCount, '1');
  assert.equal(filteredSummary.totals.totalTokens, '100');
  for (const result of [filteredList, filteredSummary]) {
    assert.equal(result.threadId, 'synthetic-thread');
    assert.equal(result.status, 'completed');
    assert.equal(result.requestedModel, 'synthetic-model');
    assert.equal(result.requestedReasoningEffort, 'synthetic-effort');
    assert.equal(result.requestedServiceTier, 'synthetic-tier');
  }
  assert.equal(JSON.parse(run(['trace-summary', '--db', db, '--source', 'release-smoke', '--requested-model', 'SYNTHETIC-MODEL'])).attemptCount, '0');
  assert.equal(JSON.parse(run(['trace-summary', '--db', db, '--source', 'release-smoke', '--status', 'failed', '--requested-tier', 'synthetic-tier'])).attemptCount, '0');
  for (const args of [['--thread', ''], ['--status', 'unknown'], ['--requested-effort', ''], ['--requested-tier', '']]) {
    run(['trace-summary', '--db', db, '--source', 'release-smoke', ...args], 1);
  }
  const filteredPage = JSON.parse(run(['trace-attempts', '--db', db, '--source', 'release-smoke', '--thread', 'synthetic-thread', '--limit', '1']));
  assert(filteredPage.nextCursor);
  assert.equal(JSON.parse(run(['trace-attempts', '--db', db, '--source', 'release-smoke', '--thread', 'synthetic-thread', '--cursor', filteredPage.nextCursor])).attempts.length, 1);
  run(['trace-attempts', '--db', db, '--source', 'release-smoke', '--cursor', filteredPage.nextCursor], 1);
  for (const args of [['--thread', 'other-thread'], ['--status', 'completed'], ['--requested-model', 'synthetic-model'], ['--requested-effort', 'synthetic-effort'], ['--requested-tier', 'synthetic-tier']]) {
    run(['trace-attempts', '--db', db, '--source', 'release-smoke', '--cursor', tracePage.nextCursor, ...args], 1);
  }
  run(['trace-summary', '--db', db, '--source', 'release-smoke', '--from', '2026-10-03'], 1);
  run(['status', '--db', db]);
  run(['health', '--db', db, '--source', 'release-smoke']);
  assert.deepEqual(await readFile(db), traceReadState.bytes, 'Schema-4 queries changed database bytes');
  assert.equal((await stat(db)).mtimeMs, traceReadState.modified, 'Schema-4 queries changed database mtime');
  assert.deepEqual(await readdir(home), traceReadState.files, 'Schema-4 queries created a sidecar');
  const replayPreview = JSON.parse(run([...traceImportArgs, '--dry-run']));
  assert.equal(replayPreview.attemptsWouldInsert, '0');
  assert.equal(replayPreview.attemptsAlreadyPresent, '2');
  assert.equal(replayPreview.importAlreadyPresent, true);
  assert.equal(replayPreview.database.wouldUpgrade, false);
  assert.deepEqual(await readFile(db), traceReadState.bytes, 'Replay preflight changed the store');
  const traceRepeated = JSON.parse(run(traceImportArgs));
  assert.equal(traceRepeated.attemptsInserted, '0');
  assert.equal(traceRepeated.importAlreadyPresent, true);
  assert.deepEqual(await readFile(db), traceReadState.bytes, 'Identical trace replay changed the store');
  await writeFile(join(traceDirectory, 'payloads/1.json'), JSON.stringify({ ...traceRequest, model: 'changed-synthetic-model' }));
  run([...traceImportArgs, '--dry-run'], 1);
  assert.deepEqual(await readFile(db), traceReadState.bytes, 'Conflict preflight changed the store');
  run(traceImportArgs, 1);
  assert.deepEqual(await readFile(db), traceReadState.bytes, 'Conflicting trace import partially changed the store');
  await writeFile(join(traceDirectory, 'payloads/1.json'), JSON.stringify(traceRequest));

  run(['settings', '--db', db, '--content', 'true']);
  run(traceImportArgs);
  assert.equal(JSON.parse(run(traceDetailArgs)).content, null, 'Enabling capture backfilled an accepted trace');
  run(['source', '--db', db, '--source', 'trace-visible', '--mode', 'imported', '--name', 'Synthetic visible trace']);
  const visibleImportArgs = [...traceImportArgs];
  visibleImportArgs[visibleImportArgs.indexOf('--source') + 1] = 'trace-visible';
  const beforeVisiblePreview = await readFile(db);
  const visiblePreview = JSON.parse(run([...visibleImportArgs, '--dry-run']));
  assert.equal(visiblePreview.contentCaptureEnabled, true);
  assert.equal(visiblePreview.contentsWouldRetain, '2');
  assert.deepEqual(await readFile(db), beforeVisiblePreview, 'Capture-enabled preflight retained content');
  assert.equal(JSON.parse(run(visibleImportArgs)).contentsRetained, '2');
  const visibleDetailArgs = [...traceDetailArgs];
  visibleDetailArgs[visibleDetailArgs.indexOf('--source') + 1] = 'trace-visible';
  const visibleDetail = run(visibleDetailArgs);
  assert.match(visibleDetail, /Synthetic trace visible user/);
  assert.match(visibleDetail, /Synthetic trace visible answer/);
  assert.match(visibleDetail, /REDACTED/);
  for (const excluded of [secretCanary, 'EXCLUDED_SYSTEM_INSTRUCTIONS_CANARY', 'EXCLUDED_UNCLASSIFIED_USER_CANARY', 'EXCLUDED_SYSTEM_CANARY', 'EXCLUDED_DEVELOPER_CANARY', 'EXCLUDED_ANALYSIS_CANARY', 'EXCLUDED_TOOL_CANARY', 'EXCLUDED_REASONING_CANARY']) {
    assert(!visibleDetail.includes(excluded), `Trace detail leaked excluded synthetic content: ${excluded}`);
    assert(!(await readFile(db)).includes(Buffer.from(excluded)), 'Database retained excluded raw trace content');
  }
  run(['delete', '--db', db, '--source', 'trace-visible', '--target', 'content', '--confirm', 'DELETE']);
  run(visibleImportArgs);
  assert.equal(JSON.parse(run(visibleDetailArgs)).content, null, 'Trace replay resurrected deleted content');
  run(['delete', '--db', db, '--source', 'trace-visible', '--target', 'all', '--confirm', 'DELETE']);
  assert.equal(JSON.parse(run(visibleImportArgs)).attemptsInserted, '0', 'Trace replay resurrected all-data-deleted evidence');
  assert.equal(JSON.parse(run(['trace-summary', '--db', db, '--source', 'trace-visible'])).attemptCount, '0');
  assert.equal(await readFile(join(traceDirectory, 'payloads/1.json'), 'utf8'), JSON.stringify(traceRequest), 'Import/deletion changed raw source files');

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
  const searchResponse = await fetch(`${url}/api/events/search?sourceId=demo&query=Synthetic&limit=1`);
  assert.equal(searchResponse.status, 200);
  const searched = await searchResponse.json();
  assert.equal(searched.source.id, 'demo');
  assert.equal(searched.events.length, 1);
  assert.equal(searched.search.scope, 'retained_redacted_local_content');
  assert.equal(searched.search.candidateLimit, 10000);
  assert.equal(searched.events[0].content, undefined, 'Search leaked retained content');
  assert(searched.nextCursor, 'Expected bounded search pagination');
  const searchNext = await fetch(`${url}/api/events/search?sourceId=demo&query=Synthetic&limit=1&cursor=${encodeURIComponent(searched.nextCursor)}`);
  assert.equal(searchNext.status, 200);
  assert.notEqual((await searchNext.json()).events[0].eventId, searched.events[0].eventId);
  assert.equal((await fetch(`${url}/api/events/search?sourceId=demo&query=changed&cursor=${encodeURIComponent(searched.nextCursor)}`)).status, 400);
  const periodResponse = await fetch(`${url}/api/response-tokens/period?sourceId=demo&fromDate=2024-02-28&toDate=2024-03-02`);
  assert.equal(periodResponse.status, 200);
  const period = await periodResponse.json();
  assert.equal(period.source.id, 'demo');
  assert.equal(period.coverage.dateBasis, 'occurred_at_utc');
  assert.equal(period.responseCount, '0');
  assert.equal(period.undatedResponseScope, 'all_retained_source');
  assert.equal(period.reasoningEffort.status, 'not_recorded');
  assert.equal(period.serviceTier.status, 'not_recorded');
  assert.equal((await fetch(`${url}/api/response-tokens/period?sourceId=demo&fromDate=2024-02-28`)).status, 400);
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
  console.log('Native executable passed SQLite persistence, schema-2/3/4 read-only queries, incremental append/no-op/truncation, trace preflight/import/replay/conflict/privacy, exact trace filters and cursor scope, doctor schema/backup guidance, health/skill trends, CLI, embedded demo HTTP/UI, and eight-tool aggregate-only stdio MCP smoke');
} finally {
  clearTimeout(limit);
  await Promise.all([...children].map(stop));
  await rm(home, { recursive: true, force: true });
}
