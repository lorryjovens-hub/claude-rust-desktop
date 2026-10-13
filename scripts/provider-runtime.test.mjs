import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { convertMessages } from './provider-runtime.mjs';

function worker(payload, bundled = false) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [bundled ? 'dist-runtime/provider-runtime.mjs' : 'scripts/provider-runtime.mjs'], { env: { ...process.env, CLAUDE_PROVIDER_WORKER: '1' }, windowsHide: true });
    let out = '', err = '';
    child.stdout.on('data', data => out += data); child.stderr.on('data', data => err += data);
    child.on('error', reject);
    child.on('exit', code => code ? reject(new Error(out + err)) : resolve(out.trim().split('\n').map(line => JSON.parse(line))));
    child.stdin.end(JSON.stringify(payload));
  });
}

test('built-in DeepSeek sends configured limits, effort and credential to selected URL', async () => {
  const requests = [];
  const server = http.createServer(async (req, res) => {
    let body = ''; for await (const chunk of req) body += chunk;
    requests.push({ url: req.url, authorization: req.headers.authorization, body: JSON.parse(body) });
    res.writeHead(200, { 'Content-Type': 'text/event-stream' });
    for (const data of [
      { choices: [{ index: 0, delta: { role: 'assistant', content: 'mock reply' }, finish_reason: null }] },
      { choices: [{ index: 0, delta: {}, finish_reason: 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 2, total_tokens: 12 } },
    ]) res.write(`data: ${JSON.stringify({ id: 'test', object: 'chat.completion.chunk', created: 1, model: 'deepseek-v4-flash', ...data })}\n\n`);
    res.end('data: [DONE]\n\n');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    for (const effort of ['off', 'low', 'high', 'max']) {
      const events = await worker({ provider: { builtin_id: 'deepseek', api_key: 'test-only-key', base_url: `http://127.0.0.1:${server.address().port}/v1` },
        model: { id: 'deepseek-v4-flash', name: 'Flash', context_window: 1000000, max_tokens: 32000, supports_text: true, supports_vision: false },
        max_tokens: 32000, effort, messages: [{ role: 'system', content: 'extra system' }, { role: 'user', content: 'hello' }], tools: [], system_prompt: 'test system' }, effort === 'max');
      const request = requests.at(-1);
      assert.equal(request.url, '/v1/chat/completions');
      assert.equal(request.authorization, 'Bearer test-only-key');
      assert.equal(request.body.max_tokens, 32000);
      assert.equal(request.body.model, 'deepseek-v4-flash');
      assert.ok(request.body.messages[0].content.includes('extra system'));
      assert.equal(request.body.messages.filter(m => m.role === 'user').length, 1);
      assert.equal(request.body.thinking.type, effort === 'off' ? 'disabled' : 'enabled');
      if (effort !== 'off') assert.equal(request.body.reasoning_effort, effort);
      assert.ok(events.some(event => event.choices[0].delta.content === 'mock reply'));
      assert.ok(events.at(-1).provider_content);
    }
  } finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
});

test('native assistant replay preserves thinking signatures and tool calls', () => {
  const native = { role: 'assistant', content: [{ type: 'thinking', thinking: 'analysis', thinkingSignature: 'signature' }, { type: 'toolCall', id: 'call', name: 'read', arguments: {} }] };
  const messages = convertMessages([{ role: 'assistant', provider_content: native }], {});
  assert.deepEqual(messages[0], native);
});

test('native replay retains tool names for the following result', () => {
  const native = { role: 'assistant', content: [{ type: 'toolCall', id: 'call-1', name: 'read_file', arguments: {} }] };
  const converted = convertMessages([{role:'assistant',provider_content:native},{role:'tool',tool_call_id:'call-1',content:'done'}], {});
  assert.equal(converted[1].toolName, 'read_file');
});

test('image input converts to native base64 content', () => {
  const messages = convertMessages([{role:'user',content:[{type:'image_url',image_url:{url:'data:image/png;base64,AA=='}},{type:'text',text:'Describe'}]}], {});
  assert.deepEqual(messages[0].content[0], {type:'image',mimeType:'image/png',data:'AA=='});
  assert.throws(() => convertMessages([{role:'user',content:[{type:'image_url',image_url:{url:'blob:unavailable'}}]}], {}));
});
