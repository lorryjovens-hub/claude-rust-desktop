import { request } from './client';

export interface MemoryItem {
  id: string;
  content: string;
  importance: number;
  created_at: string;
  metadata?: Record<string, string>;
  similarity_score?: number;
}

export interface MemoryStats {
  total_memories: number;
  total_tokens_approx: number;
  backend: string;
}

export interface MemoryConfig {
  enabled: boolean;
  backend_url: string;
  auto_ingest: boolean;
  auto_search: boolean;
  search_top_k: number;
  compression_threshold_tokens: number;
  min_importance_threshold: number;
}

async function json<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await request(`/memory${path}`, options);
  return response.json();
}

export const memoryApi = {
  exportArchive: () => json<{ version: number; memories: MemoryItem[] }>('/export'),
  importArchive: (archive: unknown, preview: boolean) => json<{ imported: number; skipped: number; preview: boolean; backup: string | null }>('/import', { method: 'POST', body: JSON.stringify({ archive, preview }) }),
  stats: () => json<MemoryStats>('/stats'),
  config: () => json<MemoryConfig>('/config'),
  search: (query: string, top_k = 50) => json<MemoryItem[]>('/search', { method: 'POST', body: JSON.stringify({ query, top_k }) }),
  ingest: (content: string, importance: number) => json('/ingest', { method: 'POST', body: JSON.stringify({ content, importance, metadata: { type: 'manual' } }) }),
  saveConfig: (config: MemoryConfig) => json('/config', { method: 'PUT', body: JSON.stringify(config) }),
  clear: () => json('/clear', { method: 'POST' }),
};
