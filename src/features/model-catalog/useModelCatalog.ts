import { useEffect, useCallback } from 'react';
import { useChatStore } from '../../stores/useChatStore';
import { getUserModels, getProviders } from '../../api';
import { modelKey, splitModelKey } from '../../utils/modelConfig';
import { stripThinking } from '../../utils/modelHelpers';
import { SelectableModel } from '../../components/ModelSelector';

export function useModelCatalog(viewingIdRef: React.MutableRefObject<string | null>) {
  const { modelCatalog, setModelCatalog, currentModel, setCurrentModel, providersCache, setProvidersCache } = useChatStore();
  const isSelfHostedMode = localStorage.getItem('user_mode') === 'selfhosted';
  const selectorModels = (modelCatalog?.all || []) as SelectableModel[];
  const isModelSelectable = useCallback((id: string) => selectorModels.some(m => m.id === id && Number(m.enabled) === 1), [modelCatalog]);
  const resolveModelForNewChat = useCallback((preferred?: string | null) => {
    const saved = preferred || localStorage.getItem('default_model') || '';
    if (isModelSelectable(saved)) return saved;
    // Migrate legacy unqualified selections only when their provider is unambiguous.
    const matches = selectorModels.filter(m => splitModelKey(m.id).modelId === stripThinking(saved));
    return matches.length === 1 ? matches[0].id : selectorModels.find(m => Number(m.enabled) === 1)?.id || '';
  }, [modelCatalog]);
  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        if (!isSelfHostedMode) { const data = await getUserModels(); if (!cancelled) setModelCatalog(data); return; }
        const providers = await getProviders();
        if (cancelled) return;
        setProvidersCache(providers);
        const all = providers.filter(p => p.enabled).flatMap(p => p.models.filter(m => m.enabled !== false).map(m => ({ ...m, id: modelKey(p.id, m.id), modelId: m.id, providerId: p.id, providerName: p.name, enabled: 1 })));
        setModelCatalog({ all, common: all, fallback_model: all[0]?.id || null });
        if (!viewingIdRef.current) setCurrentModel(previous => {
          const saved = previous || localStorage.getItem('default_model') || '';
          if (all.some(m => m.id === saved)) return saved;
          const legacy = all.filter(m => m.modelId === stripThinking(saved));
          return legacy.length === 1 ? legacy[0].id : all[0]?.id || '';
        });
      } catch { /* Settings displays connection errors; keep the last known catalog. */ }
    };
    load();
    window.addEventListener('providers-changed', load);
    const timer = setInterval(load, 60000);
    return () => { cancelled = true; clearInterval(timer); window.removeEventListener('providers-changed', load); };
  }, [isSelfHostedMode]);
  const providerId = splitModelKey(currentModel).providerId;
  return { selectorModels, isModelSelectable, resolveModelForNewChat, isSelfHostedMode, selfHostedModels: selectorModels,
    currentProviderSupportsWebSearch: providersCache.find(p => p.id === providerId)?.supportsWebSearch === true };
}
