import { request } from './client';

export interface ChangedFile { path: string; added: number; removed: number; binary: boolean; status: string }
export interface RepositoryChanges {
  workspace_path: string;
  is_repository: boolean;
  branch: string | null;
  github_url: string | null;
  compare_url: string | null;
  added: number;
  removed: number;
  files: ChangedFile[];
}
export interface FileDiff { patch: string; notice: string | null }
export async function getConversationChanges(id: string): Promise<RepositoryChanges> {
  return (await request(`/conversations/${encodeURIComponent(id)}/git`)).json();
}
export async function getConversationDiff(id: string, file: string): Promise<FileDiff> {
  return (await request(`/conversations/${encodeURIComponent(id)}/git/diff?file=${encodeURIComponent(file)}`)).json();
}
export async function openConversationWorkspace(id: string, action: 'explorer' | 'terminal' | 'github' | 'compare'): Promise<void> {
  await request(`/conversations/${encodeURIComponent(id)}/git/action`, { method: 'POST', body: JSON.stringify({ action }) });
}
