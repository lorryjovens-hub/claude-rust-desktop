import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import AgentLegacyImport from './AgentLegacyImport';

const report = { definition: { id: 'reader', tools: ['Read'], legacy: { name: 'Old Reader', when_to_use: 'Review', model: 'sonnet', base_dir: 'old/project' } }, preview: true, skipped: false, imported: false, backup: null };
beforeEach(() => vi.resetAllMocks());
it('previews mapping, invalidates confirmation after edits and refreshes after import', async () => {
  vi.mocked(invoke).mockResolvedValue(report);
  const refresh = vi.fn().mockResolvedValue(undefined);
  render(<AgentLegacyImport onImported={refresh} />);
  const confirm = screen.getByText('确认导入旧 Agent');
  expect(confirm).toBeDisabled();
  fireEvent.change(screen.getByLabelText('旧 Agent 文件路径'), { target: { value: 'D:/old.json' } });
  fireEvent.change(screen.getByLabelText('导入 Agent ID'), { target: { value: 'reader' } });
  fireEvent.click(screen.getByText('预览旧 Agent'));
  await screen.findByText('允许工具：Read');
  await waitFor(() => expect(confirm).toBeEnabled());
  expect(invoke).toHaveBeenCalledWith('peripheral_command', { request: { action: 'agent_import_legacy', source: 'D:/old.json', id: 'reader', preview: true } });
  fireEvent.change(screen.getByLabelText('导入 Agent ID'), { target: { value: 'reader2' } });
  expect(confirm).toBeDisabled();
  fireEvent.click(screen.getByText('预览旧 Agent'));
  await waitFor(() => expect(confirm).toBeEnabled());
  vi.mocked(invoke).mockResolvedValue({ ...report, preview: false, imported: true, backup: 'backup.sqlite3' });
  fireEvent.click(confirm);
  await screen.findByText('备份：backup.sqlite3');
  expect(refresh).toHaveBeenCalledOnce();
});
it('reports unsupported tools and prevents confirmation', async () => {
  vi.mocked(invoke).mockRejectedValue(new Error('Unsupported legacy tool'));
  render(<AgentLegacyImport onImported={vi.fn()} />);
  fireEvent.change(screen.getByLabelText('旧 Agent 文件路径'), { target: { value: 'D:/old.json' } });
  fireEvent.change(screen.getByLabelText('导入 Agent ID'), { target: { value: 'reader' } });
  fireEvent.click(screen.getByText('预览旧 Agent'));
  expect(await screen.findByRole('alert')).toHaveTextContent('Unsupported legacy tool');
  expect(screen.getByText('确认导入旧 Agent')).toBeDisabled();
});
