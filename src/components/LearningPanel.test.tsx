import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import LearningPanel from './LearningPanel';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

it('shows opt-in state and sends feedback without executing suggested tools', async () => {
  vi.mocked(invoke).mockImplementation(async (_cmd, args) => {
    const request = (args as { request: { action: string } }).request;
    if (request.action === 'learning_status') return { enabled: false, experiences: [{ id: 'e1', tools_used: ['Read'], success: false, feedback: null }], suggestions: [] };
    if (request.action === 'performance_status') return { enabled: false, provider_id: '', model_id: '' };
    return {};
  });
  render(<LearningPanel />);
  await screen.findByText(/已记录 1 次/);
  fireEvent.click(screen.getByText('启用学习记录'));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('peripheral_command', { request: { action: 'learning_configure', enabled: true } }));
  await screen.findByRole('status');
  fireEvent.click(screen.getByText('无帮助'));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('peripheral_command', { request: { action: 'learning_feedback', id: 'e1', rating: 0 } }));
  expect(vi.mocked(invoke).mock.calls.every(([name]) => name === 'peripheral_command')).toBe(true);
});
