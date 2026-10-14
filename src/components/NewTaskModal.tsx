import React, { useState, useEffect } from 'react';
import { X } from 'lucide-react';
import { TaskInfo } from '../utils/tauriAPI';
import { useI18n } from '../hooks/useI18n';

interface NewTaskModalProps {
  open: boolean;
  onClose: () => void;
  onSave: (task: {
    name: string;
    description?: string;
    cron_expression: string;
    task_type: string;
    task_config: string;
    conversation_id?: string;
  }) => void;
  initialTask?: TaskInfo;
}

const CRON_PRESETS = [
  { label: 'tasks.everyMinute', value: '* * * * *' },
  { label: 'tasks.every5Minutes', value: '*/5 * * * *' },
  { label: 'tasks.every15Minutes', value: '*/15 * * * *' },
  { label: 'tasks.everyHour', value: '0 * * * *' },
  { label: 'tasks.midnight', value: '0 0 * * *' },
  { label: 'tasks.daily9', value: '0 9 * * *' },
  { label: 'tasks.monday9', value: '0 9 * * 1' },
  { label: 'tasks.weekday9', value: '0 9 * * 1-5' },
];

const TASK_TYPES = [
  { value: 'prompt', label: 'tasks.prompt' },
  { value: 'webhook', label: 'tasks.webhook' },
  { value: 'system', label: 'tasks.system' },
  { value: 'report', label: 'tasks.report' },
];

export default function NewTaskModal({ open, onClose, onSave, initialTask }: NewTaskModalProps) {
  const { t } = useI18n();
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [cronExpression, setCronExpression] = useState('');
  const [taskType, setTaskType] = useState('prompt');
  const [taskConfig, setTaskConfig] = useState('');
  const [conversationId, setConversationId] = useState('');

  useEffect(() => {
    if (initialTask) {
      setName(initialTask.name);
      setDescription(initialTask.description || '');
      setCronExpression(initialTask.cron_expression);
      setTaskType(initialTask.task_type);
      setTaskConfig(initialTask.task_config);
      setConversationId(initialTask.conversation_id || '');
    }
  }, [initialTask, open]);

  const handleSave = () => {
    if (!name.trim() || !cronExpression.trim() || !taskConfig.trim()) return;
    onSave({
      name: name.trim(),
      description: description.trim() || undefined,
      cron_expression: cronExpression.trim(),
      task_type: taskType,
      task_config: taskConfig.trim(),
      conversation_id: conversationId.trim() || undefined,
    });
    setName('');
    setDescription('');
    setCronExpression('');
    setTaskType('prompt');
    setTaskConfig('');
    setConversationId('');
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/45 px-4">
      <div className="w-full max-w-lg rounded-2xl bg-claude-input border border-claude-border shadow-2xl">
        <div className="flex items-center justify-between px-5 py-4 border-b border-claude-border">
          <h3 className="text-[16px] font-semibold text-claude-text">
            {initialTask ? t('tasks.editTask') : t('tasks.newScheduledTask')}
          </h3>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg hover:bg-claude-btn-hover text-claude-textSecondary hover:text-claude-text transition-colors"
          >
            <X size={18} />
          </button>
        </div>

        <div className="px-5 py-4 space-y-4 max-h-[70vh] overflow-y-auto">
          <div>
            <label className="block text-[12px] font-medium text-claude-textSecondary mb-1.5">
              {t('tasks.name')}
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t('tasks.namePlaceholder')}
              className="w-full px-3 py-2 text-[13px] text-claude-text bg-claude-bg border border-claude-border rounded-lg outline-none focus:border-claude-accent transition-colors placeholder:text-claude-textSecondary/60"
            />
          </div>

          <div>
            <label className="block text-[12px] font-medium text-claude-textSecondary mb-1.5">
              {t('tasks.description')}
            </label>
            <input
              type="text"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder={t('tasks.descriptionPlaceholder')}
              className="w-full px-3 py-2 text-[13px] text-claude-text bg-claude-bg border border-claude-border rounded-lg outline-none focus:border-claude-accent transition-colors placeholder:text-claude-textSecondary/60"
            />
          </div>

          <div>
            <label className="block text-[12px] font-medium text-claude-textSecondary mb-1.5">
              {t('tasks.cron')}
            </label>
            <input
              type="text"
              value={cronExpression}
              onChange={(e) => setCronExpression(e.target.value)}
              placeholder={t('tasks.cronPlaceholder')}
              className="w-full px-3 py-2 text-[13px] font-mono text-claude-text bg-claude-bg border border-claude-border rounded-lg outline-none focus:border-claude-accent transition-colors placeholder:text-claude-textSecondary/60"
            />
            <div className="flex flex-wrap gap-1.5 mt-2">
              {CRON_PRESETS.map((preset) => (
                <button
                  key={preset.value}
                  onClick={() => setCronExpression(preset.value)}
                  className={`px-2 py-1 text-[11px] rounded-md border transition-colors ${
                    cronExpression === preset.value
                      ? 'bg-claude-accent/10 border-claude-accent text-claude-accent'
                      : 'border-claude-border text-claude-textSecondary hover:bg-claude-btn-hover hover:text-claude-text'
                  }`}
                >
                  {t(preset.label)}
                </button>
              ))}
            </div>
          </div>

          <div>
            <label className="block text-[12px] font-medium text-claude-textSecondary mb-1.5">
              {t('tasks.type')}
            </label>
            <select
              value={taskType}
              onChange={(e) => setTaskType(e.target.value)}
              className="w-full px-3 py-2 text-[13px] text-claude-text bg-claude-bg border border-claude-border rounded-lg outline-none focus:border-claude-accent transition-colors"
            >
              {TASK_TYPES.map((type) => (
                <option key={type.value} value={type.value}>
                  {t(type.label)}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-[12px] font-medium text-claude-textSecondary mb-1.5">
              {t('tasks.config')}
            </label>
            <textarea
              value={taskConfig}
              onChange={(e) => setTaskConfig(e.target.value)}
              placeholder={taskType === 'prompt' ? t('tasks.promptPlaceholder') : t('tasks.configPlaceholder')}
              rows={4}
              className="w-full px-3 py-2 text-[13px] text-claude-text bg-claude-bg border border-claude-border rounded-lg outline-none focus:border-claude-accent transition-colors placeholder:text-claude-textSecondary/60 resize-none"
            />
          </div>

          <div>
            <label className="block text-[12px] font-medium text-claude-textSecondary mb-1.5">
              {t('tasks.conversation')}
            </label>
            <input
              type="text"
              value={conversationId}
              onChange={(e) => setConversationId(e.target.value)}
              placeholder={t('tasks.conversationPlaceholder')}
              className="w-full px-3 py-2 text-[13px] text-claude-text bg-claude-bg border border-claude-border rounded-lg outline-none focus:border-claude-accent transition-colors placeholder:text-claude-textSecondary/60"
            />
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-4 border-t border-claude-border">
          <button
            onClick={onClose}
            className="px-4 py-2 text-[13px] font-medium text-claude-textSecondary hover:text-claude-text transition-colors rounded-lg hover:bg-claude-btn-hover"
          >
            {t('common.cancel')}
          </button>
          <button
            onClick={handleSave}
            disabled={!name.trim() || !cronExpression.trim() || !taskConfig.trim()}
            className="px-5 py-2 text-[13px] font-medium text-white bg-claude-accent rounded-lg hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed transition-all"
          >
            {initialTask ? t('tasks.saveChanges') : t('tasks.create')}
          </button>
        </div>
      </div>
    </div>
  );
}