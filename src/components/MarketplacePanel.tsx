import React, { useState, useEffect, useCallback } from 'react';
import {
  Search, ArrowLeft, Sparkles, Star, Download, Eye, X, Filter,
  Code, Palette, Database, Globe, Terminal, Zap, BookOpen,
  Check, Loader2, ExternalLink, Tag, ChevronDown, Layers
} from 'lucide-react';
import MarkdownRenderer from './MarkdownRenderer';
import { getMarketplaceSkills, getMarketplaceSkillDetail, importMarketplaceSkill } from '../api';

interface MarketplaceSkill {
  id: string;
  name: string;
  description: string;
  author?: string;
  category?: string;
  tags?: string[];
  downloads?: number;
  stars?: number;
  source?: string;
  content?: string;
  readme?: string;
  is_featured?: boolean;
}

interface MarketplacePanelProps {
  onBack: () => void;
  onImportComplete: () => void;
}

const CATEGORIES = [
  { id: 'all', name: 'All', icon: Layers },
  { id: 'code', name: 'Code & Dev', icon: Code },
  { id: '写作', name: '写作', icon: BookOpen },
  { id: 'data', name: 'Data & Analytics', icon: Database },
  { id: 'web', name: 'Web & API', icon: Globe },
  { id: 'devops', name: 'DevOps', icon: Terminal },
  { id: 'productivity', name: 'Productivity', icon: Zap },
  { id: 'creative', name: 'Creative', icon: Palette },
];

