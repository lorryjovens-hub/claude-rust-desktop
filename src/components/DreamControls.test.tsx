import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import DreamControls from './DreamControls';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const snapshot = { config: { enabled: true, provider: 'saved', model: 'm', min_hours: 24, min_memories: 5 }, memory: { pending: 3, last_run: null, runs: 0 }, services: [] };
beforeEach(() => { vi.resetAllMocks(); vi.mocked(invoke).mockResolvedValue(snapshot); });
it('requires saving edited model configuration before starting and exposes backend failures', async () => {
  render(<DreamControls onComplete={vi.fn()} />);
  await waitFor(() => expect(screen.getByLabelText('整理模型')).toHaveValue('m'));
  fireEvent.change(screen.getByLabelText('整理模型'), { target: { value: 'other' } });
  expect(screen.getByText('立即整理')).toBeDisabled();
  fireEvent.click(screen.getByText('保存整理设置'));
  await waitFor(() => expect(screen.getByText('立即整理')).not.toBeDisabled());
  expect(invoke).toHaveBeenCalledWith('peripheral_command', { request: { action: 'dream_config', config: { ...snapshot.config, model: 'other' } } });
  vi.mocked(invoke).mockRejectedValueOnce('Provider unavailable');
  fireEvent.click(screen.getByText('立即整理'));
  expect(await screen.findByRole('alert')).toHaveTextContent('Provider unavailable');
});
it('stops the actual shared automatic host', async () => {
  vi.mocked(invoke).mockResolvedValue({ ...snapshot, services: [{ name: 'auto-dream', state: 'running', error: null }] });
  render(<DreamControls onComplete={vi.fn()} />);
  await waitFor(() => expect(screen.getByText('停止自动整理')).not.toBeDisabled());
  fireEvent.click(screen.getByText('停止自动整理'));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('peripheral_command', { request: { action: 'dream_stop', automatic: true } }));
});
