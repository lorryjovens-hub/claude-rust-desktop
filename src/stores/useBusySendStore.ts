import { create } from 'zustand';
import type { PendingFile } from '../components/FileUploadPreview';

export interface QueuedMessage { id: string; text: string; files: PendingFile[] }
export const useBusySendStore = create<{
  queues: Record<string, QueuedMessage[]>;
  enqueue: (conversationId: string, text: string, files: PendingFile[], first?: boolean) => void;
  take: (conversationId: string) => QueuedMessage | undefined;
  cancel: (conversationId: string, messageId: string) => void;
}>((set, get) => ({
  queues: {},
  enqueue: (conversationId, text, files, first = false) => set(state => {
    const message = { id: crypto.randomUUID(), text, files: [...files] };
    const queue = state.queues[conversationId] || [];
    return { queues: { ...state.queues, [conversationId]: first ? [message, ...queue] : [...queue, message] } };
  }),
  take: conversationId => {
    const queue = get().queues[conversationId] || [];
    const message = queue[0];
    if (message) set(state => ({ queues: { ...state.queues, [conversationId]: queue.slice(1) } }));
    return message;
  },
  cancel: (conversationId, messageId) => set(state => {
    const queue = state.queues[conversationId] || [];
    const message = queue.find(item => item.id === messageId);
    message?.files.forEach(file => { if (file.previewUrl) URL.revokeObjectURL(file.previewUrl); });
    return { queues: { ...state.queues, [conversationId]: queue.filter(item => item.id !== messageId) } };
  }),
}));
