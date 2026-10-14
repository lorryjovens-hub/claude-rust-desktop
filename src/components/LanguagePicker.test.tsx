import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import LanguagePicker from './LanguagePicker';
import { supportedLanguages, getInitialLanguage } from '../locales/languages';
import { useUIStore } from '../stores/useUIStore';

beforeEach(() => {
  localStorage.clear();
  useUIStore.getState().setLanguage('en');
});

const openPicker = () => {
  fireEvent.click(screen.getByRole('button', { expanded: false }));
  return screen.getByRole('dialog');
};

describe('language picker', () => {
  it('shows all supported languages with native names and highlights the active language', () => {
    render(<LanguagePicker />);
    const dialog = openPicker();
    expect(within(dialog).getAllByRole('button', { pressed: false })).toHaveLength(supportedLanguages.length - 1);
    expect(within(dialog).getByRole('button', { pressed: true })).toHaveTextContent('English (United States)');
    for (const language of supportedLanguages) expect(within(dialog).getByText(language.nativeName, { selector: '[lang]' })).toBeInTheDocument();
  });

  it('searches by English, native name and locale tag, and handles no matches', () => {
    render(<LanguagePicker />);
    const dialog = openPicker();
    const search = within(dialog).getByRole('searchbox');
    for (const query of ['Japanese', '日本語', 'ja-JP']) {
      fireEvent.change(search, { target: { value: query } });
      expect(within(dialog).getAllByRole('button', { pressed: false })).toHaveLength(1);
      expect(within(dialog).getByText('日本語（日本）')).toBeInTheDocument();
    }
    fireEvent.change(search, { target: { value: 'no-such-language' } });
    expect(within(dialog).getByRole('status')).toHaveTextContent('No matching languages');
  });

  it.each(['zh-TW', 'ja-JP', 'ko-KR', 'de-DE', 'it-IT', 'ar-SA'])('selects and restores %s, including document direction', id => {
    render(<LanguagePicker />);
    const dialog = openPicker();
    const language = supportedLanguages.find(item => item.id === id)!;
    fireEvent.click(within(dialog).getByText(language.nativeName, { selector: '[lang]' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(localStorage.getItem('app_language')).toBe(id);
    expect(getInitialLanguage()).toBe(id);
    expect(document.documentElement.lang).toBe(language.locale);
    expect(document.documentElement.dir).toBe(language.direction);
    expect(screen.getByRole('button', { expanded: false })).toHaveTextContent(language.nativeName);
    act(() => useUIStore.getState().setLanguage('en'));
    expect(document.documentElement.dir).toBe('ltr');
  });

  it('traps keyboard focus and restores it without changing language on Escape', async () => {
    const user = userEvent.setup();
    render(<LanguagePicker />);
    const trigger = screen.getByRole('button');
    await user.click(trigger);
    const dialog = screen.getByRole('dialog');
    const close = within(dialog).getByRole('button', { name: 'Close' });
    close.focus();
    await user.tab({ shift: true });
    const buttons = within(dialog).getAllByRole('button');
    expect(buttons[buttons.length - 1]).toHaveFocus();
    await user.tab();
    expect(close).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    expect(useUIStore.getState().language).toBe('en');
  });
});
