export type Effort = 'off' | 'low' | 'high' | 'max';

const modelNameCollator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
export function compareModelNames(a: { name: string; id: string }, b: { name: string; id: string }): number {
  return modelNameCollator.compare(a.name, b.name) || modelNameCollator.compare(a.id, b.id);
}

export function parseTokenLimit(value: string): number | undefined {
  if (!value.trim()) return undefined;
  const match = value.trim().match(/^(\d+(?:\.\d+)?)\s*([km])?$/i);
  if (!match) throw new Error('请输入正整数或 K / M 格式，例如 256K、1M');
  const count = Number(match[1]) * ({ k: 1000, m: 1000000 }[match[2]?.toLowerCase() || ''] || 1);
  if (!Number.isSafeInteger(count) || count < 1 || count > 4294967295) throw new Error('Token 数量超出有效范围');
  return count;
}

export function formatTokenLimit(value?: number): string {
  if (!value) return '';
  if (value % 1000000 === 0) return `${value / 1000000}M`;
  if (value % 1000 === 0) return `${value / 1000}K`;
  return String(value);
}

export const modelKey = (providerId: string, modelId: string) => `${encodeURIComponent(providerId)}::${modelId}`;
export function splitModelKey(value: string): { providerId?: string; modelId: string } {
  const separator = value.indexOf('::');
  if (separator < 0) return { modelId: value };
  return { providerId: decodeURIComponent(value.slice(0, separator)), modelId: value.slice(separator + 2) };
}
