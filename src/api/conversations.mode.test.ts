import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createLocalConversation, updateLocalConversation } from '../services/localStorageService';

vi.mock('./client', () => ({
  isTauriApp: false, API_BASE: '', getToken: () => '', waitForApiReady: async () => {},
  request: vi.fn(), getUserModeForConversation: vi.fn(), resolveEnvCreds: vi.fn(),
}));

beforeEach(() => {
  localStorage.clear();
  vi.stubEnv('DEV', false);
});

afterEach(() => vi.unstubAllEnvs());

describe.each(['desktop', 'modular'] as const)('%s conversation mode restoration', implementation => {
  it.each(['chat', 'work', 'study', 'code'] as const)('restores %s and its selected project from local history', async mode => {
    const api = implementation === 'desktop' ? await import('../api') : await import('./conversations');
    const conversation = createLocalConversation('Existing conversation', undefined, { workspace_mode: mode });
    updateLocalConversation(conversation.id, { project_id: 'selected-project', workspace_path: 'D:/project' });
    const restored = await api.getConversation(conversation.id);
    expect(restored).toMatchObject({ workspace_mode: mode, project_id: 'selected-project', workspace_path: 'D:/project' });
  });

  it('restores legacy Cowork as Work', async () => {
    const api = implementation === 'desktop' ? await import('../api') : await import('./conversations');
    localStorage.setItem('app_conversations', JSON.stringify([{ id: 'legacy', workspace_mode: 'cowork', project_id: 'project' }]));
    expect(await api.getConversation('legacy')).toMatchObject({ workspace_mode: 'work', project_id: 'project' });
  });
});
