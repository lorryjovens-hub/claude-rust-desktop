import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getProjects } from '../../api';
import { useProjectStore } from '../../stores/useProjectStore';
import { useWorkspaceModeStore } from '../../stores/useWorkspaceModeStore';
import { useLastCodeProject } from './useLastCodeProject';

vi.mock('../../api', () => ({ getProjects: vi.fn() }));

const project = {
  id: 'last-code-project', name: 'Example', description: '', instructions: '',
  workspace_path: 'C:\\work\\example', folders: [{ path: 'C:\\work\\example', trusted: false }],
  is_archived: 0, created_at: '', updated_at: '',
};

beforeEach(() => {
  vi.resetAllMocks();
  localStorage.clear();
  useProjectStore.getState().resetProject();
  useWorkspaceModeStore.setState({ mode: 'code', lastCodeProjectId: project.id });
  vi.mocked(getProjects).mockResolvedValue([project]);
});

describe('last Code project', () => {
  it('restores the saved project on each new chat without granting folder trust', async () => {
    renderHook(() => useLastCodeProject(null));
    await waitFor(() => expect(useProjectStore.getState().pendingProjectId).toBe(project.id));
    act(() => { useProjectStore.getState().setPendingProjectId(null); });
    await waitFor(() => expect(useProjectStore.getState().pendingProjectId).toBe(project.id));
    expect(project.folders[0].trusted).toBe(false);
  });

  it.each(['chat', 'work', 'study'] as const)('does not apply the Code default in %s mode', mode => {
    useWorkspaceModeStore.setState({ mode });
    renderHook(() => useLastCodeProject(null));
    expect(getProjects).not.toHaveBeenCalled();
    expect(useProjectStore.getState().pendingProjectId).toBeNull();
  });

  it('does not change the project of an existing conversation', () => {
    useProjectStore.getState().setCurrentProjectId('conversation-project');
    renderHook(() => useLastCodeProject('conversation-1'));
    expect(getProjects).not.toHaveBeenCalled();
    expect(useProjectStore.getState().currentProjectId).toBe('conversation-project');
  });

  it.each([{ projects: [] }, { projects: [{ ...project, is_archived: 1 }] }])('forgets a deleted or archived project', async ({ projects }) => {
    vi.mocked(getProjects).mockResolvedValue(projects);
    renderHook(() => useLastCodeProject(null));
    await waitFor(() => expect(useWorkspaceModeStore.getState().lastCodeProjectId).toBeNull());
    expect(useProjectStore.getState().pendingProjectId).toBeNull();
  });

  it('does not replace a manual selection when a delayed lookup completes', async () => {
    let finish!: (projects: typeof project[]) => void;
    vi.mocked(getProjects).mockReturnValue(new Promise(resolve => { finish = resolve; }));
    renderHook(() => useLastCodeProject(null));
    act(() => { useProjectStore.getState().setPendingProjectId('manually-selected'); });
    await act(async () => { finish([project]); });
    expect(useProjectStore.getState().pendingProjectId).toBe('manually-selected');
  });

  it('keeps the saved preference if the backend is unavailable', async () => {
    vi.mocked(getProjects).mockRejectedValue(new Error('offline'));
    await act(async () => { renderHook(() => useLastCodeProject(null)); });
    expect(useWorkspaceModeStore.getState().lastCodeProjectId).toBe(project.id);
    expect(useProjectStore.getState().pendingProjectId).toBeNull();
  });
});
