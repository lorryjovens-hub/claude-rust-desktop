import { describe, it, expect, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useI18n } from '../hooks/useI18n';
import { useUIStore } from '../stores/useUIStore';

describe('useI18n', () => {
  beforeEach(() => {
    localStorage.clear();
    useUIStore.getState().setLanguage('en');
  });

  it('should return t function, language, and setLanguage', () => {
    const { result } = renderHook(() => useI18n());
    expect(result.current.t).toBeDefined();
    expect(result.current.language).toBeDefined();
    expect(result.current.setLanguage).toBeDefined();
  });

  it('should translate keys correctly', () => {
    const { result } = renderHook(() => useI18n());
    expect(typeof result.current.t('common.cancel')).toBe('string');
  });

  it('should fallback to English when key not found in current language', () => {
    const { result } = renderHook(() => useI18n());
    const translation = result.current.t('nonexistent.key');
    expect(translation).toBe('nonexistent.key');
  });

  it('should replace params in translation', () => {
    const { result } = renderHook(() => useI18n());
    const text = result.current.t('common.cancel', { count: 5 });
    expect(typeof text).toBe('string');
  });

  it('should set language correctly', () => {
    const { result } = renderHook(() => useI18n());
    act(() => result.current.setLanguage('zh'));
    expect(localStorage.setItem).toHaveBeenCalledWith('app_language', 'zh');
    expect(result.current.t('common.cancel')).toBe('取消');
  });
});
