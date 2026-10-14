import { useState } from 'react';
import { Database, GitBranch, Loader2, ThumbsDown, ThumbsUp } from 'lucide-react';
import CopyButton from '../trajectory/CopyButton';
import { extractTextContent, formatMessageTime } from '../../utils/messageHelpers';
import { feedbackKey, useMessageFeedbackStore } from './feedback';
import { aggregateUsage, formatTokens } from '../usage/model';
import UsagePopover from '../usage/UsagePopover';
import TokenUsageDetails from '../usage/TokenUsageDetails';

export default function ResponseFooter({ message, index, conversationId, disabled = false, onBranch }: {
  message: any;
  index: number;
  conversationId?: string | null;
  disabled?: boolean;
  onBranch: (index: number) => void | Promise<void>;
}) {
  const key = feedbackKey(conversationId || 'unsaved', index, message);
  const rating = useMessageFeedbackStore(state => state.ratings[key]);
  const toggle = useMessageFeedbackStore(state => state.toggle);
  const [branching, setBranching] = useState(false);
  const [error, setError] = useState('');
  const usage = aggregateUsage([message]);
  const time = message.created_at || (message.processEndedAt ? new Date(message.processEndedAt).toISOString() : null);
  const buttonClass = 'inline-flex items-center justify-center rounded-lg p-2 text-claude-textSecondary hover:bg-claude-hover hover:text-claude-text focus-visible:ring-2 focus-visible:ring-blue-400 disabled:opacity-40 disabled:cursor-not-allowed';

  return <div className="mt-4 mb-2" aria-label="Response actions">
    <div className="flex flex-wrap items-center gap-1 text-[13px] text-claude-textSecondary">
      <CopyButton text={extractTextContent(message.content)} label="Copy response" />
      <button type="button" aria-label="Good response" title="Good response" aria-pressed={rating === 'up'}
        className={`${buttonClass} ${rating === 'up' ? 'bg-claude-hover text-claude-text' : ''}`} onClick={() => toggle(key, 'up')}>
        <ThumbsUp size={17} fill={rating === 'up' ? 'currentColor' : 'none'} />
      </button>
      <button type="button" aria-label="Bad response" title="Bad response" aria-pressed={rating === 'down'}
        className={`${buttonClass} ${rating === 'down' ? 'bg-claude-hover text-claude-text' : ''}`} onClick={() => toggle(key, 'down')}>
        <ThumbsDown size={17} fill={rating === 'down' ? 'currentColor' : 'none'} />
      </button>
      <button type="button" aria-label="Branch into a new conversation" title="Branch into a new conversation"
        disabled={disabled || branching} className={buttonClass} onClick={async () => {
          setError(''); setBranching(true);
          try { await onBranch(index); }
          catch (err) { setError(err instanceof Error ? err.message : 'Unable to branch conversation. Please try again.'); }
          finally { setBranching(false); }
        }}>
        {branching ? <Loader2 size={17} className="animate-spin" /> : <GitBranch size={17} />}
      </button>
      <UsagePopover label="Turn usage" trigger={<><Database size={17} /><span>Usage {usage.available ? `${formatTokens(usage.total)} tok` : 'unavailable'}</span></>}>
        <TokenUsageDetails usage={usage} showModels />
      </UsagePopover>
      {time && <time className="ml-2" dateTime={time} title={new Date(time).toLocaleString()}>{formatMessageTime(time)}</time>}
    </div>
    {error && <p role="alert" className="mt-1 text-[12px] text-red-500">{error}</p>}
  </div>;
}
