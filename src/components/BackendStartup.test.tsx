import { useEffect } from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import BackendStartup from './BackendStartup';
import { waitForApiReady } from '../services/bridgeConnection';

vi.mock('../services/bridgeConnection', () => ({ waitForApiReady: vi.fn() }));
beforeEach(() => vi.clearAllMocks());

it('does not mount business components or initialize settings until the backend is ready', async () => {
  let connect!: () => void;
  vi.mocked(waitForApiReady).mockReturnValue(new Promise(resolve => { connect = resolve; }));
  const initialize = vi.fn().mockResolvedValue(undefined);
  const loadProjects = vi.fn();
  function BusinessPage() {
    useEffect(() => { loadProjects(); }, []);
    return <div>项目列表</div>;
  }
  render(<BackendStartup initialize={initialize}><BusinessPage /></BackendStartup>);
  expect(screen.getByRole('status')).toHaveTextContent('后端启动中');
  expect(initialize).not.toHaveBeenCalled();
  expect(loadProjects).not.toHaveBeenCalled();
  await act(async () => { connect(); });
  expect(initialize).toHaveBeenCalledOnce();
  expect(loadProjects).toHaveBeenCalledOnce();
  expect(screen.getByText('项目列表')).toBeInTheDocument();
});

it('keeps the app blocked on failure and allows retrying without reloading', async () => {
  vi.mocked(waitForApiReady).mockRejectedValueOnce(new Error('等待后端启动超时')).mockResolvedValueOnce(undefined);
  const initialize = vi.fn().mockResolvedValue(undefined);
  render(<BackendStartup initialize={initialize}><div>项目列表</div></BackendStartup>);
  expect(await screen.findByRole('alert')).toHaveTextContent('等待后端启动超时');
  expect(screen.queryByText('项目列表')).not.toBeInTheDocument();
  expect(initialize).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: '重试' }));
  expect(await screen.findByText('项目列表')).toBeInTheDocument();
});
