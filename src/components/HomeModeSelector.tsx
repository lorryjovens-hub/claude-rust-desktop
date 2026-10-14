import type { WorkspaceMode } from '../stores/useWorkspaceModeStore';

export type HomeMode = Exclude<WorkspaceMode, 'code'>;

export default function HomeModeSelector({ mode, onChange, disabled = false }: {
  mode: HomeMode;
  onChange: (mode: HomeMode) => void;
  disabled?: boolean;
}) {
  return <div role="group" aria-label="Home mode" className="flex items-center rounded-lg bg-claude-bg p-0.5 text-xs">
    {(['chat', 'work', 'study'] as const).map(value => <button
      key={value}
      type="button"
      aria-pressed={mode === value}
      disabled={disabled}
      onClick={() => onChange(value)}
      className={`rounded-md border px-2.5 py-1 transition-colors disabled:opacity-50 ${mode === value ? 'border-claude-border bg-claude-input text-claude-text shadow-sm' : 'border-transparent text-claude-textSecondary hover:text-claude-text'}`}
    >{value === 'chat' ? 'Chat' : value === 'work' ? 'Work' : 'Study'}</button>)}
  </div>;
}
