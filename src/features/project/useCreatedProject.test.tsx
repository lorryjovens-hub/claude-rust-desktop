import { useCallback } from 'react';
import { act, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, useLocation, useNavigate } from 'react-router-dom';
import { beforeEach, expect, it, vi } from 'vitest';
import { useCreatedProject } from './useCreatedProject';
import { useLastCodeProject } from './useLastCodeProject';
import { useProjectStore } from '../../stores/useProjectStore';
import { useWorkspaceModeStore } from '../../stores/useWorkspaceModeStore';
import { getProjects } from '../../api';

vi.mock('../../api', () => ({ getProjects: vi.fn() }));

function Chat() {
  const navigate = useNavigate();
  const location = useLocation();
  const selected = useProjectStore(s => s.pendingProjectId);
  useLastCodeProject(null);
  const reset = useCallback(() => {
    navigate('/', { replace: true });
    useProjectStore.getState().setCurrentProjectId(null);
    useProjectStore.getState().setPendingProjectId(null);
  }, [navigate]);
  useCreatedProject(reset);
  return <><span data-testid="selected">{selected}</span><span data-testid="state">{JSON.stringify(location.state)}</span></>;
}

beforeEach(() => {
  vi.resetAllMocks();
  useProjectStore.getState().resetProject();
  useWorkspaceModeStore.setState({ mode: 'code', lastCodeProjectId: 'old-project' });
  vi.mocked(getProjects).mockResolvedValue([{ id: 'old-project' }] as never);
});

it('selects the created project after reset and consumes navigation state once', async () => {
  useProjectStore.getState().setCurrentProjectId('existing-chat-project');
  render(<MemoryRouter initialEntries={[{ pathname: '/', state: { createdProjectId: 'new-project' } }]}><Chat /></MemoryRouter>);
  await waitFor(() => expect(screen.getByTestId('selected')).toHaveTextContent('new-project'));
  expect(useProjectStore.getState().currentProjectId).toBeNull();
  expect(screen.getByTestId('state')).toHaveTextContent('null');
  act(() => useProjectStore.getState().setPendingProjectId('manual-selection'));
  await waitFor(() => expect(screen.getByTestId('selected')).toHaveTextContent('manual-selection'));
});

it('does not apply creation state to an existing conversation route', async () => {
  useProjectStore.getState().setPendingProjectId('existing-selection');
  render(<MemoryRouter initialEntries={[{ pathname: '/chat/existing', state: { createdProjectId: 'new-project' } }]}><Chat /></MemoryRouter>);
  expect(screen.getByTestId('selected')).toHaveTextContent('existing-selection');
});
