import { beforeEach, describe, expect, it, vi } from 'vitest';
vi.mock('./client', () => ({ isTauriApp: false, API_BASE: '', getToken: () => '', waitForApiReady: async () => {}, request: vi.fn(), getUserModeForConversation: vi.fn(), resolveEnvCreds: vi.fn() }));
import { branchConversation } from './conversations';
import { createLocalConversation, getLocalConversation, getLocalConversations, getLocalMessages, saveLocalMessages, updateLocalConversation } from '../services/localStorageService';

beforeEach(() => localStorage.clear());
describe('conversation branches', () => {
  function source() {
    const conversation = createLocalConversation('Original', 'model', { workspace_mode: 'code' });
    updateLocalConversation(conversation.id, { workspace_path: 'D:/project', provider: 'provider', project_id: 'project' });
    saveLocalMessages(conversation.id, ['user', 'assistant', 'user', 'assistant'].map((role, index) => ({
      id: `m${index}`, conversation_id: conversation.id, role, content: `message ${index}`, sort_order: index,
      created_at: '2026-10-14T00:00:00Z', requestSnapshots: role === 'assistant' ? [{ id: `r${index}`, number: 1, model: 'model', startedAt: 1, systemPrompt: null, systemMessages: [], tools: [], inputTokens: 20, outputTokens: 10 }] : [],
    })));
    return conversation;
  }
  it('includes the selected answer, preserves context and leaves source history intact', async () => {
    const original = source();
    const branch = await branchConversation(original.id, 'm1');
    const messages = getLocalMessages(branch.new_conversation_id);
    expect(messages.map(message => message.content)).toEqual(['message 0', 'message 1']);
    expect(messages[1].id).not.toBe('m1');
    expect(messages[1].requestSnapshots?.[0].outputTokens).toBe(10);
    expect(getLocalConversation(branch.new_conversation_id)).toMatchObject({ workspace_mode: 'code', workspace_path: 'D:/project', provider: 'provider', project_id: 'project', message_count: 2 });
    expect(getLocalMessages(original.id)).toHaveLength(4);
  });
  it('uses the clicked position when optimistic IDs differ, and rejects missing boundaries', async () => {
    const original = source();
    const branch = await branchConversation(original.id, 'optimistic-id', 1);
    expect(getLocalMessages(branch.new_conversation_id)).toHaveLength(2);
    await expect(branchConversation(original.id, 'missing')).rejects.toThrow('Branch message not found');
    await expect(branchConversation(original.id, undefined, 99)).rejects.toThrow('Branch message not found');
    expect(getLocalConversations()).toHaveLength(2);
  });
});
