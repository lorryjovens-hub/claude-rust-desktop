import React, { useState } from 'react';
import { Search, SlidersHorizontal, X, RefreshCw, Grid3X3, Loader2, Sparkles } from 'lucide-react';
import { useDesignSkills } from '../../hooks/useDesignSkills';
import SkillCard from './SkillCard';
import SkillStatsPanel from './SkillStatsPanel';
import type { SkillInfo } from '../../services/previewService';

interface SkillGalleryProps {
  onSkillSelect: (skill: SkillInfo) => void;
  onGenerateWithSkill: (skill: SkillInfo) => void;
}

const MODE_LABELS: Record<string, string> = {
  prototype: '原型',
  deck: '幻灯片',
  image: '图像',
  video: '视频',
  audio: '音频',
  review: '评审',
};

const SCENARIO_LABELS: Record<string, string> = {
  design: '设计',
  marketing: '营销',
  operation: '运营',
  engineering: '工程',
  product: '产品',
  finance: '金融',
  hr: '人力资源',
  sale: '销售',
  personal: '个人',
};

const SkillGallery: React.FC<SkillGalleryProps> = ({ onSkillSelect, onGenerateWithSkill }) => {
  const {
    skills,
    filteredSkills,
    loading,
    error,
    modeFilter,
    scenarioFilter,
    searchQuery,
    selectedSkillId,
    availableModes,
    availableScenarios,
    toggleMode,
    toggleScenario,
    setSearchQuery,
    setModeFilter,
    setScenarioFilter,
    setSelectedSkillId,
    refresh,
  } = useDesignSkills();

  const [showFilters, setShowFilters] = useState(false);
  const [detailSkill, setDetailSkill] = useState<SkillInfo | null>(null);

  const handleSelect = (skill: SkillInfo) => {
    const newId = selectedSkillId === skill.id ? null : skill.id;
    setSelectedSkillId(newId);
    if (newId) onSkillSelect(skill);
  };

  const handleGenerate = (skill: SkillInfo) => {
    setSelectedSkillId(skill.id);
    onGenerateWithSkill(skill);
  };

  const hasActiveFilters = modeFilter.length > 0 || scenarioFilter.length > 0 || searchQuery.trim().length > 0;

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center py-20 text-claude-textSecondary">
        <Loader2 size={28} className="animate-spin mb-4 opacity-40" />
        <p className="text-[14px]">加载设计技能中...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center py-20 text-claude-textSecondary">
        <div className="w-12 h-12 rounded-full bg-red-500/10 flex items-center justify-center mb-4">
          <Sparkles size={20} className="text-red-400" />
        </div>
        <p className="text-[14px] font-medium text-claude-text mb-1">加载失败</p>
        <p className="text-[12px] mb-4">{error}</p>
        <button
          onClick={refresh}
          className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium text-claude-text bg-claude-hover rounded-lg hover:bg-claude-border transition-colors"
        >
          <RefreshCw size={12} />
          重试
        </button>
      </div>
    );
  }

  if (skills.length === 0) {
    return (
      <div className="flex flex-col h-full">
        <div className="px-4 py-3 border-b border-claude-border bg-claude-surface/50">
          <div className="flex items-center gap-2">
            <Grid3X3 size={15} className="text-claude-textSecondary" />
            <span className="text-[13px] font-medium text-claude-text">技能画廊</span>
          </div>
        </div>
        <div className="flex flex-1 overflow-hidden">
          <SkillStatsPanel totalSkills={0} />
          <div className="flex-1 flex flex-col items-center justify-center text-claude-textSecondary">
            <div className="w-12 h-12 rounded-xl bg-claude-hover flex items-center justify-center mb-4">
              <Grid3X3 size={20} className="opacity-40" />
            </div>
            <p className="text-[14px] font-medium text-claude-text mb-1">暂无设计技能</p>
            <p className="text-[12px]">在 SKILL.md 文件中添加 <code className="px-1 py-0.5 bg-claude-hover rounded text-[11px]">od:</code> 元数据以启用设计技能</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="px-4 py-3 border-b border-claude-border bg-claude-surface/50">
        <div className="flex items-center gap-3">
          <div className="relative flex-1">
            <Search size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-claude-textSecondary" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="搜索技能名称、描述或示例提示..."
              className="w-full pl-8 pr-3 py-1.5 bg-claude-input border border-claude-border rounded-lg text-[13px] text-claude-text focus:outline-none focus:border-[#8B5CF6]/50 placeholder:text-claude-textSecondary/60"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery('')}
                className="absolute right-2 top-1/2 -translate-y-1/2 p-0.5 hover:bg-claude-hover rounded"
              >
                <X size={12} className="text-claude-textSecondary" />
              </button>
            )}
          </div>

          <button
            onClick={() => setShowFilters(!showFilters)}
            className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-[12px] font-medium transition-colors ${
              hasActiveFilters
                ? 'bg-[#8B5CF6]/10 text-[#8B5CF6] border border-[#8B5CF6]/20'
                : 'text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover'
            }`}
          >
            <SlidersHorizontal size={13} />
            筛选
            {hasActiveFilters && (
              <span className="w-4 h-4 rounded-full bg-[#8B5CF6] text-white text-[9px] flex items-center justify-center">
                {modeFilter.length + scenarioFilter.length + (searchQuery ? 1 : 0)}
              </span>
            )}
          </button>

          <button
            onClick={refresh}
            className="p-1.5 text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
            title="刷新"
          >
            <RefreshCw size={13} />
          </button>
        </div>

        {showFilters && (
          <div className="mt-3 pt-3 border-t border-claude-border space-y-2">
            {availableModes.length > 0 && (
              <div>
                <p className="text-[11px] text-claude-textSecondary mb-1.5">模式</p>
                <div className="flex flex-wrap gap-1.5">
                  {availableModes.map(mode => (
                    <button
                      key={mode}
                      onClick={() => toggleMode(mode)}
                      className={`px-2.5 py-1 rounded-md text-[11px] font-medium transition-colors ${
                        modeFilter.includes(mode)
                          ? 'bg-[#8B5CF6] text-white'
                          : 'bg-claude-hover text-claude-textSecondary hover:text-claude-text'
                      }`}
                    >
                      {MODE_LABELS[mode] || mode}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {availableScenarios.length > 0 && (
              <div>
                <p className="text-[11px] text-claude-textSecondary mb-1.5">场景</p>
                <div className="flex flex-wrap gap-1.5">
                  {availableScenarios.map(scenario => (
                    <button
                      key={scenario}
                      onClick={() => toggleScenario(scenario)}
                      className={`px-2.5 py-1 rounded-md text-[11px] font-medium transition-colors ${
                        scenarioFilter.includes(scenario)
                          ? 'bg-emerald-500 text-white'
                          : 'bg-claude-hover text-claude-textSecondary hover:text-claude-text'
                      }`}
                    >
                      {SCENARIO_LABELS[scenario] || scenario}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {hasActiveFilters && (
              <button
                onClick={() => {
                  setSearchQuery('');
                  setModeFilter([]);
                  setScenarioFilter([]);
                }}
                className="text-[11px] text-[#8B5CF6] hover:underline"
              >
                清除所有筛选
              </button>
            )}
          </div>
        )}
      </div>

      <div className="flex flex-1 overflow-hidden">
        <SkillStatsPanel totalSkills={skills.length} />

        <div className="flex-1 overflow-y-auto p-4">
          {filteredSkills.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-16 text-claude-textSecondary">
              <Search size={24} className="opacity-30 mb-3" />
              <p className="text-[14px]">无匹配的设计技能</p>
              <p className="text-[12px] mt-1">尝试调整筛选条件或搜索词</p>
            </div>
          ) : (
            <>
              <div className="flex items-center justify-between mb-3">
                <p className="text-[12px] text-claude-textSecondary">
                  共 {filteredSkills.length} 个技能
                  {hasActiveFilters && <span className="text-[#8B5CF6]">（已筛选）</span>}
                </p>
              </div>
              <div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-3">
                {filteredSkills.map(skill => (
                  <SkillCard
                    key={skill.id}
                    skill={skill}
                    isSelected={selectedSkillId === skill.id}
                    onSelect={handleSelect}
                    onViewDetail={() => setDetailSkill(skill)}
                  />
                ))}
              </div>
            </>
          )}
        </div>
      </div>

      {detailSkill && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={() => setDetailSkill(null)}>
          <div
            className="bg-claude-surface border border-claude-border rounded-2xl max-w-lg w-full max-h-[80vh] overflow-y-auto shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between p-4 border-b border-claude-border">
              <h3 className="text-[15px] font-semibold text-claude-text">{detailSkill.name}</h3>
              <button onClick={() => setDetailSkill(null)} className="p-1 hover:bg-claude-hover rounded-lg transition-colors">
                <X size={16} className="text-claude-textSecondary" />
              </button>
            </div>

            <div className="p-4 space-y-4">
              <p className="text-[13px] text-claude-textSecondary leading-relaxed">{detailSkill.description}</p>

              {detailSkill.od_metadata && (
                <div className="space-y-3">
                  {detailSkill.od_metadata.example_prompt && (
                    <div>
                      <p className="text-[11px] font-medium text-claude-textSecondary mb-1">示例提示</p>
                      <p className="text-[12px] text-claude-text bg-claude-input border border-claude-border rounded-lg p-2.5 leading-relaxed">
                        {detailSkill.od_metadata.example_prompt}
                      </p>
                    </div>
                  )}

                  {detailSkill.od_metadata.default_for && detailSkill.od_metadata.default_for.length > 0 && (
                    <div>
                      <p className="text-[11px] font-medium text-claude-textSecondary mb-1">适用场景</p>
                      <div className="flex flex-wrap gap-1.5">
                        {detailSkill.od_metadata.default_for.map((d, i) => (
                          <span key={i} className="px-2 py-0.5 bg-claude-hover text-claude-textSecondary rounded-md text-[11px]">{d}</span>
                        ))}
                      </div>
                    </div>
                  )}

                  {detailSkill.od_metadata.outputs && (
                    <div>
                      <p className="text-[11px] font-medium text-claude-textSecondary mb-1">输出格式</p>
                      <div className="flex flex-wrap gap-1.5">
                        {detailSkill.od_metadata.outputs.primary && (
                          <span className="px-2 py-0.5 bg-blue-500/10 text-blue-400 rounded-md text-[11px]">{detailSkill.od_metadata.outputs.primary}</span>
                        )}
                        {detailSkill.od_metadata.outputs.secondary && (
                          <span className="px-2 py-0.5 bg-purple-500/10 text-purple-400 rounded-md text-[11px]">{detailSkill.od_metadata.outputs.secondary}</span>
                        )}
                      </div>
                    </div>
                  )}

                  {detailSkill.od_metadata.inputs && detailSkill.od_metadata.inputs.length > 0 && (
                    <div>
                      <p className="text-[11px] font-medium text-claude-textSecondary mb-1">输入参数</p>
                      <div className="space-y-1">
                        {detailSkill.od_metadata.inputs.map((input, i) => (
                          <div key={i} className="flex items-center gap-2 text-[11px]">
                            <span className="text-claude-text font-medium">{input.name}</span>
                            <span className="text-claude-textSecondary">({input.type})</span>
                            {input.required && <span className="text-red-400">*必填</span>}
                            {input.description && <span className="text-claude-textSecondary/60">- {input.description}</span>}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}

              <button
                onClick={() => {
                  handleGenerate(detailSkill);
                  setDetailSkill(null);
                }}
                className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-claude-text text-claude-bg rounded-xl text-[14px] font-medium hover:opacity-90 transition-opacity"
              >
                <Sparkles size={14} />
                使用此技能创建设计
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default SkillGallery;
