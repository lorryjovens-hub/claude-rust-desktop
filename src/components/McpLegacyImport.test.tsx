import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import McpLegacyImport from './McpLegacyImport';

beforeEach(() => vi.resetAllMocks());
it('requires a preview and invalidates it when the source changes', async () => {
  vi.mocked(invoke).mockResolvedValue({ imported: ['fixture'], skipped: [], preview: true, backup: null });
  const refresh = vi.fn().mockResolvedValue(undefined);
  render(<McpLegacyImport onImported={refresh} />);
  const input = screen.getByLabelText('旧 MCP 配置路径');
  const confirm = screen.getByText('确认导入 MCP');
  expect(confirm).toBeDisabled();
  fireEvent.change(input, { target: { value: 'D:/old/settings.json' } });
  fireEvent.click(screen.getByText('预览 MCP 导入'));
  await waitFor(() => expect(confirm).toBeEnabled());
  expect(invoke).toHaveBeenCalledWith('mcp_import_legacy', { source: 'D:/old/settings.json', preview: true });
  fireEvent.change(input, { target: { value: 'D:/other/settings.json' } });
  expect(confirm).toBeDisabled();
  fireEvent.click(screen.getByText('预览 MCP 导入'));
  await waitFor(() => expect(confirm).toBeEnabled());
  vi.mocked(invoke).mockResolvedValue({ imported: ['fixture'], skipped: [], preview: false, backup: 'D:/backup.json' });
  fireEvent.click(confirm);
  await screen.findByText('备份：D:/backup.json');
  expect(refresh).toHaveBeenCalledOnce();
});

it('shows conflicts and requires another preview after failure', async () => {
  vi.mocked(invoke).mockRejectedValue(new Error('MCP server fixture conflicts'));
  render(<McpLegacyImport onImported={vi.fn()} />);
  fireEvent.change(screen.getByLabelText('旧 MCP 配置路径'), { target: { value: 'D:/old.json' } });
  fireEvent.click(screen.getByText('预览 MCP 导入'));
  expect(await screen.findByRole('alert')).toHaveTextContent('conflicts');
  expect(screen.getByText('确认导入 MCP')).toBeDisabled();
});