const MarketplacePanel: React.FC<MarketplacePanelProps> = ({ onBack, onImportComplete }) => {
  const [skills, setSkills] = useState<MarketplaceSkill[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState('all');
  const [selectedSkill, setSelectedSkill] = useState<MarketplaceSkill | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importedIds, setImportedIds] = useState<Set<string>>(new Set());

  const fetchSkills = useCallback(async () => {
    setLoading(true);
    try {
      const data = await getMarketplaceSkills({
        q: searchQuery,
        category: selectedCategory !== 'all' ? selectedCategory : undefined,
      });
      setSkills(data.skills || []);
    } catch (e) {
      console.error('Failed to fetch marketplace skills:', e);
      setSkills([]);
    }
    setLoading(false);
  }, [searchQuery, selectedCategory]);

  useEffect(() => {
    fetchSkills();
  }, [fetchSkills]);

  const handlePreview = async (skill: MarketplaceSkill) => {
    setSelectedSkill(skill);
    setPreviewLoading(true);
    try {
      const detail = await getMarketplaceSkillDetail(skill.id);
      setSelectedSkill(prev => prev ? { ...prev, ...detail, readme: detail.readme || detail.content } : null);
    } catch (e) {
      console.error('Failed to fetch skill detail:', e);
    }
    setPreviewLoading(false);
  };

  const handleImport = async (skill: MarketplaceSkill) => {
    setImporting(true);
    try {
      await importMarketplaceSkill(skill.id, skill.name);
      setImportedIds(prev => new Set([...prev, skill.id]));
      setTimeout(() => {
        onImportComplete();
      }, 1500);
    } catch (e) {
      console.error('Failed to import skill:', e);
      alert('Import failed. Please try again.');
    }
    setImporting(false);
  };

  const closePreview = () => {
    setSelectedSkill(null);
  };

  return (
    <div className="flex h-full w-full bg-claude-bg text-claude-text font-sans">
      {/* Left: Skill List */}
      <div className="w-[400px] border-r border-claude-border flex flex-col flex-shrink-0 bg-claude-bg">
        {/* Header */}
        <div className="flex-shrink-0 border-b border-claude-border">
          <div className="flex items-center gap-3 px-4 py-4">
            <button
              onClick={onBack}
              className="p-1.5 rounded-md hover:bg-claude-hover text-claude-textSecondary hover:text-claude-text transition-colors"
            >
              <ArrowLeft size={20} />
            </button>
            <div className="flex items-center gap-2">
              <Sparkles size={18} className="text-amber-500" />
              <span className="text-[16px] font-semibold text-claude-text">Skills Marketplace</span>
            </div>
          </div>

          {/* Search */}
          <div className="px-4 pb-3">
            <div className="relative">
              <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-claude-textSecondary" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Search skills..."
                className="w-full pl-9 pr-4 py-2 bg-claude-input border border-claude-border rounded-lg text-[14px] text-claude-text placeholder:text-claude-textSecondary/50 outline-none focus:border-blue-500 transition-colors"
              />
              {searchQuery && (
                <button
                  onClick={() => setSearchQuery('')}
                  className="absolute right-3 top-1/2 -translate-y-1/2 text-claude-textSecondary hover:text-claude-text"
                >
                  <X size={14} />
                </button>
              )}
            </div>
          </div>

          {/* Categories */}
          <div className="px-4 pb-3 overflow-x-auto">
            <div className="flex gap-2">
              {CATEGORIES.map((cat) => {
                const Icon = cat.icon;
                return (
                  <button
                    key={cat.id}
                    onClick={() => setSelectedCategory(cat.id)}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded-full text-[12px] font-medium whitespace-nowrap transition-colors ${
                      selectedCategory === cat.id
                        ? 'bg-blue-600 text-white'
                        : 'bg-claude-input text-claude-textSecondary hover:bg-claude-hover hover:text-claude-text'
                    }`}
                  >
                    <Icon size={12} />
                    {cat.name}
                  </button>
                );
              })}
            </div>
          </div>
        </div>

        {/* Skills List */}
        <div className="flex-1 overflow-y-auto p-3 space-y-2">
          {loading ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 size={24} className="animate-spin text-claude-textSecondary" />
            </div>
          ) : skills.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-center">
              <Search size={32} className="text-claude-textSecondary/30 mb-3" />
              <p className="text-[14px] text-claude-textSecondary">No skills found</p>
              <p className="text-[12px] text-claude-textSecondary/60 mt-1">Try a different search or category</p>
            </div>
          ) : (
            skills.map((skill) => (
              <div
                key={skill.id}
                className="p-3 rounded-xl border border-claude-border bg-white dark:bg-[#1a1a1a] hover:border-claude-textSecondary/30 hover:shadow-sm transition-all cursor-pointer group"
                onClick={() => handlePreview(skill)}
              >
                <div className="flex items-start justify-between mb-2">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <h3 className="text-[14px] font-semibold text-claude-text truncate">{skill.name}</h3>
                      {skill.is_featured && (
                        <span className="flex items-center gap-0.5 px-1.5 py-0.5 bg-amber-100 dark:bg-amber-900/30 text-amber-700 dark:text-amber-400 text-[10px] font-medium rounded">
                          <Star size={8} />
                          Featured
                        </span>
                      )}
                    </div>
                    <p className="text-[12px] text-claude-textSecondary line-clamp-2 leading-relaxed">
                      {skill.description || 'No description available'}
                    </p>
                  </div>
                </div>

                {/* Tags & Stats */}
                <div className="flex items-center justify-between mt-3">
                  <div className="flex items-center gap-2 flex-wrap">
                    {skill.tags?.slice(0, 3).map((tag) => (
                      <span
                        key={tag}
                        className="px-2 py-0.5 bg-claude-input text-claude-textSecondary text-[10px] font-medium rounded-full"
                      >
                        {tag}
                      </span>
                    ))}
                    {skill.category && (
                      <span className="px-2 py-0.5 bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-400 text-[10px] font-medium rounded-full">
                        {skill.category}
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-3 text-[11px] text-claude-textSecondary">
                    {skill.downloads !== undefined && (
                      <span className="flex items-center gap-0.5">
                        <Download size={10} />
                        {skill.downloads >= 1000 ? `${(skill.downloads / 1000).toFixed(1)}k` : skill.downloads}
                      </span>
                    )}
                    {skill.stars !== undefined && (
                      <span className="flex items-center gap-0.5">
                        <Star size={10} />
                        {skill.stars >= 1000 ? `${(skill.stars / 1000).toFixed(1)}k` : skill.stars}
                      </span>
                    )}
                  </div>
                </div>

                {/* Author */}
                {skill.author && (
                  <div className="flex items-center gap-1.5 mt-2 pt-2 border-t border-claude-border/50">
                    <div className="w-4 h-4 rounded-full bg-claude-input flex items-center justify-center text-[8px] font-medium text-claude-textSecondary">
                      {skill.author.charAt(0).toUpperCase()}
                    </div>
                    <span className="text-[11px] text-claude-textSecondary">{skill.author}</span>
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </div>

      {/* Right: Preview Panel */}
      <div className="flex-1 flex flex-col min-w-0 bg-claude-bg">
        {selectedSkill ? (
          <>
            {/* Preview Header */}
            <div className="flex-shrink-0 px-8 py-6 border-b border-claude-border">
              <div className="flex items-start justify-between mb-4">
                <div>
                  <h2 className="text-xl font-bold text-claude-text mb-1">{selectedSkill.name}</h2>
                  <p className="text-[14px] text-claude-textSecondary">{selectedSkill.description}</p>
                </div>
                <button
                  onClick={closePreview}
                  className="p-1.5 rounded-md hover:bg-claude-hover text-claude-textSecondary hover:text-claude-text transition-colors"
                >
                  <X size={20} />
                </button>
              </div>

              {/* Meta info */}
              <div className="flex items-center gap-4 flex-wrap">
                {selectedSkill.author && (
                  <div className="flex items-center gap-1.5 text-[13px] text-claude-textSecondary">
                    <span>by</span>
                    <span className="font-medium text-claude-text">{selectedSkill.author}</span>
                  </div>
                )}
                {selectedSkill.category && (
                  <span className="px-2.5 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-400 text-[12px] font-medium rounded-lg">
                    {selectedSkill.category}
                  </span>
                )}
                {selectedSkill.tags?.map((tag) => (
                  <span
                    key={tag}
                    className="flex items-center gap-1 px-2.5 py-1 bg-claude-input text-claude-textSecondary text-[12px] font-medium rounded-lg"
                  >
                    <Tag size={10} />
                    {tag}
                  </span>
                ))}
              </div>

              {/* Actions */}
              <div className="flex items-center gap-3 mt-5">
                <button
                  onClick={() => handleImport(selectedSkill)}
                  disabled={importing || importedIds.has(selectedSkill.id)}
                  className={`flex items-center gap-2 px-5 py-2.5 rounded-lg font-medium text-[14px] transition-colors ${
                    importedIds.has(selectedSkill.id)
                      ? 'bg-green-600 text-white cursor-default'
                      : importing
                      ? 'bg-blue-600 text-white cursor-wait'
                      : 'bg-blue-600 text-white hover:bg-blue-700'
                  }`}
                >
                  {importing ? (
                    <>
                      <Loader2 size={16} className="animate-spin" />
                      Importing...
                    </>
                  ) : importedIds.has(selectedSkill.id) ? (
                    <>
                      <Check size={16} />
                      Imported
                    </>
                  ) : (
                    <>
                      <Download size={16} />
                      Import to My Skills
                    </>
                  )}
                </button>
                <button
                  onClick={() => window.open(`https://github.com/anthropics/skills/tree/main/${selectedSkill.id}`, '_blank')}
                  className="flex items-center gap-2 px-4 py-2.5 border border-claude-border rounded-lg text-claude-text hover:bg-claude-hover transition-colors text-[14px]"
                >
                  <ExternalLink size={14} />
                  View on GitHub
                </button>
              </div>
            </div>

            {/* Preview Content */}
            <div className="flex-1 overflow-y-auto px-8 py-6">
              {previewLoading ? (
                <div className="flex items-center justify-center h-64">
                  <Loader2 size={32} className="animate-spin text-claude-textSecondary" />
                </div>
              ) : (
                <div className="border border-claude-border rounded-xl bg-white dark:bg-[#30302E] overflow-hidden shadow-sm">
                  <div className="px-4 py-3 border-b border-claude-border bg-claude-bg/50">
                    <div className="flex items-center gap-2 text-[13px] font-medium text-claude-textSecondary">
                      <Eye size={14} />
                      Preview
                    </div>
                  </div>
                  <div className="p-6">
                    {selectedSkill.readme || selectedSkill.content ? (
                      <div className="prose prose-sm max-w-none dark:prose-invert">
                        <MarkdownRenderer content={selectedSkill.readme || selectedSkill.content || ''} />
                      </div>
                    ) : (
                      <p className="text-[14px] text-claude-textSecondary italic">No preview available</p>
                    )}
                  </div>
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <div className="w-20 h-20 rounded-2xl bg-claude-input flex items-center justify-center mb-4">
              <Eye size={32} className="text-claude-textSecondary/40" />
            </div>
            <h3 className="text-[16px] font-medium text-claude-text mb-2">Select a skill to preview</h3>
            <p className="text-[14px] text-claude-textSecondary max-w-sm">
              Click on any skill from the list to see its details and preview the content
            </p>
          </div>
        )}
      </div>
    </div>
  );
};

export default MarketplacePanel;