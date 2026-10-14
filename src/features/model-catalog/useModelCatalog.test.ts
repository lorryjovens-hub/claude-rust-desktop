import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { getProviders, type Provider } from '../../api';
import { useChatStore } from '../../stores/useChatStore';
import { normalizeGeneralSettings, useGeneralSettingsStore } from '../../stores/useGeneralSettingsStore';
import { useWorkspaceModeStore } from '../../stores/useWorkspaceModeStore';
import { useModelCatalog } from './useModelCatalog';

vi.mock('../../api', () => ({ getProviders: vi.fn(), getUserModels: vi.fn() }));
const providers: Provider[] = ['a', 'b'].map(id => ({ id, name: id, baseUrl: 'http://localhost', apiKey: '', format: 'openai', enabled: true,
  models: [{ id: 'same', name: 'Same', enabled: true }, { id: 'other', name: 'Other', enabled: true }, { id: 'disabled', name: 'Disabled', enabled: false }],
}));
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  localStorage.setItem('user_mode', 'selfhosted');
  vi.mocked(getProviders).mockResolvedValue(providers);
  useChatStore.setState({ currentModel: '', modelCatalog: null, providersCache: [] });
  useWorkspaceModeStore.setState({ mode: 'chat' });
  useGeneralSettingsStore.setState(normalizeGeneralSettings({ defaultModelByMode: {
    chat: { providerId: 'b', modelId: 'same' }, work: { providerId: 'a', modelId: 'other' }, study: { providerId: 'b', modelId: 'other' },
  } }));
});

it('uses mode defaults for initialization and new chats while keeping explicit selections', async () => {
  const { result } = renderHook(() => useModelCatalog({ current: null }));
  await waitFor(() => expect(result.current.selectorModels).toHaveLength(4));
  expect(useChatStore.getState().currentModel).toBe('b::same');
  expect(result.current.resolveModelForNewChat()).toBe('b::same');
  expect(result.current.resolveModelForMode('work', 'b::same')).toBe('a::other');
  expect(result.current.resolveModelForMode('study', 'a::other')).toBe('b::other');
  expect(result.current.resolveModelForNewChat('a::same')).toBe('a::same');
  act(() => { useChatStore.getState().setCurrentModel('a::same'); window.dispatchEvent(new Event('providers-changed')); });
  await waitFor(() => expect(getProviders).toHaveBeenCalledTimes(2));
  expect(useChatStore.getState().currentModel).toBe('a::same');
});

it('keeps existing conversations and ignores unavailable or disabled defaults', async () => {
  useChatStore.setState({ currentModel: 'a::other' });
  useGeneralSettingsStore.setState(normalizeGeneralSettings({ defaultModelByMode: {
    chat: { providerId: 'removed', modelId: 'same' }, work: { providerId: 'a', modelId: 'disabled' },
  } }));
  const { result } = renderHook(() => useModelCatalog({ current: 'existing-conversation' }));
  await waitFor(() => expect(result.current.selectorModels).toHaveLength(4));
  expect(useChatStore.getState().currentModel).toBe('a::other');
  expect(result.current.resolveModelForNewChat()).toBe('a::other');
  expect(result.current.resolveModelForMode('work', 'b::same')).toBe('b::same');
});

it('resolves an existing unqualified model before applying a new-session default', async () => {
  vi.mocked(getProviders).mockResolvedValue([providers[0], { ...providers[1], models: [providers[1].models[0]] }]);
  const { result } = renderHook(() => useModelCatalog({ current: 'legacy-conversation' }));
  await waitFor(() => expect(result.current.selectorModels).toHaveLength(3));
  expect(result.current.resolveModelForNewChat('other')).toBe('a::other');
  expect(result.current.resolveModelForNewChat()).toBe('b::same');
});
