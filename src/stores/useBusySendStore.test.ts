import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useBusySendStore } from './useBusySendStore';
import type { PendingFile } from '../components/FileUploadPreview';

beforeEach(() => useBusySendStore.setState({ queues: {} }));
describe('busy message queues', () => {
  it('sends FIFO while keeping other conversations and attachment snapshots separate', () => {
    const store = useBusySendStore.getState();
    const files = [{ fileName: 'notes.txt', fileId: 'f1', status: 'done' }] as PendingFile[];
    store.enqueue('a', 'first', files);
    files.length = 0;
    store.enqueue('a', 'second', []);
    store.enqueue('b', 'other conversation', []);
    expect(store.take('a')).toMatchObject({ text: 'first', files: [{ fileId: 'f1' }] });
    expect(store.take('a')?.text).toBe('second');
    expect(store.take('a')).toBeUndefined();
    expect(store.take('b')?.text).toBe('other conversation');
  });
  it('prioritizes a steer message and releases attachment previews on cancellation', () => {
    const revoke = vi.fn();
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revoke });
    const store = useBusySendStore.getState();
    store.enqueue('a', 'waiting', [{ previewUrl: 'blob:preview' }] as PendingFile[]);
    store.enqueue('a', 'new direction', [], true);
    expect(store.take('a')?.text).toBe('new direction');
    const waiting = useBusySendStore.getState().queues.a[0];
    store.cancel('a', waiting.id);
    expect(store.take('a')).toBeUndefined();
    expect(revoke).toHaveBeenCalledWith('blob:preview');
  });
});
