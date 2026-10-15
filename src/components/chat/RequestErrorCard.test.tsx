import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import WorkDetails from '../../features/trajectory/WorkDetails';
import { copyToClipboard } from '../../utils/clipboard';
vi.mock('../../utils/clipboard', () => ({ copyToClipboard: vi.fn().mockResolvedValue(true) }));
vi.mock('../MarkdownRenderer', () => ({ default: ({ content }: { content: string }) => <p>{content}</p> }));

it('keeps the answer, expands raw error text and copies status with details', async () => {
  const user = userEvent.setup();
  const detail = '{"detail":"The model is not supported. <script>test</script>"}';
  render(<WorkDetails message={{ content: '已读取 package.json。', requestError: { status: 400, detail } }} index={0} live={false} />);
  expect(screen.getByText('已读取 package.json。')).toBeInTheDocument();
  expect(screen.getByRole('alert')).toHaveTextContent('无效请求');
  expect(screen.queryByText(detail)).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '查看详情' }));
  expect(screen.getByText(detail)).toBeInTheDocument();
  expect(screen.getByText('Status: 400')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '复制错误详情' }));
  expect(copyToClipboard).toHaveBeenCalledWith(`${detail}\nStatus: 400`);
  expect(screen.getByText('已复制')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '收起详情' }));
  expect(screen.queryByText(detail)).not.toBeInTheDocument();
});
