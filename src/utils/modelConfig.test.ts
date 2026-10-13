import { describe, expect, it } from 'vitest';
import { parseTokenLimit, formatTokenLimit, modelKey, splitModelKey } from './modelConfig';

describe('model configuration', () => {
  it('accepts K, M, decimals and integer token counts without losing precision', () => {
    expect(parseTokenLimit('1M')).toBe(1000000);
    expect(parseTokenLimit(' 1.5m ')).toBe(1500000);
    expect(parseTokenLimit('256K')).toBe(256000);
    expect(parseTokenLimit('32768')).toBe(32768);
    expect(formatTokenLimit(1000000)).toBe('1M');
    expect(parseTokenLimit('')).toBeUndefined();
  });
  it('rejects invalid and overflowing token counts', () => {
    for (const value of ['0', '-1', '1e6', '1MB', '0.0001K', '5000M']) expect(() => parseTokenLimit(value)).toThrow();
  });
  it('keeps two providers with the same model ID distinct', () => {
    expect(modelKey('first', 'same')).not.toBe(modelKey('second', 'same'));
    expect(splitModelKey(modelKey('second', 'org/model:1'))).toEqual({ providerId: 'second', modelId: 'org/model:1' });
  });
});
