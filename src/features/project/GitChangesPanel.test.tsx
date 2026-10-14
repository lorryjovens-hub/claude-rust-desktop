import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import GitChangesPanel, { parsePatch } from './GitChangesPanel';
import ConversationWorkspaceBar from './ConversationWorkspaceBar';
import { getConversationDiff, openConversationWorkspace, type RepositoryChanges } from '../../api/git';
import { useUIStore } from '../../stores/useUIStore';
vi.mock('../../api/git', () => ({ getConversationDiff: vi.fn(), openConversationWorkspace: vi.fn() }));
vi.mock('../../api', () => ({ getProject: vi.fn().mockResolvedValue({ name: 'Demo project' }) }));
vi.mock('./ProjectWorkspaceBar', () => ({ default: () => <div>Project picker</div> }));
const data: RepositoryChanges = { workspace_path: 'C:/demo', is_repository: true, branch: 'main', github_url: 'https://github.com/a/b', compare_url: 'https://github.com/a/b/compare/main', added: 2, removed: 1, files: [
  { path: 'src/main.ts', added: 2, removed: 1, binary: false, status: 'modified' },
  { path: 'image.png', added: 0, removed: 0, binary: true, status: 'untracked' },
] };
beforeEach(() => { vi.clearAllMocks(); useUIStore.setState({ language: 'en' }); vi.mocked(getConversationDiff).mockResolvedValue({ patch: '@@ -1 +1 @@\n-old\n+new\n', notice: null }); });
it('lists every file, lazily opens diffs, filters and closes the sidebar', async () => {
  const close = vi.fn();
  render(<GitChangesPanel id="session" data={data} error="" onClose={close} onRefresh={vi.fn()} />);
  expect(await screen.findByText('+new')).toBeInTheDocument();
  expect(screen.getByText('image.png')).toBeInTheDocument();
  expect(getConversationDiff).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole('button', { name: /image.png/ }));
  await waitFor(() => expect(screen.getAllByText('+new')).toHaveLength(2));
  expect(getConversationDiff).toHaveBeenCalledWith('session', 'image.png');
  fireEvent.change(screen.getByRole('textbox'), { target: { value: 'image' } });
  expect(screen.queryByText('main.ts')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Close' }));
  expect(close).toHaveBeenCalledOnce();
});
it('numbers added and removed lines independently', () => {
  expect(parsePatch('@@ -5,2 +9,2 @@\n-old\n+new\n same\n').slice(1)).toEqual([
    { text: '-old', kind: 'removed', oldLine: '5', newLine: '' },
    { text: '+new', kind: 'added', oldLine: '', newLine: '9' },
    { text: ' same', kind: 'context', oldLine: '6', newLine: '10' },
  ]);
});
it('opens changes and uses the conversation workspace for menu actions', async () => {
  const view = vi.fn();
  render(<ConversationWorkspaceBar id="session" projectId="project" data={data} error="" disabled={false} onSelect={vi.fn()} onViewChanges={view} onRefresh={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: 'View changes' }));
  expect(view).toHaveBeenCalledOnce();
  fireEvent.click(await screen.findByRole('button', { name: 'Demo project' }));
  fireEvent.click(screen.getByRole('menuitem', { name: 'Show in Explorer' }));
  expect(openConversationWorkspace).toHaveBeenCalledWith('session', 'explorer');
  fireEvent.click(screen.getByRole('button', { name: 'Branch' }));
  fireEvent.click(screen.getByRole('menuitem', { name: 'Open in terminal' }));
  expect(openConversationWorkspace).toHaveBeenCalledWith('session', 'terminal');
});
