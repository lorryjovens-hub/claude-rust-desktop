import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it } from 'vitest';
import ModeModelDefaults from './ModeModelDefaults';
import { normalizeGeneralSettings, useGeneralSettingsStore } from '../stores/useGeneralSettingsStore';
import { useUIStore } from '../stores/useUIStore';
import type { Provider } from '../api';

const providers: Provider[] = ['a', 'b'].map(id => ({ id, name: id.toUpperCase(), apiKey: '', baseUrl: 'http://localhost', format: 'openai', enabled: true,
  models: [{ id: 'z', name: 'Z model' }, { id: 'same', name: 'A model' }, { id: 'disabled', name: 'Disabled', enabled: false }],
}));

beforeEach(() => {
  localStorage.clear();
  useUIStore.getState().setLanguage('en');
  useGeneralSettingsStore.setState({ ...normalizeGeneralSettings({}), saveError: '' });
});

it('saves independent provider/model pairs and sorts model choices', () => {
  const view = render(<ModeModelDefaults providers={providers} />);
  fireEvent.change(screen.getByLabelText('Chat · Provider'), { target: { value: 'b' } });
  fireEvent.change(screen.getByLabelText('Work · Provider'), { target: { value: 'a' } });
  fireEvent.change(screen.getByLabelText('Work · Model'), { target: { value: 'z' } });
  expect([...screen.getByLabelText('Chat · Model').querySelectorAll('option')].map(option => option.textContent)).toEqual(['A model', 'Z model']);
  const saved = JSON.parse(localStorage.getItem('general_settings')!);
  expect(saved.defaultModelByMode).toEqual({ chat: { providerId: 'b', modelId: 'same' }, work: { providerId: 'a', modelId: 'z' }, study: null });
  view.unmount();
  useGeneralSettingsStore.setState(normalizeGeneralSettings(saved));
  render(<ModeModelDefaults providers={providers} />);
  expect(screen.getByLabelText('Chat · Provider')).toHaveValue('b');
  expect(screen.getByLabelText('Work · Model')).toHaveValue('z');
  fireEvent.change(screen.getByLabelText('Chat · Provider'), { target: { value: '' } });
  expect(useGeneralSettingsStore.getState().defaultModelByMode.chat).toBeNull();
});

it('retains an unavailable default visibly instead of silently selecting another provider', () => {
  useGeneralSettingsStore.setState(normalizeGeneralSettings({ defaultModelByMode: { chat: { providerId: 'removed', modelId: 'same' } } }));
  render(<ModeModelDefaults providers={providers} />);
  expect(screen.getByLabelText('Chat · Provider')).toHaveValue('removed');
  expect(screen.getByLabelText('Chat · Model')).toBeDisabled();
  expect(screen.getByRole('option', { name: 'removed (Unavailable)' })).toBeDisabled();
});
