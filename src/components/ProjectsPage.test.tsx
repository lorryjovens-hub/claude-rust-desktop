import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ProjectsPage from './ProjectsPage';
import { createProjectConversation, getProject, getProjects, getProviders, getUserModels } from '../api';
import type { Provider } from '../api';
import { useChatStore } from '../stores/useChatStore';
import { modelKey } from '../utils/modelConfig';

vi.mock('../api', () => ({
  getProject: vi.fn(), getProjects: vi.fn(), getProviders: vi.fn(), getUserModels: vi.fn(),
  createProjectConversation: vi.fn(), updateProject: vi.fn(), deleteProject: vi.fn(),
  uploadProjectFile: vi.fn(), deleteProjectFile: vi.fn(), deleteConversation: vi.fn(),
  getSkills: vi.fn(), createProject: vi.fn(), trustProjectFolders: vi.fn(),
}));
vi.mock('../hooks/useI18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }));

const project = {
  id: 'project-1', name: 'paper-question-cv', description: '', instructions: '',
  workspace_path: 'C:\\work\\paper-question-cv',
  folders: [{ path: 'C:\\work\\paper-question-cv', trusted: true }],
  conversations: [], files: [], is_archived: 0, created_at: '', updated_at: '',
};
const provider: Provider = {
  id: 'configured-provider', name: 'Configured provider', apiKey: '', baseUrl: 'http://localhost',
  format: 'openai', enabled: true,
  models: Array.from({ length: 47 }, (_, index) => ({ id: `model-${index + 1}`, name: `Model ${index + 1}`, enabled: true })),
};

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  localStorage.setItem('user_mode', 'selfhosted');
  useChatStore.setState({ modelCatalog: null, currentModel: '', providersCache: [], reasoningMode: 'off' });
  vi.mocked(getProjects).mockResolvedValue([project]);
  vi.mocked(getProject).mockResolvedValue(project);
  vi.mocked(getProviders).mockResolvedValue([provider]);
  vi.mocked(createProjectConversation).mockResolvedValue({ id: 'new-conversation' });
});

const renderProject = () => render(
  <MemoryRouter initialEntries={['/projects?project=project-1']}><ProjectsPage /></MemoryRouter>,
);
const openModels = async () => {
  fireEvent.click(await screen.findByRole('button', { name: '模型与 Effort' }));
  fireEvent.click(within(screen.getByRole('dialog', { name: '模型选择' })).getByRole('button', { name: /^Model / }));
  return screen.getByRole('dialog', { name: '模型选择' });
};

describe('project model selection', () => {
  it('loads all 47 configured models and submits the selected provider-qualified model', async () => {
    localStorage.setItem('chat_models', JSON.stringify([{ id: 'legacy-model', name: 'Legacy model' }]));
    renderProject();
    await waitFor(() => expect(useChatStore.getState().modelCatalog?.all).toHaveLength(47));
    const menu = await openModels();
    expect(within(menu).getAllByRole('button', { name: /^Model \d+$/ })).toHaveLength(47);
    expect(within(menu).queryByText('Legacy model')).not.toBeInTheDocument();
    fireEvent.click(within(menu).getByRole('button', { name: 'Model 47' }));
    fireEvent.change(screen.getByPlaceholderText('customize.howCanIHelp'), { target: { value: 'Explain this project' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }));
    await waitFor(() => expect(createProjectConversation).toHaveBeenCalledWith(project.id, 'Explain this project', modelKey(provider.id, 'model-47')));
  });

  it('refreshes after provider edits and excludes disabled providers and models', async () => {
    renderProject();
    await waitFor(() => expect(useChatStore.getState().modelCatalog?.all).toHaveLength(47));
    const menu = await openModels();
    vi.mocked(getProviders).mockResolvedValue([
      { ...provider, models: [{ id: 'new-model', name: 'New model', enabled: true }, { id: 'disabled-model', name: 'Disabled model', enabled: false }] },
      { ...provider, id: 'disabled-provider', enabled: false },
    ]);
    act(() => { window.dispatchEvent(new Event('providers-changed')); });
    expect(await within(menu).findByRole('button', { name: 'New model' })).toBeEnabled();
    expect(within(menu).queryByRole('button', { name: 'Disabled model' })).not.toBeInTheDocument();
    expect(within(menu).queryByRole('button', { name: 'Model 47' })).not.toBeInTheDocument();
    expect(useChatStore.getState().currentModel).toBe(modelKey(provider.id, 'new-model'));
  });

  it('does not invent fallback models or allow submission without enabled models', async () => {
    vi.mocked(getProviders).mockResolvedValue([]);
    renderProject();
    await screen.findByRole('heading', { name: project.name });
    await waitFor(() => expect(useChatStore.getState().modelCatalog?.all).toEqual([]));
    const menu = await openModels();
    expect(within(menu).getByText('请先在设置中配置并启用供应商和模型。')).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('customize.howCanIHelp'), { target: { value: 'Explain this project' } });
    expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled();
    fireEvent.keyDown(screen.getByPlaceholderText('customize.howCanIHelp'), { key: 'Enter' });
    expect(createProjectConversation).not.toHaveBeenCalled();
  });

  it('uses the account catalog in hosted mode', async () => {
    localStorage.setItem('user_mode', 'clawparrot');
    vi.mocked(getUserModels).mockResolvedValue({ all: [{ id: 'account-model', name: 'Account model', enabled: 1 }], common: [], fallback_model: 'account-model' });
    renderProject();
    await waitFor(() => expect(useChatStore.getState().currentModel).toBe('account-model'));
    const menu = await openModels();
    expect(within(menu).getByRole('button', { name: 'Account model' })).toBeEnabled();
    expect(getProviders).not.toHaveBeenCalled();
  });
});
