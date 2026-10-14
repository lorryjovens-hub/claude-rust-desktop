import type { Provider } from '../api';
import { useI18n } from '../hooks/useI18n';
import { useGeneralSettingsStore, type HomeModelMode, type ModeModel } from '../stores/useGeneralSettingsStore';
import { compareModelNames } from '../utils/modelConfig';

export default function ModeModelDefaults({ providers }: { providers: Provider[] }) {
  const { t } = useI18n();
  const defaults = useGeneralSettingsStore(state => state.defaultModelByMode);
  const update = useGeneralSettingsStore(state => state.update);
  const saveError = useGeneralSettingsStore(state => state.saveError);
  const available = providers.filter(provider => provider.enabled && provider.models.some(model => model.enabled !== false)).sort(compareModelNames);
  const save = (mode: HomeModelMode, value: ModeModel | null) => update({ defaultModelByMode: { ...defaults, [mode]: value } });
  const selectClass = 'min-w-0 w-full rounded-lg border border-claude-border bg-claude-input px-3 py-2 text-sm text-claude-text';
  return <fieldset className="mb-6 min-w-0 rounded-2xl border border-claude-border bg-claude-input p-5">
    <legend className="px-2 text-sm font-medium">{t('general.modeModels')}</legend>
    <p className="mb-4 text-xs text-claude-textSecondary">{t('general.modeModelsDescription')}</p>
    {saveError && <p role="alert" className="mb-3 text-sm text-red-500">{saveError}</p>}
    <div className="grid gap-3">
      {(['chat', 'work', 'study'] as const).map(mode => {
        const label = mode[0].toUpperCase() + mode.slice(1);
        const selected = defaults[mode];
        const provider = available.find(entry => entry.id === selected?.providerId);
        const models = [...(provider?.models || [])].filter(model => model.enabled !== false).sort(compareModelNames);
        const missing = selected && (!provider || !models.some(model => model.id === selected.modelId));
        return <div key={mode} className="grid items-center gap-3 sm:grid-cols-[70px_1fr_1fr]">
          <span className="text-sm">{label}</span>
          <label className="min-w-0 text-xs text-claude-textSecondary">{t('general.modelProvider')}
            <select aria-label={`${label} · ${t('general.modelProvider')}`} className={`${selectClass} mt-1`} value={selected?.providerId || ''} onChange={event => {
              const entry = available.find(item => item.id === event.target.value);
              const first = entry && [...entry.models].filter(model => model.enabled !== false).sort(compareModelNames)[0];
              save(mode, entry && first ? { providerId: entry.id, modelId: first.id } : null);
            }}>
              <option value="">{t('general.useCurrentModel')}</option>
              {selected && !provider && <option disabled value={selected.providerId}>{selected.providerId} ({t('general.modelUnavailable')})</option>}
              {available.map(entry => <option key={entry.id} value={entry.id}>{entry.name}</option>)}
            </select>
          </label>
          <label className="min-w-0 text-xs text-claude-textSecondary">Model
            <select aria-label={`${label} · Model`} disabled={!provider} className={`${selectClass} mt-1 disabled:opacity-50`} value={selected?.modelId || ''} onChange={event => {
              if (provider) save(mode, { providerId: provider.id, modelId: event.target.value });
            }}>
              {!selected && <option value="">{t('general.useCurrentModel')}</option>}
              {missing && <option disabled value={selected.modelId}>{selected.modelId} ({t('general.modelUnavailable')})</option>}
              {models.map(model => <option key={model.id} value={model.id}>{model.name}</option>)}
            </select>
          </label>
        </div>;
      })}
    </div>
  </fieldset>;
}
