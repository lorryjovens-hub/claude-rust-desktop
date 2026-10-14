import { useState } from 'react';
import { Puzzle, Search } from 'lucide-react';
import { useI18n } from '../hooks/useI18n';

const categories = ['webTools', 'codeTools', 'aiModels', 'fileTools', 'communication', 'dataAnalytics'] as const;

export default function PluginsPage() {
  const { t } = useI18n();
  const [search, setSearch] = useState('');
  const query = search.trim().toLocaleLowerCase();
  const filtered = categories.filter(category =>
    `${t(`customize.pluginCategories.${category}.name`)} ${t(`customize.pluginCategories.${category}.description`)}`
      .toLocaleLowerCase().includes(query),
  );

  return (
    <section className="max-w-4xl space-y-5">
      <div>
        <h2 className="text-xl font-medium">{t('customize.plugins')}</h2>
        <p className="mt-2 text-sm text-claude-textSecondary">{t('customize.pluginsDescription')}</p>
      </div>
      <div className="relative">
        <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-claude-textSecondary" />
        <input type="search" aria-label={t('customize.searchPlugins')} placeholder={t('customize.searchPlugins')}
          value={search} onChange={event => setSearch(event.target.value)}
          className="w-full pl-9 pr-4 py-2.5 bg-claude-input border border-claude-border rounded-xl text-sm outline-none focus:border-claude-textSecondary" />
      </div>
      <div className="grid grid-cols-1 xl:grid-cols-2 gap-3">
        {filtered.map(category => (
          <article key={category} className="flex items-start gap-3 p-4 bg-claude-input border border-claude-border rounded-xl">
            <Puzzle size={20} className="shrink-0 mt-1 text-claude-textSecondary" />
            <div>
              <h3 className="text-sm font-semibold">{t(`customize.pluginCategories.${category}.name`)}</h3>
              <p className="mt-2 text-sm text-claude-textSecondary">{t(`customize.pluginCategories.${category}.description`)}</p>
            </div>
          </article>
        ))}
      </div>
      {filtered.length === 0 && <p role="status" className="py-8 text-sm text-claude-textSecondary">{t('customize.noPluginsFound')}</p>}
    </section>
  );
}
