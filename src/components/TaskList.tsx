import React from 'react';
import { Plus, Clock } from 'lucide-react';
import TaskRow from './TaskRow';
import { TaskInfo } from '../utils/tauriAPI';
import { useI18n } from '../hooks/useI18n';

interface TaskListProps {
  tasks: TaskInfo[];
  onEdit: (task: TaskInfo) => void;
  onDelete: (id: string) => void;
  onExecute: (id: string) => void;
  onToggle: (id: string, enabled: boolean) => void;
  onAdd: () => void;
}

export default function TaskList({
  tasks,
  onEdit,
  onDelete,
  onExecute,
  onToggle,
  onAdd,
}: TaskListProps) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col h-full bg-claude-bg">
      <div className="flex items-center justify-between px-4 py-3 border-b border-claude-border">
        <div className="flex items-center gap-2">
          <Clock size={16} className="text-claude-textSecondary" />
          <h2 className="text-[15px] font-semibold text-claude-text">{t('tasks.title')}</h2>
          {tasks.length > 0 && (
            <span className="text-[11px] text-claude-textSecondary bg-claude-hover px-1.5 py-0.5 rounded-full">
              {tasks.length}
            </span>
          )}
        </div>
        <button
          onClick={onAdd}
          className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium text-claude-text bg-claude-input border border-claude-border rounded-lg hover:bg-claude-btn-hover transition-colors"
        >
          <Plus size={14} />
          {t('tasks.newTask')}
        </button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {tasks.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-3 text-claude-textSecondary">
            <Clock size={32} className="opacity-40" />
            <p className="text-[13px]">{t('tasks.empty')}</p>
            <button
              onClick={onAdd}
              className="px-4 py-2 text-[12px] font-medium text-claude-accent hover:underline"
            >
              {t('tasks.createFirst')}
            </button>
          </div>
        ) : (
          tasks.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              onToggle={onToggle}
              onEdit={() => onEdit(task)}
              onDelete={() => onDelete(task.id)}
              onExecute={() => onExecute(task.id)}
            />
          ))
        )}
      </div>
    </div>
  );
}