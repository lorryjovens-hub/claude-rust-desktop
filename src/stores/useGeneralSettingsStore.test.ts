import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { initializeGeneralSettings, normalizeGeneralSettings, openGeneralConfiguration, useGeneralSettingsStore } from './useGeneralSettingsStore';
import { useUIStore } from './useUIStore';

beforeEach(() => {
  (window as any).__TAURI_INTERNALS__ = {};
  localStorage.clear();
  useGeneralSettingsStore.setState({ ...normalizeGeneralSettings({}), saveError: '' });
  vi.mocked(invoke).mockReset();
});
afterEach(() => { delete (window as any).__TAURI_INTERNALS__; });

it('validates provider/model defaults per mode and saves them to desktop configuration', async () => {
  const settings = normalizeGeneralSettings({ defaultModelByMode: { chat: { providerId: ' a ', modelId: 'same' }, work: { providerId: '', modelId: 'bad' }, study: 'bad' } });
  expect(settings.defaultModelByMode).toEqual({ chat: { providerId: 'a', modelId: 'same' }, work: null, study: null });
  vi.mocked(invoke).mockResolvedValue(null);
  useGeneralSettingsStore.getState().update({ defaultModelByMode: settings.defaultModelByMode });
  await openGeneralConfiguration();
  expect(invoke).toHaveBeenCalledWith('write_general_settings', expect.objectContaining({ settings: expect.objectContaining({ defaultModelByMode: settings.defaultModelByMode }) }));
});

it('loads a desktop configuration, validates it, and applies language and appearance', async () => {
  vi.mocked(invoke).mockResolvedValue({ defaultPermissionMode: 'plan_mode', theme: 'dark', fontSize: 99, language: 'zh', chatFont: 'sans', workDetailsByMode: { chat: 'verbose' }, busySendBehavior: 'steer' });
  await initializeGeneralSettings();
  expect(useGeneralSettingsStore.getState()).toMatchObject({ defaultPermissionMode: 'plan_mode', fontSize: 32, workDetailsByMode: { chat: 'verbose', work: 'standard', study: 'standard', code: 'detailed' }, busySendBehavior: 'steer' });
  expect(document.documentElement).toHaveClass('dark');
  expect(document.documentElement.getAttribute('data-chat-font')).toBe('sans');
  expect(useUIStore.getState().language).toBe('zh');
  expect(invoke).toHaveBeenCalledTimes(1);
});

it('finishes saving before opening the desktop configuration and excludes internal state', async () => {
  vi.mocked(invoke).mockResolvedValue(null);
  useGeneralSettingsStore.getState().update({ fontSize: 18 });
  await openGeneralConfiguration();
  const calls = vi.mocked(invoke).mock.calls;
  expect(calls[calls.length - 1][0]).toBe('open_general_settings');
  const snapshot = (calls[0][1] as { settings: Record<string, unknown> }).settings;
  expect(snapshot.fontSize).toBe(18);
  expect(snapshot).not.toHaveProperty('update');
  expect(snapshot).not.toHaveProperty('saveError');
});

it('keeps local settings usable and reports an unreadable configuration', async () => {
  vi.mocked(invoke).mockRejectedValue(new Error('Invalid JSON'));
  await initializeGeneralSettings();
  expect(useGeneralSettingsStore.getState().saveError).toContain('Invalid JSON');
  expect(useGeneralSettingsStore.getState().fontSize).toBe(16);
  expect(invoke).toHaveBeenCalledTimes(1);
});

it('upgrades the old default size once while preserving later and custom choices', () => {
  expect(normalizeGeneralSettings({}).fontSize).toBe(16);
  expect(normalizeGeneralSettings({ fontSize: 14 })).toMatchObject({ fontSize: 16, typographyVersion: 1 });
  expect(normalizeGeneralSettings({ fontSize: 18 }).fontSize).toBe(18);
  expect(normalizeGeneralSettings({ fontSize: 14, typographyVersion: 1 }).fontSize).toBe(14);
});

it('migrates global work details to mode defaults and validates each saved mode independently', () => {
  const defaults = { chat: 'compact', work: 'standard', study: 'standard', code: 'detailed' };
  expect(normalizeGeneralSettings({}).workDetailsByMode).toEqual(defaults);
  expect(normalizeGeneralSettings({ workDetails: 'verbose' }).workDetailsByMode).toEqual(defaults);
  expect(normalizeGeneralSettings({ workDetailsByMode: { chat: 'verbose', work: 'invalid', code: 'compact' } }).workDetailsByMode)
    .toEqual({ ...defaults, chat: 'verbose', code: 'compact' });
});

it('uses per-mode usage defaults and preserves explicit false for Code after configuration reload', async () => {
  const defaults = { chat: false, work: false, study: false, code: true };
  expect(normalizeGeneralSettings({ showHomeUsage: true }).showUsageByMode).toEqual(defaults);
  expect(normalizeGeneralSettings({ showUsageByMode: { chat: true, work: 'false', code: false } }).showUsageByMode)
    .toEqual({ ...defaults, chat: true, code: false });
  vi.mocked(invoke).mockResolvedValue({ showUsageByMode: { chat: true, code: false } });
  await initializeGeneralSettings();
  expect(useGeneralSettingsStore.getState().showUsageByMode).toEqual({ ...defaults, chat: true, code: false });
});
