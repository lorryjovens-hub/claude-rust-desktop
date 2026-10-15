import { afterEach, describe, expect, it, vi } from 'vitest';
import { sendMessage as sendDesktopMessage } from '../api';
import { sendMessage as sendModularMessage } from './chat';

vi.mock('./client', () => ({
  API_BASE: '/api', isTauriApp: true, getToken: () => null,
  detectBridgePort: vi.fn().mockResolvedValue(30085),
  isBridgeReachable: vi.fn().mockResolvedValue(true), resetBridgeReachableCache: vi.fn(),
  getUserModeForConversation: () => 'selfhosted', resolveEnvCreds: () => ({}),
}));

afterEach(() => vi.unstubAllGlobals());

describe.each([['desktop', sendDesktopMessage], ['modular', sendModularMessage]] as const)('%s streaming text', (_name, sendMessage) => {
  it('forwards HTTP 400 with its original response body', async () => {
    const body = '{"detail":"The model is not supported."}';
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(body, { status: 400 })));
    const onError = vi.fn();
    await sendMessage('test', 'hello', null, vi.fn(), vi.fn(), onError);
    expect(onError).toHaveBeenCalledWith(`HTTP 400\n${body}`);
  });
  it('forwards a structured 400 error from an established stream', async () => {
    const event = { type: 'error', status: 400, error: { message: 'Unsupported model' } };
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(`data: ${JSON.stringify(event)}\n\n`)));
    const onError = vi.fn();
    const onDone = vi.fn();
    await sendMessage('test', 'hello', null, vi.fn(), onDone, onError);
    expect(onError).toHaveBeenCalledWith('HTTP 400\n{"message":"Unsupported model"}');
    expect(onDone).not.toHaveBeenCalled();
  });
  it('displays short Chinese text immediately, before a delayed tool starts', async () => {
    let controller!: ReadableStreamDefaultController<Uint8Array>;
    const stream = new ReadableStream<Uint8Array>({ start(value) { controller = value; } });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(stream)));
    const events: string[] = [];
    const error = vi.fn();
    const done = vi.fn();
    const push = (data: unknown) => controller.enqueue(new TextEncoder().encode(`data: ${JSON.stringify(data)}\n\n`));
    const running = sendMessage('test', 'hello', null, (delta) => events.push(`text:${delta}`), done, error,
      undefined, undefined, undefined, undefined, undefined, undefined, event => events.push(`tool:${event.type}`), undefined, 'test-model');
    push({ type: 'content_block_delta', delta: { type: 'text_delta', text: '我查一下。' } });
    await vi.waitFor(() => expect(events).toEqual(['text:我查一下。']));
    push({ type: 'tool_use_start', tool_use_id: 'search', tool_name: 'WebSearch', tool_input: { query: 'test' } });
    push({ type: 'tool_use_done', tool_use_id: 'search', content: 'result' });
    push({ type: 'content_block_delta', delta: { type: 'text_delta', text: '结果如下。' } });
    push({ type: 'message_stop' });
    controller.close();
    await running;
    expect(error).not.toHaveBeenCalled();
    expect(events).toEqual(['text:我查一下。', 'tool:start', 'tool:done', 'text:结果如下。']);
    expect(done).toHaveBeenCalledWith('我查一下。结果如下。');
  });

  it('keeps split artifact tags out of chat while promptly emitting surrounding text', async () => {
    const chunks = ['前言。<cp_', 'artifact title="Report" format="markdown">', '# 内容', '</cp_art', 'ifact>结束。'];
    const body = chunks.map(text => `data: ${JSON.stringify({ type: 'content_block_delta', delta: { type: 'text_delta', text } })}\n\n`).join('')
      + 'data: {"type":"message_stop"}\n\n';
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(body)));
    const done = vi.fn(), error = vi.fn(), draft = vi.fn();
    await sendMessage('test', 'hello', null, vi.fn(), done, error, undefined, undefined, undefined, undefined, draft, undefined, undefined, undefined, 'test-model');
    expect(error).not.toHaveBeenCalled();
    expect(done).toHaveBeenCalledWith('前言。结束。');
    expect(draft).toHaveBeenLastCalledWith(expect.objectContaining({ title: 'Report', preview: '# 内容', done: true }));
  });
});
