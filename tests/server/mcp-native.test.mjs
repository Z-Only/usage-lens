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
    assert.deepEqual(tools.map(t => t.name), ['usage_status','usage_overview','usage_daily','usage_quota','usage_tools','usage_skills','usage_response_tokens']);
    for (const tool of tools) {
      assert.deepEqual(tool.annotations,{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:false});
      const args = tool.name === 'usage_status' ? {} : tool.name === 'usage_daily' ? { sourceId:'demo',fromDate:'2026-09-01',toDate:'2026-10-02' } : {sourceId:'demo'};
      const result = await client.callTool({name:tool.name,arguments:args});
      assert.notEqual(result.isError,true);
      for (const secret of ['Synthetic request:','toolArguments','toolResult','example.csv','Synthetic response:']) assert.ok(!JSON.stringify(result).includes(secret));
    }
    for (const call of [{name:'usage_overview',arguments:{sourceId:'missing'}},{name:'usage_overview',arguments:{sourceId:'demo',path:'/private'}},{name:'collect',arguments:{}}]) assert.equal((await client.callTool(call)).isError,true);
    await client.ping();
  } finally { await client.close(); }
});
