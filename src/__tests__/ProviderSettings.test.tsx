import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ProviderSettings from '../components/ProviderSettings';
import { fetchProviderModels, getProviders, updateProvider, type Provider } from '../api';

vi.mock('../api', () => ({
  getProviders: vi.fn(), createProvider: vi.fn(), updateProvider: vi.fn(),
  deleteProvider: vi.fn(), fetchProviderModels: vi.fn(),
}));

const provider: Provider = {
  id: 'gateway', name: 'Gateway', baseUrl: 'https://gateway.example/v1',
  apiKey: '', format: 'openai', category: 'custom', enabled: true, models: [],
};

describe('provider web search settings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getProviders).mockResolvedValue([provider]);
    vi.mocked(updateProvider).mockImplementation(async (_id, updates) => ({ ...provider, ...updates }));
  });

  it('defaults existing providers to native auto and persists a gateway override', async () => {
    const user = userEvent.setup();
    render(<ProviderSettings />);
    const search = await screen.findByLabelText('联网搜索');
    expect(search).toHaveValue('auto');
    await user.selectOptions(search, 'deepseek_native');
    await user.click(screen.getAllByRole('button', { name: '保存' })[0]);
    await waitFor(() => expect(updateProvider).toHaveBeenCalledWith('gateway', expect.objectContaining({
      webSearchStrategy: 'deepseek_native', baseUrl: provider.baseUrl,
    })));
    expect(await screen.findByText('已保存到本地配置文件')).toBeInTheDocument();
    expect(search).toHaveValue('deepseek_native');
  });

  it('preserves an explicit public search choice when loading settings', async () => {
    vi.mocked(getProviders).mockResolvedValue([{ ...provider, webSearchStrategy: 'public' }]);
    render(<ProviderSettings />);
    expect(await screen.findByLabelText('联网搜索')).toHaveValue('public');
  });

  it('identifies an edited model and saves a subsequent manual override', async () => {
    const user = userEvent.setup();
    render(<ProviderSettings />);
    await screen.findByLabelText('联网搜索');
    await user.click(screen.getByRole('button', { name: '添加模型' }));
    const id = screen.getByLabelText('模型 ID');
    await user.clear(id);
    await user.type(id, 'gpt-4o');
    expect(screen.getByLabelText('Context window')).toHaveValue('128K');
    expect(screen.getByLabelText('Image')).toBeChecked();
    const context = screen.getByLabelText('Context window');
    await user.clear(context);
    await user.type(context, '64K');
    await user.click(screen.getByLabelText('Image'));
    await user.click(screen.getAllByRole('button', { name: '保存' })[0]);
    await waitFor(() => expect(updateProvider).toHaveBeenCalledWith('gateway', expect.objectContaining({
      models: [expect.objectContaining({ id: 'gpt-4o', contextWindow: 64000, inputTypes: ['text'] })],
    })));
  });

  it('uses fetched capabilities instead of the old text-only default', async () => {
    vi.mocked(getProviders).mockResolvedValue([{ ...provider, models: [{ id: 'gpt-4o', name: 'Custom label', enabled: false, inputTypes: ['text'], contextWindow: 8192 }] }]);
    vi.mocked(fetchProviderModels).mockResolvedValue([{ id: 'gpt-4o', name: 'GPT-4o', enabled: true, contextWindow: 256000, inputTypes: ['text', 'image'] }]);
    const user = userEvent.setup();
    render(<ProviderSettings />);
    await user.click(await screen.findByRole('button', { name: '获取模型列表' }));
    await waitFor(() => expect(screen.getByLabelText('Context window')).toHaveValue('256K'));
    expect(screen.getByLabelText('Image')).toBeChecked();
    expect(screen.getByLabelText('模型名称')).toHaveValue('Custom label');
    expect(screen.getByLabelText('启用 Custom label')).not.toBeChecked();
    await user.click(screen.getAllByRole('button', { name: '保存' })[0]);
    await waitFor(() => expect(updateProvider).toHaveBeenCalledWith('gateway', expect.objectContaining({
      models: [expect.objectContaining({ contextWindow: 256000, inputTypes: ['text', 'image'] })],
    })));
  });
});
