import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import CreateProjectDialog from './CreateProjectDialog';
import ProjectWorkspaceBar from './ProjectWorkspaceBar';
import { createProject, getProject, getProjects, trustProjectFolders, updateProject } from '../../api';
import { useWorkspaceModeStore } from '../../stores/useWorkspaceModeStore';

vi.mock('../../api', () => ({
  createProject: vi.fn(), getProject: vi.fn(), getProjects: vi.fn(),
  trustProjectFolders: vi.fn(), updateProject: vi.fn(),
}));

const project = {
  id: 'project-1', name: 'Example', description: '', instructions: '', workspace_path: 'C:\\work\\first',
  folders: [{ path: 'C:\\work\\first', trusted: false }], is_archived: 0, created_at: '', updated_at: '',
};

beforeEach(() => {
  vi.resetAllMocks();
  useWorkspaceModeStore.setState({ mode: 'chat', lastCodeProjectId: null });
  vi.mocked(getProjects).mockResolvedValue([project]);
  vi.mocked(getProject).mockResolvedValue(project);
});

describe('project creation and trust', () => {
  it('requires a name and folder, creates before requesting consent, and does not trust on cancel', async () => {
    const onCreated = vi.fn();
    const onClose = vi.fn();
    vi.mocked(createProject).mockResolvedValue(project);
    render(<CreateProjectDialog onCreated={onCreated} onClose={onClose} />);
    const submit = screen.getByRole('button', { name: 'Create project' });
    expect(submit).toBeDisabled();
    fireEvent.change(screen.getByLabelText('Project name'), { target: { value: 'Example' } });
    expect(submit).toBeDisabled();
    fireEvent.change(screen.getByLabelText('Folder path'), { target: { value: 'C:\\work\\first' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    fireEvent.click(submit);
    await screen.findByText('Trust this folder?');
    expect(createProject).toHaveBeenCalledWith('Example', '', ['C:\\work\\first']);
    expect(trustProjectFolders).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onClose).toHaveBeenCalledOnce();
    expect(trustProjectFolders).not.toHaveBeenCalled();
  });

  it('selects the project only after explicit trust succeeds', async () => {
    const onCreated = vi.fn();
    const trusted = { ...project, folders: [{ ...project.folders[0], trusted: true }] };
    vi.mocked(createProject).mockResolvedValue(project);
    vi.mocked(trustProjectFolders).mockResolvedValue(trusted);
    render(<CreateProjectDialog onCreated={onCreated} onClose={vi.fn()} />);
    fireEvent.change(screen.getByLabelText('Project name'), { target: { value: 'Example' } });
    fireEvent.change(screen.getByLabelText('Folder path'), { target: { value: project.workspace_path } });
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    fireEvent.click(screen.getByRole('button', { name: 'Create project' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Trust folder' }));
    await waitFor(() => expect(onCreated).toHaveBeenCalledWith(trusted));
  });
});

describe('chat project folders', () => {
  it('remembers the displayed Code project for the next new chat', async () => {
    useWorkspaceModeStore.getState().setMode('code');
    render(<ProjectWorkspaceBar projectId={project.id} onSelect={vi.fn()} />);
    await waitFor(() => expect(useWorkspaceModeStore.getState().lastCodeProjectId).toBe(project.id));
  });

  it('prevents removing the last folder', async () => {
    render(<ProjectWorkspaceBar projectId={project.id} onSelect={vi.fn()} />);
    expect(await screen.findByRole('button', { name: `Remove folder ${project.workspace_path}` })).toBeDisabled();
    expect(updateProject).not.toHaveBeenCalled();
  });

  it('saves an added directory and requests consent without implicitly trusting it', async () => {
    const trusted = { ...project, folders: [{ ...project.folders[0], trusted: true }] };
    vi.mocked(getProject).mockResolvedValue(trusted);
    const updated = { ...trusted, folders: [...trusted.folders, { path: 'C:\\work\\second', trusted: false }] };
    vi.mocked(updateProject).mockResolvedValue(updated);
    render(<ProjectWorkspaceBar projectId={project.id} onSelect={vi.fn()} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Add project folders' }));
    fireEvent.change(screen.getByLabelText('Folder path'), { target: { value: 'C:\\work\\second' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save folders' }));
    await screen.findByText('Trust this folder?');
    expect(updateProject).toHaveBeenCalledWith(project.id, { folders: [project.workspace_path, 'C:\\work\\second'] });
    expect(trustProjectFolders).not.toHaveBeenCalled();
  });
});
