// Protocol compatibility against the official JavaScript SDK. The shipped binary needs no Node.
import test from 'node:test';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
const executable = process.env.USAGE_LENS_BINARY || resolve('target/debug/usage-lens');
test('native MCP speaks official SDK stdio and exposes only body-free aggregates', async () => {
  const transport = new StdioClientTransport({ command: executable, args: ['mcp', '--demo'], stderr: 'pipe' });
  const client = new Client({ name: 'native-compat-fixture', version: '1.0.0' });
  try {
    await client.connect(transport);
    const { tools } = await client.listTools();
    assert.deepEqual(tools.map(t => t.name), ['usage_status','usage_overview','usage_daily','usage_quota','usage_tools','usage_skills','usage_response_tokens','usage_health']);
    for (const tool of tools) {
      assert.deepEqual(tool.annotations,{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:false});
      const args = tool.name === 'usage_status' ? {} : tool.name === 'usage_daily' ? { sourceId:'demo',fromDate:'2026-09-01',toDate:'2026-10-02' } : {sourceId:'demo'};
      const result = await client.callTool({name:tool.name,arguments:args});
      assert.notEqual(result.isError,true);
      for (const secret of ['Synthetic request:','toolArguments','toolResult','example.csv','Synthetic response:']) assert.ok(!JSON.stringify(result).includes(secret));
    }
    for (const call of [{name:'usage_overview',arguments:{sourceId:'missing'}},{name:'usage_overview',arguments:{sourceId:'demo',path:'/private'}},{name:'collect',arguments:{}}]) assert.equal((await client.callTool(call)).isError,true);
    const skillTool = tools.find(t => t.name === 'usage_skills');
    assert.deepEqual(skillTool.inputSchema.dependentRequired, {fromDate:['toDate'],toDate:['fromDate']});
    const trend = await client.callTool({name:'usage_skills',arguments:{sourceId:'demo',fromDate:'2026-10-01',toDate:'2026-10-03',skillName:'spreadsheets'}});
    assert.notEqual(trend.isError,true);
    const summary = JSON.parse(trend.content[0].text);
    assert.equal(summary.basis,'occurred_at_utc');
    assert.equal(summary.totalsScope,'dated_range');
    assert.equal(summary.unknownOccurredAtScope,'all_retained_source_matching_skill');
    assert.ok(Array.isArray(summary.daily));
    for (const row of summary.daily) for (const key of ['requested','loaded','invoked']) assert.match(row[key],/^\d+$/);
    for (const args of [
      {sourceId:'demo',fromDate:'2026-10-01'},
      {sourceId:'demo',fromDate:'2024-01-01',toDate:'2025-01-01'},
      {sourceId:'demo',skillName:''},
      {sourceId:'demo',fromDate:'2023-02-29',toDate:'2023-03-01'}
    ]) assert.equal((await client.callTool({name:'usage_skills',arguments:args})).isError,true);
    await client.ping();
  } finally { await client.close(); }
});
