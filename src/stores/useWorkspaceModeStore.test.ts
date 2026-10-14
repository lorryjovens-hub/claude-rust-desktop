import { beforeEach, describe, expect, it } from 'vitest';
import { conversationWorkspaceMode, useWorkspaceModeStore } from './useWorkspaceModeStore';
import { createLocalConversation, getLocalConversations } from '../services/localStorageService';

beforeEach(() => {
  localStorage.clear();
  useWorkspaceModeStore.setState({ mode: 'chat', recentsCollapsed: { chat: false, cowork: false, code: false } });
});

describe('workspace histories', () => {
  it('keeps equal-titled local conversations separate and assigns legacy history only to Chat', () => {
    for (const mode of ['chat', 'cowork', 'code'] as const) {
      createLocalConversation('Same title', undefined, { workspace_mode: mode });
    }
    createLocalConversation('Legacy');
    const conversations = getLocalConversations();
    expect(conversations.filter(c => conversationWorkspaceMode(c) === 'chat')).toHaveLength(2);
    expect(conversations.filter(c => conversationWorkspaceMode(c) === 'cowork')).toHaveLength(1);
    expect(conversations.filter(c => conversationWorkspaceMode(c) === 'code')).toHaveLength(1);
    expect(conversationWorkspaceMode({})).toBe('chat');
  });

  it('persists active mode and separate collapsed states across hydration', async () => {
    useWorkspaceModeStore.getState().toggleRecents('cowork');
    useWorkspaceModeStore.getState().setMode('code');
    await useWorkspaceModeStore.persist.rehydrate();
    expect(useWorkspaceModeStore.getState().mode).toBe('code');
    expect(useWorkspaceModeStore.getState().recentsCollapsed).toEqual({ chat: false, cowork: true, code: false });
  });
});
