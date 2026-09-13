// Run through the ignored Rust server test official_sdk_http_and_stdio.
// Install the official SDK in a separate npm prefix and set MCP_SDK_ROOT.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { mkdtemp, writeFile, readFile, open, stat, rm } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
const require = createRequire(join(process.env.MCP_SDK_ROOT, 'package.json'));
const load = path => import(pathToFileURL(require.resolve(`@modelcontextprotocol/sdk/${path}`)));
const { Client } = await load('client/index.js');
const { StreamableHTTPClientTransport } = await load('client/streamableHttp.js');
const { StdioClientTransport } = await load('client/stdio.js');
const timer = setTimeout(() => { console.error('MCP interoperability test timed out'); process.exit(1); }, 45000);
const root = await mkdtemp(join(tmpdir(), 'hortcmd-sdk-'));
const path = join(root, 'message.txt');
await writeFile(path, 'official SDK round trip');
try {
  const endpoint = new URL(`http://127.0.0.1:${process.env.HORTCMD_MCP_PORT}/mcp`);
  for (const transport of [
    new StreamableHTTPClientTransport(endpoint, {requestInit:{headers:{Authorization:`Bearer ${process.env.HORTCMD_MCP_TOKEN}`}}}),
    new StdioClientTransport({command:process.env.MCP_BRIDGE_BIN, env:{HORTCMD_MCP_PORT:process.env.HORTCMD_MCP_PORT,HORTCMD_MCP_TOKEN:process.env.HORTCMD_MCP_TOKEN}, stderr:'pipe'}),
  ]) {
    const client = new Client({name:'hortcmd-sdk-interoperability',version:'1.0'});
    try {
      let transferRequest;
      const send = transport.send.bind(transport);
      transport.send = (message, ...rest) => {
        if (message.method === 'tools/call' && ['copy', 'move'].includes(message.params.name)) transferRequest = message.id;
        return send(message, ...rest);
      };
      await client.connect(transport);
      const listing = await client.listTools();
      assert(listing.tools.some(tool => tool.name === 'read_file'));
      const result = await client.callTool({name:'read_file',arguments:{path}});
      assert.equal(result.isError, false);
      assert(JSON.stringify(result).includes('official SDK round trip'));
      const refused = await client.callTool({name:'delete',arguments:{path}});
      assert.equal(refused.isError, true);
      assert.equal(refused.structuredContent.error.code, 'mcp.notPermitted');
      const resources = await client.listResources();
      assert(resources.resources.some(resource => resource.uri === 'hortcmd://panes/active'));
      const pane = await client.readResource({uri:'hortcmd://panes/active'});
      assert.equal(pane.contents[0].mimeType, 'application/json');
      // Sparse input keeps setup cheap while giving the real transfer time to
      // report progress. Wait for the server's terminal reply after cancellation.
      for (const name of ['copy', 'move']) {
        const source = join(root, `${name}-source`);
        const destination = join(root, `${name}-destination`);
        const file = await open(source, 'w');
        await file.truncate(1024 * 1024 * 1024);
        await file.close();
        await writeFile(destination, 'existing destination');
        let cancellation;
        let progressCount = 0;
        const result = await client.callTool({name, arguments:{from:source,to:destination,overwrite:true}}, undefined, {
          onprogress: () => {
            progressCount++;
            cancellation ??= client.notification({method:'notifications/cancelled',params:{requestId:transferRequest,reason:'integration test'}});
          },
        });
        assert(progressCount > 0, `${name}: no progress notification: ${JSON.stringify(result)}`);
        await cancellation;
        assert.equal(result.isError, true, `${name}: did not cancel`);
        assert.equal(await readFile(destination, 'utf8'), 'existing destination');
        assert.equal((await stat(source)).size, 1024 * 1024 * 1024);
        await rm(source);
      }
    } finally { await client.close(); }
  }
  const refusedBridge = spawnSync(process.env.MCP_BRIDGE_BIN, [], {env:{...process.env,HORTCMD_MCP_TOKEN:'wrong'},encoding:'utf8',timeout:5000});
  assert.notEqual(refusedBridge.status, 0);
  assert.match(refusedBridge.stderr, /token and port/);
  const offlineBridge = spawnSync(process.env.MCP_BRIDGE_BIN, [], {env:{...process.env,HORTCMD_MCP_PORT:'0'},encoding:'utf8',timeout:5000});
  assert.notEqual(offlineBridge.status, 0);
  assert.match(offlineBridge.stderr, /Cannot reach hortcmd.*Start the app/);
  console.log('Official SDK: HTTP and stdio handshake, tools, resources, rejection and copy/move cancellation passed.');
} finally { clearTimeout(timer); await rm(root,{recursive:true,force:true}); }
