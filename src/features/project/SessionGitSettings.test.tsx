import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SessionGitSettings from './SessionGitSettings';
import { getProjectBranches } from '../../api/projects';
import { useUIStore } from '../../stores/useUIStore';
vi.mock('../../api/projects', () => ({ getProjectBranches: vi.fn() }));
beforeEach(() => { vi.resetAllMocks(); useUIStore.setState({ language: 'en' }); });
const branches = { is_repository: true, current_branch: 'main', branches: ['feature', 'main'] };
describe('new session Git settings', () => {
  it('searches branches and sends branch/worktree choices, resets when the project changes', async () => {
    vi.mocked(getProjectBranches).mockResolvedValue(branches);
    const onChange = vi.fn();
    const { rerender } = render(<SessionGitSettings projectId="one" disabled={false} onChange={onChange} />);
    await screen.findByText('main');
    fireEvent.click(screen.getByRole('button', { name: 'Branch' }));
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'feat' } });
    expect(screen.queryByRole('option', { name: 'main' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('option', { name: 'feature' }));
    fireEvent.click(screen.getByRole('checkbox'));
    expect(onChange).toHaveBeenLastCalledWith({ projectId: 'one', ready: true, branch: 'feature', use_worktree: true });
    rerender(<SessionGitSettings projectId="two" disabled={false} onChange={onChange} />);
    await waitFor(() => expect(onChange).toHaveBeenLastCalledWith({ projectId: 'two', ready: true, branch: 'main', use_worktree: false }));
  });
  it('preserves choices on remount and closes the picker with Escape', async () => {
    vi.mocked(getProjectBranches).mockResolvedValue(branches);
    render(<SessionGitSettings projectId="one" disabled={false} onChange={vi.fn()} initialSelection={{ projectId: 'one', ready: true, branch: 'feature', use_worktree: true }} />);
    await screen.findByText('feature');
    expect(screen.getByRole('checkbox')).toBeChecked();
    fireEvent.click(screen.getByRole('button', { name: 'Branch' }));
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });
  it('disables Git options in non-Git folders without blocking ordinary sessions', async () => {
    vi.mocked(getProjectBranches).mockResolvedValue({ is_repository: false, current_branch: null, branches: [] });
    const onChange = vi.fn();
    render(<SessionGitSettings projectId="one" disabled={false} onChange={onChange} />);
    await screen.findByText('Not a Git repository');
    expect(screen.getByRole('button', { name: 'Branch' })).toBeDisabled();
    expect(screen.getByRole('checkbox')).toBeDisabled();
    expect(onChange).toHaveBeenLastCalledWith({ projectId: 'one', ready: true, branch: undefined, use_worktree: false });
  });
  it('ignores stale responses and lets the user retry an error', async () => {
    let resolveFirst!: (value: any) => void;
    vi.mocked(getProjectBranches).mockImplementationOnce(() => new Promise(resolve => { resolveFirst = resolve; }))
      .mockRejectedValueOnce(new Error('Git unavailable')).mockResolvedValueOnce(branches);
    const onChange = vi.fn();
    const { rerender } = render(<SessionGitSettings projectId="one" disabled={false} onChange={onChange} />);
    rerender(<SessionGitSettings projectId="two" disabled={false} onChange={onChange} />);
    await screen.findByText('Refresh branches');
    fireEvent.click(screen.getByRole('button', { name: 'Branch' }));
    await screen.findByRole('alert');
    resolveFirst(branches);
    expect(onChange).toHaveBeenLastCalledWith({ projectId: 'two', ready: false });
    fireEvent.click(screen.getByRole('button', { name: 'Refresh branches' }));
    await waitFor(() => expect(onChange).toHaveBeenLastCalledWith({ projectId: 'two', ready: true, branch: 'main', use_worktree: false }));
  });
});
