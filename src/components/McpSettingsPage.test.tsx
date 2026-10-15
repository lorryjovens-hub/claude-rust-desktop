import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import McpSettingsPage from './McpSettingsPage';

beforeEach(() => vi.resetAllMocks());
it('preserves imported arguments, working directory and disabled state when edited', async () => {
  const server = { id: 'old', name: 'old', command: 'node', args: ['D:/path with spaces/server.cjs', '', 'quoted "text"'], cwd: 'D:/work space', env: {}, enabled: false, running: false, tools_count: 0, resources_count: 0, transport_type: 'stdio' };
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'mcp_list_servers' ? [server] : cmd === 'mcp_list_tools' ? [] : undefined);
  render(<McpSettingsPage />);
  fireEvent.click(await screen.findByTitle('Edit'));
  expect(screen.getByLabelText('MCP 工作目录')).toHaveValue(server.cwd);
  fireEvent.click(screen.getByRole('button', { name: 'Update' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('mcp_update_server', { id: 'old', config: { id: 'old', name: 'old', command: 'node', args: server.args, cwd: server.cwd, env: {}, enabled: false, transport_type: 'stdio' } }));
});
