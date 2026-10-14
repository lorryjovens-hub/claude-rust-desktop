import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import type { RequestSnapshot } from '../trajectory/model';
import { aggregateUsage, contextBreakdown, sessionStatistics } from './model';
import SessionUsageBar from './SessionUsageBar';
import UsagePopover from './UsagePopover';
import TokenUsageDetails from './TokenUsageDetails';
import { useChatStore } from '../../stores/useChatStore';

const request = (override: Partial<RequestSnapshot> = {}): RequestSnapshot => ({ id: 'request-1', number: 1, model: 'model', startedAt: 1000, tools: [], systemPrompt: '', systemMessages: [], ...override });
beforeEach(() => useChatStore.setState({ providersCache: [] }));
describe('usage provider names', () => {
  it('keeps the recorded provider name when the provider was renamed or deleted', () => {
    useChatStore.setState({ providersCache: [{ id: 'provider-uuid', name: 'Renamed provider', enabled: true }] });
    render(<TokenUsageDetails showModels usage={aggregateUsage([{ requestSnapshots: [request({ providerId: 'provider-uuid', providerName: 'DeepSeek Official', responseModel: 'deepseek-flash' })] }])} />);
    expect(screen.getByText('DeepSeek Official / deepseek-flash')).toBeInTheDocument();
    expect(screen.queryByText(/provider-uuid/)).not.toBeInTheDocument();
  });
  it('resolves older snapshots through the provider catalog', () => {
    useChatStore.setState({ providersCache: [{ id: 'provider-uuid', name: 'My provider', enabled: true }] });
    render(<TokenUsageDetails showModels usage={aggregateUsage([{ requestSnapshots: [request({ providerId: 'provider-uuid', model: 'model-v2' })] }])} />);
    expect(screen.getByText('My provider / model-v2')).toBeInTheDocument();
    expect(screen.queryByText(/provider-uuid/)).not.toBeInTheDocument();
  });
  it('shows a meaningful fallback without exposing an unresolved provider UUID', () => {
    render(<TokenUsageDetails showModels usage={aggregateUsage([{ requestSnapshots: [request({ providerId: 'deleted-provider-uuid', model: 'model-v2' })] }])} />);
    expect(screen.getByText('Unknown provider / model-v2')).toBeInTheDocument();
    expect(screen.queryByText(/deleted-provider-uuid/)).not.toBeInTheDocument();
  });
});
describe('session usage', () => {
  it('counts auxiliary search usage and steps without replacing the conversation context', () => {
    const main = request({ inputTokens: 1000, outputTokens: 50, contextWindow: 80000, reservedForCompletion: 8192, systemPrompt: 'Conversation instructions', totalDurationMs: 500 });
    const search = request({ id: 'search', source: 'web_search', apiFormat: 'anthropic', inputTokens: 100, cacheReadTokens: 20, outputTokens: 30, contextWindow: 16000, systemPrompt: 'Search instructions', totalDurationMs: 800 });
    const messages = [{ role: 'user', content: 'Search the web' }, { role: 'assistant', requestSnapshots: [main, { ...search, outputTokens: null }, search] }];
    expect(aggregateUsage(messages)).toMatchObject({ total: 1200, uncachedInput: 1100, cachedInput: 20, output: 80, measuredRequests: 2 });
    expect(sessionStatistics(messages)).toMatchObject({ turns: 1, steps: 2, llmMs: 1300 });
    expect(contextBreakdown(messages)).toMatchObject({ used: 1050, limit: 80000, buffer: 8192, latest: main, system: 'Conversation instructions' });
  });
  it('normalizes Anthropic cache-exclusive input and OpenAI cache-inclusive input', () => {
    const usage = aggregateUsage([{ requestSnapshots: [request({ apiFormat: 'anthropic', inputTokens: 100, cacheReadTokens: 300, cacheWriteTokens: 20, outputTokens: 50 }), request({ id: 'request-2', apiFormat: 'openai', inputTokens: 420, cacheReadTokens: 300, cacheWriteTokens: 20, outputTokens: 50 })] }]);
    expect(usage).toMatchObject({ uncachedInput: 200, cachedInput: 600, cacheWrite: 40, output: 100, total: 940, available: true });
    expect(usage.cacheHit).toBeCloseTo(600 / 840 * 100);
  });
  it('counts a request once after updates and keeps sessions independent', () => {
    expect(aggregateUsage([{ requestSnapshots: [request({ outputTokens: 1 }), request({ outputTokens: 12 })] }]).output).toBe(12);
    expect(aggregateUsage([])).toMatchObject({ total: 0, available: false, cacheHit: null });
  });
  it('uses weighted generation throughput and union of parallel tool times', () => {
    const stats = sessionStatistics([{ role: 'user' }, { role: 'assistant', requestSnapshots: [request({ outputTokens: 100, generationMs: 1000, totalDurationMs: 1500, ttftMs: 500 }), request({ id: 'second', outputTokens: 100, generationMs: 3000, totalDurationMs: 4000, ttftMs: 1000 })], toolCalls: [{ startedAt: 5000, endedAt: 7000 }, { startedAt: 6000, endedAt: 8000 }] }]);
    expect(stats).toMatchObject({ turns: 1, steps: 2, llmMs: 5500, toolMs: 3000, ttftMs: 750, tps: 50 });
  });
  it('does not fabricate missing measurements or cache hit rates', () => {
    expect(sessionStatistics([{ role: 'assistant', requestSnapshots: [request()], toolCalls: [{ name: 'legacy' }] }])).toMatchObject({ llmMs: null, toolMs: null, ttftMs: null, tps: null });
    expect(aggregateUsage([{ requestSnapshots: [request({ inputTokens: 100, outputTokens: 5 })] }]).cacheHit).toBeNull();
  });
});
describe('context window', () => {
  it('estimates conversation context when only an auxiliary search snapshot exists', () => {
    const context = contextBreakdown([{ role: 'user', content: 'hello' }, { role: 'assistant', requestSnapshots: [request({ source: 'web_search', inputTokens: 9999, contextWindow: 80000 })] }]);
    expect(context).toMatchObject({ estimated: true, limit: null, latest: undefined });
    expect(context.used).toBeLessThan(9999);
  });
  it('uses latest context rather than cumulative session input after compaction', () => {
    const context = contextBreakdown([{ requestSnapshots: [request({ inputTokens: 10000, outputTokens: 100 }), request({ id: 'compacted', inputTokens: 900, outputTokens: 100, systemPrompt: '<skills>use this skill</skills>prompt', tools: [{ name: 'mcp__search', description: 'Search', input_schema: {} }, { name: 'Read', description: 'Read file', input_schema: {} }] })] }], 2000);
    expect(context.used).toBe(1000);
    expect(context.percent).toBe(50);
    expect(context.free).toBe(1000);
    expect(context.rows.reduce((sum, row) => sum + row.tokens, 0)).toBe(1000);
    expect(context.rows.find(row => row.label === 'Skills')!.tokens).toBeGreaterThan(0);
    expect(context.tools.filter(t => t.mcp)).toHaveLength(1);
  });
  it('marks estimates and unknown capacity, with finite nonnegative breakdown', () => {
    const estimated = contextBreakdown([{ role: 'user', content: '你好 hello' }]);
    expect(estimated.estimated).toBe(true);
    expect(estimated.percent).toBeNull();
    expect(estimated.limit).toBeNull();
    const small = contextBreakdown([{ requestSnapshots: [request({ inputTokens: 1, systemPrompt: 'system'.repeat(500) })] }], 1);
    expect(small.rows.reduce((sum, row) => sum + row.tokens, 0)).toBe(1);
    expect(small.rows.every(row => row.tokens >= 0)).toBe(true);
  });
  it('uses recorded engine capacity and completion reserve when catalog omits them', () => {
    const context = contextBreakdown([{ requestSnapshots: [request({ inputTokens: 1000, outputTokens: 50, contextWindow: 80000, reservedForCompletion: 8192 })] }]);
    expect(context).toMatchObject({ used: 1050, limit: 80000, buffer: 8192, free: 70758 });
  });
  it('opens all three statistics popovers and expands detailed context', () => {
    render(<SessionUsageBar messages={[{ role: 'user', content: 'hello' }, { role: 'assistant', requestSnapshots: [request({ inputTokens: 100, outputTokens: 20 })] }]} contextLimit={1000} />);
    fireEvent.click(screen.getByRole('button', { name: 'Session statistics' }));
    expect(screen.getByText('Avg time to first token (TTFT)')).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Token usage' }));
    expect(within(screen.getByRole('dialog', { name: 'Token usage' })).getByText('120 tok')).toBeInTheDocument();
    fireEvent.mouseDown(document.body);
    fireEvent.click(screen.getByRole('button', { name: 'Context window' }));
    fireEvent.click(screen.getByRole('button', { name: 'See detailed breakdown' }));
    expect(screen.getByText('System prompt and instructions')).toBeInTheDocument();
  });
  it('keeps an expanding popover inside the available viewport and restores keyboard focus', () => {
    vi.stubGlobal('innerHeight', 850);
    render(<UsagePopover label="Viewport test" trigger="Open"><details><summary>More details</summary><div style={{ height: 900 }}>Long breakdown</div></details></UsagePopover>);
    const trigger = screen.getByRole('button', { name: 'Viewport test' });
    vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue({ top: 450, bottom: 478, left: 100, right: 220, width: 120, height: 28, x: 100, y: 450, toJSON: () => ({}) });
    fireEvent.click(trigger);
    const dialog = screen.getByRole('dialog', { name: 'Viewport test' });
    expect(dialog.style.bottom).toBe('408px');
    expect(dialog.style.maxHeight).toBe('430px');
    expect(dialog).toHaveFocus();
    fireEvent.click(screen.getByText('More details'));
    expect(dialog.style.maxHeight).toBe('430px');
    fireEvent.keyDown(dialog, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: 'Viewport test' })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    vi.unstubAllGlobals();
  });
  it('keeps the panel below the measured application titlebar', () => {
    vi.stubGlobal('innerHeight', 850);
    const titlebar = document.createElement('div');
    titlebar.setAttribute('data-app-titlebar', '');
    document.body.append(titlebar);
    vi.spyOn(titlebar, 'getBoundingClientRect').mockReturnValue({ top: 0, bottom: 44, left: 0, right: 1000, width: 1000, height: 44, x: 0, y: 0, toJSON: () => ({}) });
    render(<UsagePopover label="Titlebar test" trigger="Open">Details</UsagePopover>);
    const trigger = screen.getByRole('button', { name: 'Titlebar test' });
    vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue({ top: 450, bottom: 478, left: 100, right: 220, width: 120, height: 28, x: 100, y: 450, toJSON: () => ({}) });
    fireEvent.click(trigger);
    const dialog = screen.getByRole('dialog', { name: 'Titlebar test' });
    expect(dialog.style.bottom).toBe('408px');
    expect(dialog.style.maxHeight).toBe('386px');
    expect(850 - parseFloat(dialog.style.bottom) - parseFloat(dialog.style.maxHeight)).toBe(56);
    titlebar.remove();
    vi.unstubAllGlobals();
  });
});
