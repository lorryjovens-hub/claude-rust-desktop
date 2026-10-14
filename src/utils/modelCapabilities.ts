import type { ProviderModel } from '../api';
import catalog from '../data/providerCatalog.json';
import { parseTokenLimit } from './modelConfig';

type Capabilities = Pick<ProviderModel, 'contextWindow' | 'maxTokens' | 'inputTypes'>;
const normalize = (value: string) => value.trim().toLowerCase().replace(/[\s_.]+/g, '-');
const index = new Map<string, Capabilities>();
const primaryProviders = new Set(['anthropic', 'openai', 'google', 'deepseek', 'xai', 'mistral', 'moonshotai', 'minimax', 'zai']);
// Prefer the model owner's entry over gateway copies with different limits.
for (const provider of catalog.providers) {
  for (const model of provider.models) {
    for (const key of [model.id, model.name]) {
      const normalized = normalize(key);
      if (!index.has(normalized) || primaryProviders.has(provider.id)) {
        index.set(normalized, model);
      }
    }
  }
}
for (const [key, model] of [...index]) {
  const alias = key.replace(/(?:(?:-\d{8})|(?:-(?:latest|thinking))|(?:-\(latest\)))+$/, '');
  if (!index.has(alias)) index.set(alias, model);
}

function lookup(value: string): Capabilities | undefined {
  const key = normalize(value);
  const candidates = [key, key.replace(/^(?:models\/|[^/]+\/)/, '')];
  for (const candidate of candidates) {
    const exact = index.get(candidate);
    if (exact) return exact;
  }
  // Only remove recognized aliases, never arbitrary version or variant suffixes.
  for (const candidate of candidates) {
    const base = candidate.replace(/(?:(?:-\d{8})|(?:-(?:latest|thinking)))+$/, '');
    if (base !== candidate && index.has(base)) return index.get(base);
  }
  return undefined;
}

export function inferModelCapabilities(model: Pick<ProviderModel, 'id' | 'name'>): Capabilities {
  const known = lookup(model.id) || lookup(model.name);
  const identity = `${model.id} ${model.name}`;
  const limit = identity.match(/\b(\d+(?:\.\d+)?\s*[km])\b/i);
  let namedContext: number | undefined;
  if (limit) {
    try { namedContext = parseTokenLimit(limit[1]); } catch { /* Keep unknown limits empty. */ }
  }
  // A size such as 72B is a parameter count, not a context window.
  const vision = /(?:^|[\s\-_/])(vision|vl|vlm|multimodal)(?:$|[\s\-_/])/i.test(identity);
  const contextWindow = namedContext ?? known?.contextWindow;
  return {
    contextWindow,
    maxTokens: contextWindow && known?.maxTokens ? Math.min(known.maxTokens, contextWindow) : known?.maxTokens,
    inputTypes: known?.inputTypes ? [...known.inputTypes] : vision ? ['text', 'image'] : ['text'],
  };
}

/** Discovery metadata wins; the catalog and model name fill missing fields. */
export function completeModelCapabilities(model: ProviderModel): ProviderModel {
  const inferred = inferModelCapabilities(model);
  const contextWindow = model.contextWindow ?? inferred.contextWindow;
  const maxTokens = model.maxTokens ?? inferred.maxTokens;
  return {
    ...model,
    contextWindow,
    maxTokens: contextWindow && maxTokens ? Math.min(maxTokens, contextWindow) : maxTokens,
    inputTypes: model.inputTypes ?? inferred.inputTypes,
  };
}

export function mergeDiscoveredModel(fetched: ProviderModel, previous?: ProviderModel): ProviderModel {
  const inferred = inferModelCapabilities(fetched);
  return completeModelCapabilities({
    ...previous,
    ...fetched,
    name: previous?.name || fetched.name,
    enabled: previous?.enabled ?? fetched.enabled,
    contextWindow: fetched.contextWindow ?? inferred.contextWindow ?? previous?.contextWindow,
    maxTokens: fetched.maxTokens ?? inferred.maxTokens ?? previous?.maxTokens,
    inputTypes: fetched.inputTypes ?? (lookup(fetched.id) || lookup(fetched.name) || inferred.inputTypes?.includes('image') ? inferred.inputTypes : undefined)
      ?? previous?.inputTypes ?? inferred.inputTypes,
  });
}
