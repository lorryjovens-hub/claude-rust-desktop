import { extractTextContent } from '../../utils/messageHelpers';

export interface ProcessStep {
  id: string;
  kind: 'text' | 'thinking' | 'tool';
  text?: string;
  toolId?: string;
  startedAt?: number;
  endedAt?: number;
  requestId?: string;
}
export interface RequestSnapshot {
  id: string;
  source?: 'native_engine' | 'conversation' | 'web_search';
  number: number;
  startedAt: number;
  model: string;
  systemPrompt: string | null;
  systemMessages: { role: string; content: unknown }[];
  tools: { name: string; description: string; input_schema: unknown }[];
  persisted?: boolean;
  status?: 'running' | 'completed' | 'error' | 'interrupted';
  providerId?: string;
  providerName?: string;
  apiFormat?: string;
  responseModel?: string;
  responseId?: string | null;
  endedAt?: number | null;
  firstTokenAt?: number | null;
  totalDurationMs?: number | null;
  ttftMs?: number | null;
  generationMs?: number | null;
  throughputTokensPerSecond?: number | null;
  inputTokens?: number | null;
  outputTokens?: number | null;
  reasoningTokens?: number | null;
  contentTokens?: number | null;
  totalTokens?: number | null;
  cacheReadTokens?: number | null;
  cacheWriteTokens?: number | null;
  maxOutputTokens?: number | null;
  contextWindow?: number | null;
  reservedForCompletion?: number | null;
  effort?: string | null;
  finishReason?: string | null;
  timingSource?: string;
  error?: string | null;
  responseBlocks?: ResponseBlock[];
  usage?: Record<string, unknown>;
}
export interface ResponseBlock {
  type: string;
  text?: string;
  id?: string;
  name?: string;
  input?: unknown;
  arguments?: string;
  [key: string]: unknown;
}
export interface TraceRecord extends ProcessStep {
  turn: number;
  step: number;
  role: string;
  title: string;
  status: string;
  input?: unknown;
  result?: unknown;
  schema?: unknown;
  request?: RequestSnapshot;
  blocks?: ResponseBlock[];
  aliases?: string[];
}
export const rawText = (value: unknown): string => value == null ? '' : typeof value === 'string' ? value : JSON.stringify(value, null, 2);
export function toolInput(tool: any): any {
  if (typeof tool.input !== 'string') return tool.input || {};
  try { return JSON.parse(tool.input); } catch { return { command: tool.input }; }
}
export function toolCommand(tool: any): string {
  const input = toolInput(tool);
  return rawText(input.command ?? input.cmd ?? input.script ?? input.code ?? tool.input);
}
export function commandStreams(value: unknown): { stdout: string; stderr: string } | null {
  let result: any = value;
  if (typeof result === 'string') {
    try { result = JSON.parse(result); } catch { return null; }
  }
  if (!result || typeof result !== 'object' || Array.isArray(result)
    || !['stdout', 'stderr', 'exit_code', 'exitCode', 'shell', 'error'].some(key => key in result)) return null;
  const format = (stream: unknown) => rawText(stream).replace(/\r\n?/g, '\n');
  return { stdout: format(result.stdout), stderr: format(result.stderr || result.error) };
}
export function toolOutput(tool: any): string {
  const result = tool.result ?? tool.output ?? tool.error;
  const webContent = webFetchContent(tool.name || '', result);
  if (webContent !== null) return webContent;
  const streams = toolCategory(tool.name || '') === 'Ran commands' ? commandStreams(result) : null;
  return streams ? [streams.stdout, streams.stderr].filter(Boolean).join('\n') : rawText(result);
}
/** Successful fetches display/copy the readable page, not transport metadata. */
export function webFetchContent(name: string, value: unknown): string | null {
  if (!/^web_?fetch$/i.test(name)) return null;
  let result: any = value;
  if (typeof result === 'string') {
    try { result = JSON.parse(result); } catch { return null; }
  }
  if (!result || typeof result !== 'object' || result.success === false || result.is_error === true || result.isError === true
    || result.error || Number(result.status_code ?? result.status) >= 400) return null;
  return typeof result.content === 'string' || Array.isArray(result.content) ? extractTextContent(result.content) : null;
}
export function requestSystemPrompt(request: RequestSnapshot): string {
  const contentText = (value: unknown): string => Array.isArray(value)
    ? value.map(block => block?.type === 'text' ? block.text || '' : '').filter(Boolean).join('\n')
    : extractTextContent(value);
  return [request.systemPrompt, ...(request.systemMessages || []).map(message => contentText(message.content))].filter(Boolean).join('\n\n');
}
export function toolStatus(tool: any): string {
  if (tool.status === 'error' || tool.is_error === true || tool.isError === true) return 'error';
  if (tool.status === 'running') return 'running';
  let result = tool.result ?? tool.output;
  if (typeof result === 'string') {
    try { result = JSON.parse(result); } catch { result = null; }
  }
  if (result?.is_error === true || result?.isError === true || result?.success === false) return 'error';
  if (toolCategory(tool.name || '') === 'Ran commands') {
    const exitCode = result?.exit_code ?? result?.exitCode;
    if (exitCode != null && exitCode !== '' && Number.isFinite(Number(exitCode)) && Number(exitCode) !== 0) return 'error';
  }
  return tool.status || 'done';
}
export function toolCategory(name: string): string {
  if (/bash|pwsh|powershell|shell|exec_command|terminal/i.test(name)) return 'Ran commands';
  if (/glob|list|find/i.test(name)) return 'Found files';
  if (/grep|search/i.test(name)) return 'Searched';
  if (/read/i.test(name)) return 'Read files';
  if (/edit|write|patch/i.test(name)) return 'Edited files';
  return name;
}
export function finishProcess(message: any, now = Date.now()) {
  const steps: ProcessStep[] = message.processSteps || [];
  for (const s of steps) if (s.endedAt == null) s.endedAt = now;
  message.processEndedAt = now;
}
/** Capture ordered blocks at stream boundaries, independent of their presentation. */
export function recordText(message: any, kind: 'text' | 'thinking', full: string, now = Date.now(), streamedDelta?: string) {
  const offsetKey = kind === 'text' ? '_traceTextLength' : '_traceThinkingLength';
  const previous = message[offsetKey] ?? (message.processSteps || []).filter((s: ProcessStep) => s.kind === kind).reduce((n: number, s: ProcessStep) => n + (s.text?.length || 0), 0);
  const delta = streamedDelta ?? full.slice(previous <= full.length ? previous : 0);
  message[offsetKey] = full.length;
  if (!delta) return;
  const steps: ProcessStep[] = message.processSteps ||= [];
  const last = steps[steps.length - 1];
  if (last?.kind === kind && last.endedAt == null) last.text = (last.text || '') + delta;
  else {
    if (last && last.kind !== 'tool' && last.endedAt == null) last.endedAt = now;
    steps.push({ id: crypto.randomUUID(), kind, text: delta, startedAt: now, requestId: message.currentRequestId });
  }
  message.processStartedAt ??= now;
}
export function recordTool(message: any, tool: any, now = Date.now()) {
  const steps: ProcessStep[] = message.processSteps ||= [];
  if (steps.some(s => s.toolId === tool.id)) return;
  const last = steps[steps.length - 1];
  if (last && last.kind !== 'tool' && last.endedAt == null) last.endedAt = now;
  steps.push({ id: `tool:${tool.id}`, kind: 'tool', toolId: tool.id, startedAt: tool.startedAt, requestId: message.currentRequestId });
  message.processStartedAt ??= now;
}
export function messageSteps(message: any): ProcessStep[] {
  const recorded: ProcessStep[] = message.processSteps || [];
  // Auxiliary search completions belong to the tool, not the visible answer.
  const snapshots: RequestSnapshot[] = (message.requestSnapshots || []).filter((request: RequestSnapshot) => request.source !== 'web_search');
  if (snapshots.some(request => request.responseBlocks?.length)) {
    const usedTools = new Set<number>();
    const reconstructed = snapshots.flatMap(request => request.responseBlocks?.length ? request.responseBlocks.flatMap<ProcessStep>((block, index) => {
      const base = { requestId: request.id, startedAt: request.firstTokenAt ?? request.startedAt, endedAt: request.endedAt ?? undefined };
      if (block.type === 'tool_call') {
        const tools = message.toolCalls || [];
        const exactIndex = tools.findIndex((tool: any, i: number) => !usedTools.has(i) && tool.id === block.id);
        const toolIndex = exactIndex >= 0 ? exactIndex : tools.findIndex((tool: any, i: number) => !usedTools.has(i)
          && tool.name === block.name && canonical(toolInput(tool)) === canonical(block.input));
        if (toolIndex < 0) return [];
        usedTools.add(toolIndex);
        const tool = message.toolCalls[toolIndex];
        return [{ ...base, id: `tool:${tool.id || toolIndex}`, kind: 'tool', toolId: tool.id || String(toolIndex) }];
      }
      if (block.type !== 'text' && block.type !== 'thinking') return [];
      return [{ ...base, id: `response:${request.id}:block:${index}`, kind: block.type === 'thinking' ? 'thinking' : 'text', text: block.text || '' }];
    }) : recorded.filter(step => step.requestId === request.id));
    const full = extractTextContent(message.content);
    const matches = (steps: ProcessStep[]) => {
      const text = steps.filter(step => step.kind === 'text').map(step => step.text || '').join('');
      const toolIds = new Set(steps.filter(step => step.kind === 'tool').map(step => step.toolId));
      return text === full && (message.toolCalls || []).every((tool: any, index: number) => toolIds.has(tool.id || String(index)));
    };
    // Provider response blocks retain the true text/tool boundaries. Older local
    // events may have recorded buffered preamble text after its tool. Prefer the
    // complete authoritative order, while retaining live steps for running requests.
    // Matching visible text prevents reintroducing filtered inline artifacts.
    if (matches(reconstructed)) return reconstructed;
    reconstructed.push(...recorded.filter(step => !snapshots.some(request => request.id === step.requestId)));
    if (matches(reconstructed)) return reconstructed;
    const snapshotText = reconstructed.filter(step => step.kind === 'text').map(step => step.text || '').join('');
    if (!recorded.length && full.startsWith(snapshotText)) {
      const tail = full.slice(snapshotText.length);
      if (tail) reconstructed.push({ id: 'live-answer', kind: 'text', text: tail, requestId: snapshots[snapshots.length - 1]?.id });
      if (matches(reconstructed)) return reconstructed;
    }
  }
  if (recorded.length) return recorded;
  const steps: ProcessStep[] = [];
  if (message.thinking) steps.push({ id: 'thinking', kind: 'thinking', text: message.thinking });
  const tools = message.toolCalls || [];
  let prefix = '';
  tools.forEach((tool: any, i: number) => {
    if (tool.textBefore) {
      // Native tool events carry a cumulative prefix; legacy events may instead
      // contain only the commentary since the previous tool.
      const text = tool.textBefore.startsWith(prefix) ? tool.textBefore.slice(prefix.length) : tool.textBefore;
      if (text) steps.push({ id: `before:${i}`, kind: 'text', text });
      prefix += text;
    }
    steps.push({ id: `tool:${tool.id || i}`, kind: 'tool', toolId: tool.id || String(i), startedAt: tool.startedAt, endedAt: tool.endedAt });
  });
  const full = extractTextContent(message.content);
  const offset = message.toolTextEndOffset;
  const final = offset > 0 && offset <= full.length ? full.slice(offset) : prefix && full.startsWith(prefix) ? full.slice(prefix.length) : full;
  if (final) steps.push({ id: 'answer', kind: 'text', text: final });
  return steps;
}
export const recordId = (messageIndex: number, stepId: string) => `message:${messageIndex}/${stepId}`;
export function buildTrace(messages: any[], loading = false): TraceRecord[] {
  let turn = 0;
  let step = 0;
  let previousSystemContext: string | undefined;
  const records = messages.flatMap<TraceRecord>((message, i) => {
    if (message.role === 'user') { turn++; step = 0; }
    const running = loading && i === messages.length - 1;
    if (message.role !== 'assistant') return [{
      id: recordId(i, 'message'), kind: 'text' as const, text: extractTextContent(message.content),
      role: message.role || 'context', turn, step: 0, title: message.role || 'context', status: 'done',
      startedAt: message.created_at && Number.isFinite(Date.parse(message.created_at)) ? Date.parse(message.created_at) : undefined,
    }];
    const snapshots: RequestSnapshot[] = message.requestSnapshots || [];
    let steps: TraceRecord[] = messageSteps(message).map((s) => {
      const tool = s.kind === 'tool' ? (message.toolCalls || []).find((t: any, k: number) => (t.id || String(k)) === s.toolId) : null;
      const request = snapshots.find(r => r.id === s.requestId) ?? (snapshots.length === 1 ? snapshots[0] : undefined);
      const status = tool ? toolStatus(tool) : undefined;
      return {
        ...s, id: recordId(i, s.id), turn, step: 0, role: tool ? 'tool' : 'assistant', request,
        title: tool?.name || (s.kind === 'thinking' ? 'Think' : 'Assistant'),
        status: status ? status === 'running' && !running ? 'interrupted' : status : running && !s.endedAt ? 'running' : 'done',
        startedAt: tool?.startedAt ?? s.startedAt, endedAt: tool?.endedAt ?? s.endedAt,
        input: tool?.input, result: tool?.result ?? tool?.output, schema: tool?.schema ?? request?.tools.find(t => t.name === tool?.name)?.input_schema,
      };
    });
    if (snapshots.length) {
      const claimedTools = new Set<string>();
      const responses = snapshots.flatMap<TraceRecord>(request => {
        const related = steps.filter(s => s.request?.id === request.id);
        const blocks: ResponseBlock[] = request.responseBlocks?.length ? request.responseBlocks : related.map(s => s.role === 'tool'
          ? { type: 'tool_call', id: s.toolId, name: s.title, input: s.input }
          : { type: s.kind === 'thinking' ? 'thinking' : 'text', text: s.text || '' });
        const assistant: TraceRecord = {
          id: recordId(i, `response:${request.id}`), kind: 'text', role: 'assistant', turn, step: 0,
          title: request.source === 'web_search' ? 'WebSearch request' : 'Assistant', text: blocks.filter(block => block.type === 'text').map(block => block.text || '').join(''),
          requestId: request.id, request, blocks, aliases: related.filter(s => s.role === 'assistant').map(s => s.id),
          status: request.status === 'completed' ? 'done' : request.status === 'running' && !running ? 'interrupted' : request.status || (running ? 'running' : 'done'),
          startedAt: request.startedAt, endedAt: request.endedAt ?? undefined,
        };
        // Request snapshots can arrive before tool events, or survive a reload
        // without the tool-result rows. Never drop the calls in those snapshots.
        const calls = blocks.flatMap<TraceRecord>((block, index) => {
          if (block.type !== 'tool_call') return [];
          const candidates = steps.filter(s => s.role === 'tool' && !claimedTools.has(s.id)
            && (!s.request || s.request.id === request.id));
          const input = block.input ?? toolInput({ input: block.arguments });
          const recorded = candidates.find(s => block.id && s.toolId === block.id)
            ?? candidates.find(s => s.title === block.name && canonical(toolInput({ input: s.input })) === canonical(input));
          if (recorded) {
            claimedTools.add(recorded.id);
            return [{ ...recorded, requestId: request.id, request,
              schema: recorded.schema ?? request.tools?.find(tool => tool.name === block.name)?.input_schema }];
          }
          const toolId = block.id || `${request.id}:${index}`;
          return [{ id: recordId(i, `tool:${toolId}`), kind: 'tool', toolId, role: 'tool', turn, step: 0,
            title: block.name || 'Tool call', input, requestId: request.id, request,
            schema: request.tools?.find(tool => tool.name === block.name)?.input_schema,
            status: '未记录结果' }];
        });
        for (const tool of related.filter(s => s.role === 'tool' && !claimedTools.has(s.id))) {
          claimedTools.add(tool.id);
          calls.push(tool);
        }
        return [assistant, ...calls];
      });
      steps = [...responses, ...steps.filter(s => !s.request && !claimedTools.has(s.id))];
    }
    steps = steps.map(s => ({ ...s, step: ++step }));
    const headers: TraceRecord[] = snapshots.flatMap(request => {
      const context = canonical({ systemPrompt: request.systemPrompt, systemMessages: request.systemMessages || [] });
      if (context === previousSystemContext) return [];
      previousSystemContext = context;
      return [{
        id: `request:${request.id}`, kind: 'text' as const, turn, step: 0, role: 'system',
        title: 'System Prompt', status: request.persisted === false ? '未保存' : 'recorded',
        text: requestSystemPrompt(request), input: request.systemMessages, result: request.tools, request,
        startedAt: request.startedAt,
      }];
    });
    if (!steps.some(s => s.requestId)) return [...headers, ...steps];
    const ordered: TraceRecord[] = [];
    const emitted = new Set<string>();
    for (const s of steps) {
      const header = headers.find(h => h.request?.id === s.requestId);
      if (header && !emitted.has(header.id)) { ordered.push(header); emitted.add(header.id); }
      ordered.push(s);
    }
    return [...ordered, ...headers.filter(h => !emitted.has(h.id))];
  });
  if (records.length) {
    const first = records.findIndex(r => r.role === 'system' && (r.request || r.text));
    if (first >= 0) {
      const [initial] = records.splice(first, 1);
      records.unshift({ ...initial, turn: 0, title: 'Initial System Prompt' });
    } else {
      records.unshift({ id: 'system:unrecorded', kind: 'text', role: 'system', turn: 0, step: 0, title: 'Initial System Prompt', status: '未记录' });
    }
  }
  return records;
}

export function processMetadata(message: any) {
  return { processSteps: message.processSteps, processStartedAt: message.processStartedAt,
    processEndedAt: message.processEndedAt, toolTextEndOffset: message.toolTextEndOffset, requestSnapshots: message.requestSnapshots, requestError: message.requestError };
}
function canonical(value: any): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
}
/** Server history remains authoritative; only enrich matching messages with locally captured events. */
export function restoreProcessMetadata(message: any, index: number, cached: any[]) {
  const local = cached.find(m => m.id === message.id) || cached[index];
  if (!local || local.role !== message.role || local.content !== message.content) return message;
  const serverTools = message.toolCalls || [];
  const localTools = local.toolCalls || [];
  const compatible = !serverTools.length || serverTools.length === localTools.length && serverTools.every((t: any, i: number) => t.name === localTools[i].name && canonical(toolInput(t)) === canonical(toolInput(localTools[i])));
  if (!compatible) return message;
  return { ...message, ...processMetadata(local), requestSnapshots: message.requestSnapshots ?? local.requestSnapshots,
    toolCalls: localTools.length ? localTools.map((t: any, i: number) => ({ ...serverTools[i], ...t })) : serverTools };
}
