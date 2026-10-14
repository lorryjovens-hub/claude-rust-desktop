import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { extractTextContent } from '../../utils/messageHelpers';

export type Feedback = 'up' | 'down';

// Message IDs can change when optimistic history is replaced by SQLite history.
// Scope by conversation and position, with a content fingerprint to avoid rating edited answers.
export function feedbackKey(conversationId: string, index: number, message: any): string {
  const text = extractTextContent(message.content);
  let hash = 2166136261;
  for (let i = 0; i < text.length; i++) hash = Math.imul(hash ^ text.charCodeAt(i), 16777619);
  return `${conversationId}:${index}:${text.length}:${hash >>> 0}`;
}

export const useMessageFeedbackStore = create<{
  ratings: Record<string, Feedback>;
  toggle: (key: string, feedback: Feedback) => void;
}>()(persist((set) => ({
  ratings: {},
  toggle: (key, feedback) => set(state => {
    const ratings = { ...state.ratings };
    if (ratings[key] === feedback) delete ratings[key];
    else ratings[key] = feedback;
    return { ratings };
  }),
}), { name: 'message-feedback-v1' }));
