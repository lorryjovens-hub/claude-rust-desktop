import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import MessageList from './MessageList';
import MessageActions from '../../components/chat/MessageActions';

vi.mock('../../components/MarkdownRenderer', () => ({ default: ({ content }: { content: string }) => <p>{content}</p> }));
vi.mock('../../components/ToolDiffView', () => ({ default: () => null, shouldUseDiffView: () => false }));

describe('message branch controls', () => {
  it('offers branching only after assistant responses in the active conversation list', () => {
    const onBranch = vi.fn();
    render(<MessageList conversationId="session" messages={[{ role: 'user', content: 'Question' }, { role: 'assistant', content: 'Answer' }]}
      loading={false} expandedMessages={new Set()} editingMessageIdx={null} editingContent="" copiedMessageIdx={null} compactStatus={{ state: 'idle' }}
      onSetEditingContent={() => {}} onEditCancel={() => {}} onEditSave={() => {}} onToggleExpand={() => {}} onResend={() => {}} onEdit={() => {}}
      onCopy={() => {}} onBranch={onBranch} onSetMessages={() => {}} messageContentRefs={{ current: new Map() }} t={key => key} />);
    expect(screen.getAllByRole('button', { name: 'Branch into a new conversation' })).toHaveLength(1);
    expect(screen.queryByTitle('分支')).not.toBeInTheDocument();
    expect(screen.getByText('Question')).toBeInTheDocument();
  });
  it('keeps the alternate action component consistent for user and assistant roles', () => {
    const props = { content: 'Text', idx: 0, isCopied: false, onCopy: vi.fn(), onBranch: vi.fn() };
    const view = render(<MessageActions {...props} role="user" />);
    expect(screen.queryByTitle('分支对话')).not.toBeInTheDocument();
    view.rerender(<MessageActions {...props} role="assistant" />);
    expect(screen.getByTitle('分支对话')).toBeInTheDocument();
  });
});
