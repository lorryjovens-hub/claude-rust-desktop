import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ResponseFooter from './ResponseFooter';
import { feedbackKey, useMessageFeedbackStore } from './feedback';

const message = { id: 'optimistic', content: 'The complete answer', created_at: '2026-10-14T12:00:00Z', role: 'assistant', requestSnapshots: [{ id: 'r', model: 'deepseek-flash', providerId: 'deepseek', inputTokens: 24000, outputTokens: 901, cacheReadTokens: 22000, cacheWriteTokens: 0 }] };
const writeText = vi.fn(async () => {});
beforeEach(() => {
  localStorage.clear();
  useMessageFeedbackStore.setState({ ratings: {} });
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  writeText.mockClear();
});
describe('response footer', () => {
  it('copies answer text and branches at the clicked response', async () => {
    const onBranch = vi.fn();
    render(<ResponseFooter message={message} index={1} onBranch={onBranch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Copy response' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('The complete answer'));
    fireEvent.click(screen.getByRole('button', { name: 'Branch into a new conversation' }));
    expect(onBranch).toHaveBeenCalledWith(1);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Branch into a new conversation' })).not.toBeDisabled());
  });
  it('persists exclusive feedback across remounts and reconciled message IDs', async () => {
    const view = render(<ResponseFooter message={message} index={1} conversationId="session" onBranch={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: 'Good response' }));
    expect(screen.getByRole('button', { name: 'Good response' })).toHaveAttribute('aria-pressed', 'true');
    const saved = localStorage.getItem('message-feedback-v1');
    view.unmount();
    useMessageFeedbackStore.setState({ ratings: {} });
    localStorage.setItem('message-feedback-v1', saved!);
    await useMessageFeedbackStore.persist.rehydrate();
    render(<ResponseFooter message={{ ...message, id: 'sqlite-id' }} index={1} conversationId="session" onBranch={() => {}} />);
    expect(screen.getByRole('button', { name: 'Good response' })).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(screen.getByRole('button', { name: 'Bad response' }));
    expect(screen.getByRole('button', { name: 'Good response' })).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByRole('button', { name: 'Bad response' })).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(screen.getByRole('button', { name: 'Bad response' }));
    expect(useMessageFeedbackStore.getState().ratings[feedbackKey('session', 1, message)]).toBeUndefined();
    expect(feedbackKey('other-session', 1, message)).not.toBe(feedbackKey('session', 1, message));
  });
  it('shows real turn usage and reports branch failures', async () => {
    render(<ResponseFooter message={message} index={1} onBranch={async () => { throw new Error('Branch failed'); }} />);
    fireEvent.click(screen.getByRole('button', { name: 'Turn usage' }));
    expect(screen.getByText('Provider / model')).toBeInTheDocument();
    expect(screen.getByText('Cache hit')).toBeInTheDocument();
    expect(screen.getByText('Cached input')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Branch into a new conversation' }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Branch failed'));
  });
});
