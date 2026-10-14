import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryPanel } from './MemoryPanel';
import { memoryApi } from '../api/memory';

vi.mock('../api/memory', () => ({ memoryApi: { stats: vi.fn(), config: vi.fn(), search: vi.fn(), ingest: vi.fn(), saveConfig: vi.fn(), clear: vi.fn() } }));

const config = { enabled: true, backend_url: 'local', auto_search: true, auto_ingest: true, search_top_k: 5, compression_threshold_tokens: 2500000, min_importance_threshold: 0.3 };

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(memoryApi.stats).mockResolvedValue({ total_memories: 1, total_tokens_approx: 12, backend: 'Local SQLite' });
  vi.mocked(memoryApi.config).mockResolvedValue(config);
  vi.mocked(memoryApi.search).mockResolvedValue([{ id: 'one', content: 'Use PostgreSQL', importance: 0.7, created_at: '2026-10-14T00:00:00Z' }]);
});

describe('long-term memory management', () => {
  it('loads saved memories and adds a manual memory', async () => {
    render(<MemoryPanel />);
    await screen.findByText('Use PostgreSQL');
    fireEvent.click(screen.getByText('添加记忆'));
    fireEvent.change(screen.getByLabelText('记忆内容'), { target: { value: 'Prefer Chinese replies' } });
    fireEvent.click(screen.getByText('保存记忆'));
    await waitFor(() => expect(memoryApi.ingest).toHaveBeenCalledWith('Prefer Chinese replies', 0.7));
    expect(await screen.findByRole('status')).toHaveTextContent('记忆已保存');
  });

  it('keeps existing memories visible and reports a failed clear', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    vi.mocked(memoryApi.clear).mockRejectedValue(new Error('Storage is unavailable'));
    render(<MemoryPanel />);
    await screen.findByText('Use PostgreSQL');
    fireEvent.click(screen.getByLabelText('清空记忆'));
    expect(await screen.findByRole('alert')).toHaveTextContent('Storage is unavailable');
    expect(screen.getByText('Use PostgreSQL')).toBeInTheDocument();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('saves disabled memory settings instead of only changing UI state', async () => {
    render(<MemoryPanel />);
    await screen.findByText('Use PostgreSQL');
    fireEvent.click(screen.getByLabelText('记忆设置'));
    fireEvent.click(screen.getByLabelText('启用长期记忆'));
    fireEvent.click(screen.getByText('保存设置'));
    await waitFor(() => expect(memoryApi.saveConfig).toHaveBeenCalledWith({ ...config, enabled: false }));
  });
});
