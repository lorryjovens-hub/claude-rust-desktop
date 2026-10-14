// One request per process. Credentials arrive on stdin and are never command-line arguments.
import { builtinModels } from '@earendil-works/pi-ai/providers/all';

const emit = value => process.stdout.write(JSON.stringify(value) + '\n');
const usage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } };

export function convertMessages(messages, model) {
  const toolNames = new Map();
  return messages.map(message => {
    if (message.role === 'assistant' && message.provider_content) {
      for (const part of message.provider_content.content || []) {
        if (part.type === 'toolCall') toolNames.set(part.id, part.name);
      }
      return message.provider_content;
    }
    let content = typeof message.content === 'string' ? [{ type: 'text', text: message.content }] : (message.content || []).map(part => {
      if (part.type === 'text') return part;
      const data = part.image_url?.url?.match(/^data:([^;]+);base64,(.+)$/s);
      if (!data) throw new Error('Image input must contain base64 image data');
      return { type: 'image', mimeType: data[1], data: data[2] };
    });
    if (message.role === 'tool') return { role: 'toolResult', toolCallId: message.tool_call_id, toolName: toolNames.get(message.tool_call_id) || 'tool', content, isError: false, timestamp: Date.now() };
    if (message.role === 'assistant') {
      if (message.reasoning_content) content.unshift({ type: 'thinking', thinking: message.reasoning_content });
      for (const call of message.tool_calls || []) {
        toolNames.set(call.id, call.function.name);
        content.push({ type: 'toolCall', id: call.id, name: call.function.name, arguments: JSON.parse(call.function.arguments || '{}') });
      }
      return { role: 'assistant', content, api: model.api, provider: model.provider, model: model.id, usage, stopReason: message.tool_calls?.length ? 'toolUse' : 'stop', timestamp: Date.now() };
    }
    return { role: 'user', content, timestamp: Date.now() };
  });
}

export async function run(request) {
  const models = builtinModels();
  const builtinId = request.provider.builtin_id;
  const provider = models.getProvider(builtinId);
  if (!provider) throw new Error(`Unknown built-in provider: ${builtinId}`);
  const configured = request.model;
  const template = models.getModel(builtinId, configured.id);
  const model = {
    ...(template || { id: configured.id, name: configured.name, provider: builtinId, api: configured.api, reasoning: true, cost: usage.cost }),
    id: configured.id, name: configured.name,
    baseUrl: request.provider.base_url || template?.baseUrl || provider.baseUrl,
    contextWindow: configured.context_window || template?.contextWindow || 128000,
    maxTokens: configured.max_tokens || template?.maxTokens || 8192,
    input: [...(configured.supports_text ? ['text'] : []), ...(configured.supports_vision ? ['image'] : [])],
  };
  if (!model.api) throw new Error('Select a model from the provider catalog or specify its protocol');
  const options = { maxTokens: request.max_tokens, timeoutMs: 300000, maxRetries: 2,
    ...(request.provider.api_key ? { apiKey: request.provider.api_key } : {}),
    ...(request.effort && request.effort !== 'off' ? { reasoning: request.effort } : {}),
  };
  const systemPrompt = [request.system_prompt, ...request.messages.filter(m => m.role === 'system').map(m => typeof m.content === 'string' ? m.content : m.content.filter(p => p.type === 'text').map(p => p.text).join('\n'))].filter(Boolean).join('\n\n');
  const context = { systemPrompt, messages: convertMessages(request.messages.filter(m => m.role !== 'system'), model), tools: request.tools.map(t => ({ name: t.name, description: t.description, parameters: t.input_schema })) };
  let toolIndex = 0;
  const chunk = (delta, finish_reason = null, extra = {}) => emit({ choices: [{ index: 0, delta, finish_reason }], ...extra });
  for await (const event of models.streamSimple(model, context, options)) {
    if (event.type === 'start') chunk({ role: 'assistant' });
    if (event.type === 'text_delta') chunk({ content: event.delta });
    if (event.type === 'thinking_delta') chunk({ reasoning_content: event.delta });
    if (event.type === 'toolcall_start') chunk({}, null, { response_content_started: true });
    if (event.type === 'toolcall_end') chunk({ tool_calls: [{ index: toolIndex++, id: event.toolCall.id, type: 'function', function: { name: event.toolCall.name, arguments: JSON.stringify(event.toolCall.arguments) } }] });
    if (event.type === 'error') throw new Error(event.error.errorMessage || 'Provider stream failed');
    if (event.type === 'done') {
      const finish = event.reason === 'toolUse' ? 'tool_calls' : event.reason === 'length' ? 'length' : 'stop';
      chunk({}, event.message.rawStopReason || finish, { id: event.message.responseId,
        model: event.message.responseModel || event.message.model, provider_content: event.message, usage: {
        prompt_tokens: event.message.usage.input + event.message.usage.cacheRead + event.message.usage.cacheWrite,
        completion_tokens: event.message.usage.output,
        total_tokens: event.message.usage.totalTokens,
        ...(event.message.usage.reasoning != null ? { completion_tokens_details: { reasoning_tokens: event.message.usage.reasoning } } : {}),
        prompt_tokens_details: { cached_tokens: event.message.usage.cacheRead },
        cache_creation_input_tokens: event.message.usage.cacheWrite,
        provider_usage: event.message.usage,
      } });
    }
  }
}

if (process.env.CLAUDE_PROVIDER_WORKER === '1') {
  try {
    let input = '';
    for await (const part of process.stdin) input += part;
    await run(JSON.parse(input));
  } catch (error) { emit({ error: error instanceof Error ? error.message : String(error) }); process.exitCode = 1; }
}
