import React, { useState } from 'react';
import { ChevronLeft, ChevronRight, Lightbulb, BookOpen, Gamepad2, Palette, Trees } from 'lucide-react';
import { codeSuggestions, homeSuggestions } from '../data/promptSuggestions';

interface PromptSuggestionsPanelProps {
  onSelectPrompt: (prompt: string) => void;
  mode?: 'home' | 'code';
}

const CATEGORY_ICONS: Record<string, React.ReactNode> = {
  learn: <BookOpen size={16} />,
  'life-hacks': <Lightbulb size={16} />,
  games: <Gamepad2 size={16} />,
  creative: <Palette size={16} />,
  'touch-grass': <Trees size={16} />,
};

const CATEGORY_LABELS: Record<string, string> = {
  learn: 'Learn something',
  'life-hacks': 'Life hacks',
  games: 'Play a game',
  creative: 'Be creative',
  'touch-grass': 'Touch grass',
};

const CATEGORY_ORDER = ['learn', 'life-hacks', 'games', 'creative', 'touch-grass'];

const PromptSuggestionsPanel: React.FC<PromptSuggestionsPanelProps> = ({ onSelectPrompt, mode = 'home' }) => {
  const [activeCategory, setActiveCategory] = useState<string>('life-hacks');
  const [page, setPage] = useState(0);

  const items = mode === 'code' ? codeSuggestions : homeSuggestions;
  const categoryItems = items.filter(item => item.category === activeCategory);

  const pageCount = Math.ceil(categoryItems.length / 3);
  // Fill the last page from the beginning so every page contains three cards.
  const visibleItems = Array.from({ length: Math.min(3, categoryItems.length) },
    (_, index) => categoryItems[(page * 3 + index) % categoryItems.length]);

  return (
    <section aria-label="Prompt suggestions" className="relative z-0 w-full min-w-0 max-w-full mt-8">
      {/* Category tabs */}
      <div className="flex flex-wrap items-center justify-center gap-2 mb-4">
        {CATEGORY_ORDER.map(cat => {
          const isActive = cat === activeCategory;
          return (
            <button
              key={cat}
              aria-pressed={isActive}
              onClick={() => { setActiveCategory(cat); setPage(0); }}
              className={`flex items-center gap-2 px-4 py-2 rounded-full text-[14px] font-medium transition-all duration-200 ${
                isActive
                  ? 'bg-claude-text text-claude-bg shadow-sm'
                  : 'bg-transparent text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover'
              }`}
            >
              {CATEGORY_ICONS[cat]}
              <span>{CATEGORY_LABELS[cat]}</span>
            </button>
          );
        })}
      </div>

      {/* Suggestion cards */}
      <div className="relative">
        {/* Left scroll arrow */}
        <button
          aria-label="Previous suggestions"
          onClick={() => setPage(current => (current - 1 + pageCount) % pageCount)}
          disabled={pageCount <= 1}
          className="absolute left-0 top-1/2 -translate-y-1/2 z-10 w-8 h-8 flex items-center justify-center bg-claude-bg/80 backdrop-blur-sm rounded-full shadow-md text-claude-textSecondary hover:text-claude-text hover:bg-claude-bg transition-colors"
        >
          <ChevronLeft size={18} />
        </button>

        {/* Cards container */}
        <div
          key={activeCategory}
          aria-live="polite"
          className="grid w-full min-w-0 grid-cols-3 gap-2 sm:gap-4 px-9 sm:px-10 py-2"
        >
          {visibleItems.map(item => (
            <button
              key={item.artifact_id}
              onClick={() => onSelectPrompt(item.starting_prompt)}
              className="min-w-0 w-full bg-claude-input border border-claude-border rounded-xl p-2 sm:p-4 text-left hover:border-[#CCC] dark:hover:border-[#5a5a58] hover:shadow-md transition-all duration-200 group"
            >
              <div className="flex flex-col sm:flex-row items-start gap-2 sm:gap-3 mb-2">
                <div className="w-8 h-8 rounded-lg bg-claude-bg flex items-center justify-center text-claude-textSecondary group-hover:text-claude-text transition-colors flex-shrink-0">
                  {CATEGORY_ICONS[item.category]}
                </div>
                <div className="min-w-0 max-w-full">
                  <h3 className="text-[14px] font-semibold text-claude-text line-clamp-2 break-words">{item.name}</h3>
                </div>
              </div>
              <p className="text-[13px] text-claude-textSecondary leading-snug line-clamp-2">
                {item.description}
              </p>
            </button>
          ))}
        </div>

        {/* Right scroll arrow */}
        <button
          aria-label="Next suggestions"
          onClick={() => setPage(current => (current + 1) % pageCount)}
          disabled={pageCount <= 1}
          className="absolute right-0 top-1/2 -translate-y-1/2 z-10 w-8 h-8 flex items-center justify-center bg-claude-bg/80 backdrop-blur-sm rounded-full shadow-md text-claude-textSecondary hover:text-claude-text hover:bg-claude-bg transition-colors"
        >
          <ChevronRight size={18} />
        </button>
      </div>
    </section>
  );
};

export default PromptSuggestionsPanel;
