import inspirations from './inspirations.json';

export interface PromptSuggestion {
  artifact_id: string;
  category: string;
  name: string;
  description: string;
  starting_prompt: string;
}

// These examples ask for apps, games or interactive artifacts to be built.
// The weekly chronicle is a writing request and remains on Home.
const writingExampleId = 'acb45ef5-69f5-436f-b4d9-b16c681257bb';
export const codeSuggestions: PromptSuggestion[] = inspirations.items.filter(item => item.artifact_id !== writingExampleId);

const homePrompts = [
  ['learn', 'Explain a concept', 'Understand a topic with clear examples', 'Help me understand a new topic. Ask what I want to learn, then explain it step by step with concrete examples.'],
  ['learn', 'Study plan', 'Build a study routine around your goals', 'Help me make a realistic study plan. Ask about my subject, current level, deadline and available study time.'],
  ['learn', 'Practice a language', 'Have a conversation and improve together', 'Be my language tutor. Ask which language and level I want to practice, then have a conversation and explain corrections.'],
  ['life-hacks', 'Polish an email', 'Turn your thoughts into a clear message', 'Help me write an email. Ask who it is for, what I want to say and the tone, then draft a clear message.'],
  ['life-hacks', 'Summarize meeting notes', 'Find decisions, action items and next steps', 'Help me summarize my meeting notes. Ask me to paste the notes, then extract decisions, action items, owners and deadlines without inventing missing details.'],
  ['games', 'Trivia challenge', 'Test your knowledge one question at a time', 'Play a trivia game with me in this chat. Ask for my favorite topic and difficulty, then ask one question at a time and explain the answers.'],
  ['games', 'Word game', 'Try a playful word challenge', 'Play a word guessing game with me in this chat. Explain the rules briefly and start with a clue.'],
  ['games', 'Logic puzzle', 'Work through a puzzle with optional hints', 'Give me a logic puzzle to solve in this chat. Ask my preferred difficulty and provide hints only when I ask.'],
  ['creative', 'Story ideas', 'Find a fresh starting point for a story', 'Help me brainstorm story ideas. Ask about the genre and mood, then suggest three different premises.'],
  ['creative', 'Writing feedback', 'Improve clarity, flow and expression', 'Give me constructive feedback on my writing. Ask me to paste a passage and describe the audience, then suggest edits with explanations.'],
  ['creative', 'Brainstorm names', 'Explore names for an idea or project', 'Help me brainstorm names. Ask what I am naming, who it is for and the feeling I want it to convey.'],
  ['touch-grass', 'Plan a day outside', 'Find an outing that fits your time and energy', 'Help me plan a day outdoors. Ask about my location, available time, budget and interests before suggesting an itinerary.'],
  ['touch-grass', 'Take a mindful break', 'Pause and reset with a short exercise', 'Guide me through a short mindful break with a simple breathing and observation exercise.'],
  ['touch-grass', 'Start a small garden', 'Choose plants and make a practical care plan', 'Help me start a small garden. Ask about my climate, available space, sunlight and gardening experience.'],
];

export const homeSuggestions: PromptSuggestion[] = [
  ...homePrompts.map(([category, name, description, starting_prompt], index) => ({ artifact_id: `home-${index}`, category, name, description, starting_prompt })),
  ...inspirations.items.filter(item => item.artifact_id === writingExampleId),
];
