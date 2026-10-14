import { beforeEach, expect, it, vi } from 'vitest';
import { openChatLink } from './chatLinks';
import { useGeneralSettingsStore } from '../stores/useGeneralSettingsStore';

beforeEach(() => useGeneralSettingsStore.setState({ chatLinks: 'sidebar' }));
it('routes web links to the sidebar and ignores non-web protocols', async () => {
  const handler = vi.fn();
  window.addEventListener('request-open-panel', handler);
  await openChatLink('https://example.com/test');
  expect(handler.mock.calls[0][0].detail).toEqual({ panel: 'browser', url: 'https://example.com/test' });
  await openChatLink('javascript:alert(1)');
  expect(handler).toHaveBeenCalledTimes(1);
  window.removeEventListener('request-open-panel', handler);
});
it('uses the browser when selected', async () => {
  useGeneralSettingsStore.setState({ chatLinks: 'browser' });
  const open = vi.spyOn(window, 'open').mockReturnValue(null);
  await openChatLink('https://example.com');
  expect(open).toHaveBeenCalledWith('https://example.com', '_blank', 'noopener,noreferrer');
  open.mockRestore();
});
