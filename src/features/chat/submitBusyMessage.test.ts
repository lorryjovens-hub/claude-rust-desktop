import { beforeEach, expect, it, vi } from 'vitest';
import { submitBusyMessage } from './submitBusyMessage';
import { useBusySendStore } from '../../stores/useBusySendStore';

beforeEach(() => useBusySendStore.setState({ queues: {} }));
it('queues a message without interrupting the current generation', async () => {
  const interrupt = vi.fn();
  await submitBusyMessage('a', 'next', [], 'queue', interrupt);
  expect(interrupt).not.toHaveBeenCalled();
  expect(useBusySendStore.getState().take('a')?.text).toBe('next');
});
it('waits for interruption before prioritizing the steer message', async () => {
  let finish!: () => void;
  const interrupted = new Promise<void>(resolve => { finish = resolve; });
  useBusySendStore.getState().enqueue('a', 'waiting', []);
  const sending = submitBusyMessage('a', 'direction', [], 'steer', () => interrupted);
  expect(useBusySendStore.getState().queues.a.map(message => message.text)).toEqual(['waiting']);
  finish();
  await sending;
  expect(useBusySendStore.getState().take('a')?.text).toBe('direction');
  expect(useBusySendStore.getState().take('a')?.text).toBe('waiting');
});
it('does not send a steer message when interruption fails', async () => {
  await expect(submitBusyMessage('a', 'direction', [], 'steer', async () => { throw new Error('offline'); })).rejects.toThrow('offline');
  expect(useBusySendStore.getState().take('a')).toBeUndefined();
});
