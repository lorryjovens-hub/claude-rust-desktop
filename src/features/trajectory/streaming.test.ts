import { describe, it, expect, vi } from 'vitest';
import { createStreamCallbacks } from '../streaming/createStreamCallbacks';
import { useWorkspaceModeStore } from '../../stores/useWorkspaceModeStore';
import { persistMessages } from '../../api/conversations';
import { buildTrace, processMetadata, restoreProcessMetadata } from './model';
vi.mock('../../api', () => ({ getConversation: vi.fn(), getContextSize: vi.fn(), getAttachmentUrl: vi.fn(), compactConversation: vi.fn() }));
vi.mock('../../api/conversations', () => ({ persistMessages: vi.fn() }));
vi.mock('../../pyodideRunner', () => ({ executeCode: vi.fn(), sendCodeResult: vi.fn() }));
vi.mock('../../hooks/useAnalytics', () => ({ trackTokensUsed: vi.fn() }));

function harness() {
  let messages: any[] = [{ role: 'user', content: 'list' }, { role: 'assistant', content: '' }];
  const ctx: any = {
    conversationId: 'test', streamRequestId: 1, isStreamSessionActive: () => true,
    setMessagesFor: (_id: string, update: any) => { messages = update(messages); },
    setMessages: (update: any) => { messages = update(messages); },
    removeStreaming: vi.fn(), clearStreamSession: vi.fn(), messagesBufferRef: { current: new Map() },
    activeRequestCountRef: { current: 1 }, viewingIdRef: { current: 'test' }, abortControllerRef: { current: null },
    setLoading: vi.fn(), setCompactStatus: vi.fn(), setContextInfo: vi.fn(), setTokenUsage: vi.fn(), setActiveTasks: vi.fn(),
    setAskUserDialog: vi.fn(), setToolPermissionDialog: vi.fn(), setPermissionApproval: vi.fn(), setPlanMode: vi.fn(), setConversationTitle: vi.fn(),
  };
  return { callbacks: createStreamCallbacks(ctx), message: () => messages[messages.length - 1] };
}
describe('stream process recording', () => {
  it('persists a 400 card without replacing partial output or tool history', () => {
    const { callbacks: c, message } = harness();
    c.onDelta('Before.', 'Before.');
    c.onToolUse({ type: 'done', tool_use_id: 'read', tool_name: 'Read', content: 'package.json', is_error: false });
    c.onError('HTTP 400\n{"detail":"Unsupported model"}');
    expect(message().content).toBe('Before.');
    expect(message().requestError).toEqual({ status: 400, detail: '{"detail":"Unsupported model"}' });
    expect(message().toolCalls[0].result).toBe('package.json');
    expect(message().isThinking).toBe(false);
    const cached = JSON.parse(JSON.stringify({ ...message(), ...processMetadata(message()) }));
    expect(restoreProcessMetadata({ role: 'assistant', content: 'Before.' }, 0, [cached]).requestError).toEqual(message().requestError);
    expect(vi.mocked(persistMessages).mock.lastCall?.[1].slice(-1)[0]?.requestError).toEqual(message().requestError);
  });
  it.each(['chat', 'work', 'study', 'code'] as const)('records regular and task tool calls in %s, independently of presentation', mode => {
    useWorkspaceModeStore.setState({ mode });
    const { callbacks: c, message } = harness();
    for (const name of ['Read', 'TaskCreate', 'EnterPlanMode']) {
      c.onToolUse({ type: 'start', tool_use_id: name, tool_name: name, tool_input: { description: name } });
      c.onToolUse({ type: 'done', tool_use_id: name, tool_name: name, content: `${name} result`, is_error: false });
    }
    expect(message().toolCalls.map((tool: any) => tool.name)).toEqual(['Read', 'TaskCreate', 'EnterPlanMode']);
    expect(message().toolCalls.every((tool: any) => tool.status === 'done')).toBe(true);
    const calls = vi.mocked(persistMessages).mock.calls;
    const savedMessages = calls[calls.length - 1]![1];
    expect(buildTrace(JSON.parse(JSON.stringify(savedMessages))).filter(record => record.role === 'tool').map(record => record.result))
      .toEqual(['Read result', 'TaskCreate result', 'EnterPlanMode result']);
  });
  it.each(['onSystemEvents', 'onSystemEventsLight'] as const)('%s records search usage without closing or reassigning conversation steps', callback => {
    const { callbacks: c, message } = harness();
    c[callback]('request_snapshot', '', { snapshot: { id: 'main', tools: [] }, persisted: true });
    c.onDelta('Searching.', 'Searching.');
    c.onToolUse({ type: 'start', tool_use_id: 'search-tool', tool_name: 'WebSearch', tool_input: { query: 'weather' } });
    const search = { id: 'search', source: 'web_search', tools: [], status: 'running' };
    c[callback]('request_snapshot', '', { snapshot: search, persisted: true });
    c[callback]('request_snapshot', '', { snapshot: { ...search, status: 'completed', inputTokens: 50, outputTokens: 20 }, persisted: true });
    expect(message().requestSnapshots).toHaveLength(2);
    expect(message().requestSnapshots[1]).toMatchObject({ source: 'web_search', inputTokens: 50, outputTokens: 20, persisted: true });
    expect(message().currentRequestId).toBe('main');
    expect(message().processSteps.at(-1)).toMatchObject({ kind: 'tool', requestId: 'main' });
    expect(message().processSteps.at(-1).endedAt).toBeUndefined();
    c.onToolUse({ type: 'done', tool_use_id: 'search-tool', tool_name: 'WebSearch', content: 'Found a source', is_error: false });
    c.onDelta('Done.', 'Searching.Done.');
    expect(message().processSteps.at(-1)).toMatchObject({ kind: 'text', requestId: 'main', text: 'Done.' });
  });
  it.each(['onSystemEvents', 'onSystemEventsLight'] as const)('%s updates the same request on response completion without ending or reassigning tool steps', callback => {
    const { callbacks: c, message } = harness();
    const request = { id: 'request-1', number: 27, startedAt: 1000, model: 'test', status: 'running', systemPrompt: 'test', tools: [], responseBlocks: [] };
    c[callback]('request_snapshot', '', { snapshot: request, persisted: true });
    c.onThinking('Think', 'Think');
    c.onDelta('Answer', 'Answer');
    c.onToolUse({ type: 'start', tool_use_id: 'call', tool_name: 'Bash', tool_input: { command: 'pwd' } });
    c[callback]('request_snapshot', '', { snapshot: { ...request, status: 'completed', outputTokens: 305, reasoningTokens: 44, contentTokens: 261,
      responseBlocks: [{ type: 'thinking', text: 'Think' }, { type: 'text', text: 'Answer' }, { type: 'tool_call', id: 'call', name: 'Bash', input: { command: 'pwd' } }] }, persisted: true });
    expect(message().requestSnapshots).toHaveLength(1);
    expect(message().requestSnapshots[0]).toMatchObject({ number: 27, status: 'completed', outputTokens: 305, reasoningTokens: 44, contentTokens: 261 });
    expect(message().processSteps.at(-1)).toMatchObject({ kind: 'tool', requestId: 'request-1' });
    expect(message().processSteps.at(-1).endedAt).toBeUndefined();
    c[callback]('request_snapshot', '', { snapshot: { ...request, id: 'request-2', number: 28 }, persisted: true });
    c.onDelta('Final', 'AnswerFinal');
    expect(message().processSteps.at(-1).requestId).toBe('request-2');
    expect(message().requestSnapshots.map((r: any) => r.id)).toEqual(['request-1', 'request-2']);
  });
  it.each(['onSystemEvents', 'onSystemEventsLight'] as const)('%s associates blocks with their actual request snapshot', callback => {
    const { callbacks: c, message } = harness();
    const first = { id: 'request-1', number: 1, systemPrompt: 'initial', tools: [] };
    c[callback]('request_snapshot', '', { snapshot: first, persisted: true });
    c.onDelta('First.', 'First.');
    c[callback]('request_snapshot', '', { snapshot: { ...first, id: 'request-2', number: 2 }, persisted: true });
    c.onDelta('Second.', 'First.Second.');
    expect(message().processSteps.map((s: any) => s.requestId)).toEqual(['request-1', 'request-2']);
    expect(message().requestSnapshots).toHaveLength(2);
    expect(message().requestSnapshots[0]).toMatchObject({ systemPrompt: 'initial', persisted: true });
  });
  it('records ordered tool boundaries and final parameters/results', () => {
    const { callbacks: c, message } = harness();
    c.onThinking('check', 'check');
    c.onDelta('Before.', 'Before.');
    c.onToolUse({ type: 'start', tool_use_id: 'call', tool_name: 'pwsh', tool_input: {} });
    c.onToolUse({ type: 'input', tool_use_id: 'call', tool_input: { command: 'Get-Date' } });
    c.onToolUse({ type: 'done', tool_use_id: 'call', tool_name: 'pwsh', content: 'today', is_error: false });
    c.onDelta('After.', 'Before.After.');
    c.onDoneSimple('Before.After.');
    expect(message().processSteps.map((s: any) => s.kind)).toEqual(['thinking', 'text', 'tool', 'text']);
    expect(message().toolCalls[0]).toMatchObject({ id: 'call', input: { command: 'Get-Date' }, result: 'today', status: 'done' });
    expect(message().processSteps.every((s: any) => s.endedAt != null)).toBe(true);
    expect(message().processSteps.filter((s: any) => s.kind === 'text').map((s: any) => s.text).join('')).toBe('Before.After.');
  });
  it('preserves visible errors and correctly marks a result received without its start', () => {
    const { callbacks: c, message } = harness();
    c.onDelta('Before.', 'Before.');
    c.onToolUse({ type: 'done', tool_use_id: 'late', tool_name: 'pwsh', tool_input: { command: 'fail' }, content: 'failed', is_error: true });
    c.onError('connection lost');
    expect(message().toolCalls[0]).toMatchObject({ status: 'error', input: { command: 'fail' } });
    expect(message().processSteps.at(-1).text).toContain('Error: connection lost');
    expect(message().content).toContain('Before.');
    expect(message().processEndedAt).toBeGreaterThan(0);
  });
});
