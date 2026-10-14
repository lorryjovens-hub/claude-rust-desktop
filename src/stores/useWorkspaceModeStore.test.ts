import { beforeEach, describe, expect, it } from 'vitest';
import { conversationWorkspaceMode, useWorkspaceModeStore } from './useWorkspaceModeStore';
import { createLocalConversation, getLocalConversations } from '../services/localStorageService';

beforeEach(() => {
  localStorage.clear();
  useWorkspaceModeStore.setState({ mode: 'chat', lastHomeMode: 'chat', recentsCollapsed: { chat: false, work: false, study: false, code: false }, lastCodeProjectId: null });
});

describe('workspace histories', () => {
  it('keeps equal-titled local conversations separate and assigns legacy history only to Chat', () => {
    for (const mode of ['chat', 'work', 'study', 'code'] as const) {
      createLocalConversation('Same title', undefined, { workspace_mode: mode });
    }
    createLocalConversation('Legacy');
    const conversations = getLocalConversations();
    expect(conversations.filter(c => conversationWorkspaceMode(c) === 'chat')).toHaveLength(2);
    expect(conversations.filter(c => conversationWorkspaceMode(c) === 'work')).toHaveLength(1);
    expect(conversations.filter(c => conversationWorkspaceMode(c) === 'study')).toHaveLength(1);
    expect(conversations.filter(c => conversationWorkspaceMode(c) === 'code')).toHaveLength(1);
    expect(conversationWorkspaceMode({})).toBe('chat');
  });

  it('persists active mode and separate collapsed states across hydration', async () => {
    useWorkspaceModeStore.getState().toggleRecents('work');
    useWorkspaceModeStore.getState().setMode('study');
    useWorkspaceModeStore.getState().setMode('code');
    await useWorkspaceModeStore.persist.rehydrate();
    expect(useWorkspaceModeStore.getState().mode).toBe('code');
    expect(useWorkspaceModeStore.getState().recentsCollapsed).toEqual({ chat: false, work: true, study: false, code: false });
    expect(useWorkspaceModeStore.getState().lastHomeMode).toBe('study');
  });

  it('migrates persisted Cowork selection and local history to Work without losing project metadata', async () => {
    localStorage.setItem('workspace-mode', JSON.stringify({ version: 0, state: {
      mode: 'cowork', recentsCollapsed: { chat: true, cowork: true, code: false }, lastCodeProjectId: 'project-1',
    } }));
    localStorage.setItem('app_conversations', JSON.stringify([{ id: 'legacy', workspace_mode: 'cowork', project_id: 'project-1' }]));
    await useWorkspaceModeStore.persist.rehydrate();
    expect(useWorkspaceModeStore.getState()).toMatchObject({
      mode: 'work', lastHomeMode: 'work', lastCodeProjectId: 'project-1',
      recentsCollapsed: { chat: true, work: true, study: false, code: false },
    });
    expect(getLocalConversations()).toEqual([{ id: 'legacy', workspace_mode: 'work', project_id: 'project-1' }]);
    expect(conversationWorkspaceMode({ workspace_mode: 'cowork' })).toBe('work');
  });

  it('persists the last Code project across hydration and mode changes', async () => {
    useWorkspaceModeStore.getState().setMode('code');
    useWorkspaceModeStore.getState().setLastCodeProjectId('project-1');
    useWorkspaceModeStore.getState().setMode('chat');
    await useWorkspaceModeStore.persist.rehydrate();
    expect(useWorkspaceModeStore.getState().lastCodeProjectId).toBe('project-1');
    expect(JSON.parse(localStorage.getItem('workspace-mode')!).state.lastCodeProjectId).toBe('project-1');
  });
});
