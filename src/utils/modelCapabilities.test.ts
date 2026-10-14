import { describe, expect, it } from 'vitest';
import { completeModelCapabilities, inferModelCapabilities, mergeDiscoveredModel } from './modelCapabilities';

describe('automatic model capabilities', () => {
  it('recognizes catalog IDs, display names, gateway prefixes and dated thinking aliases', () => {
    for (const id of ['gpt-4o', 'openai/gpt-4o', 'gpt-4o-20240806-thinking']) {
      expect(inferModelCapabilities({ id, name: '' })).toMatchObject({ contextWindow: 128000, inputTypes: ['text', 'image'] });
    }
    expect(inferModelCapabilities({ id: 'gateway-alias', name: 'GPT-4o' }).inputTypes).toEqual(['text', 'image']);
    expect(inferModelCapabilities({ id: 'claude-sonnet-4-5', name: '' }).contextWindow).toBe(1000000);
    expect(inferModelCapabilities({ id: 'gpt-4', name: '' }).inputTypes).toEqual(['text']);
  });

  it('uses visual keywords and context labels without mistaking parameter counts for tokens', () => {
    expect(inferModelCapabilities({ id: 'custom-vl-72b-256k', name: 'Custom' })).toMatchObject({ contextWindow: 256000, inputTypes: ['text', 'image'] });
    expect(inferModelCapabilities({ id: 'unknown', name: 'Custom Vision (1.5M)' }).contextWindow).toBe(1500000);
    expect(inferModelCapabilities({ id: 'custom-72b', name: 'Custom' })).toEqual({ contextWindow: undefined, maxTokens: undefined, inputTypes: ['text'] });
    expect(inferModelCapabilities({ id: 'future-gpt-900', name: '' }).contextWindow).toBeUndefined();
    expect(inferModelCapabilities({ id: 'gpt-4o', name: 'GPT-4o 8K' })).toMatchObject({ contextWindow: 8000, maxTokens: 8000 });
  });

  it('gives discovery metadata priority including explicit text-only capability', () => {
    expect(completeModelCapabilities({ id: 'gpt-4o', name: 'GPT', contextWindow: 64000, maxTokens: 90000, inputTypes: ['text'] }))
      .toMatchObject({ contextWindow: 64000, maxTokens: 64000, inputTypes: ['text'] });
  });

  it('refreshes stale defaults while keeping the display name and enabled choice', () => {
    const old = { id: 'gpt-4o', name: 'My model', enabled: false, contextWindow: 8192, inputTypes: ['text'] };
    expect(mergeDiscoveredModel({ id: 'gpt-4o', name: 'GPT-4o', enabled: true }, old))
      .toMatchObject({ name: 'My model', enabled: false, contextWindow: 128000, inputTypes: ['text', 'image'] });
    expect(mergeDiscoveredModel({ id: 'gpt-4o', name: 'GPT-4o', contextWindow: 32000, inputTypes: ['text'] }, old))
      .toMatchObject({ contextWindow: 32000, inputTypes: ['text'] });
  });

  it('keeps existing limits for unknown models but applies visual name hints', () => {
    const old = { id: 'custom-vl', name: 'My model', contextWindow: 42000, inputTypes: ['text'] };
    expect(mergeDiscoveredModel({ id: 'custom-vl', name: 'Custom' }, old))
      .toMatchObject({ contextWindow: 42000, inputTypes: ['text', 'image'] });
    expect(mergeDiscoveredModel({ id: 'private-model', name: 'Private' }, { ...old, id: 'private-model', inputTypes: ['image'] }))
      .toMatchObject({ contextWindow: 42000, inputTypes: ['image'] });
  });
});
