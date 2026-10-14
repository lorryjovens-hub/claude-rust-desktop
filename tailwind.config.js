/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        claude: {
          bg: 'var(--bg-claude-main)',
          sidebar: 'var(--bg-claude-sidebar)',
          border: 'var(--border-claude)',
          text: 'var(--text-claude-main)',
          textSecondary: 'var(--text-claude-secondary)',
          accent: 'var(--bg-claude-accent)',
          hover: 'var(--bg-claude-hover)',
          btnHover: 'var(--bg-claude-btn-hover)',
          input: 'var(--bg-claude-input)',
          avatar: 'var(--bg-claude-avatar)',
          avatarText: 'var(--text-claude-avatar)',
          tooltipBg: 'var(--bg-tooltip)',
          tooltipText: 'var(--text-tooltip)',
          navActive: 'var(--text-nav-active)',
          navDisabled: 'var(--text-nav-disabled)',
        }
      },
      fontFamily: {
        sans: ['Anthropic Sans', 'Figtree Variable', 'Figtree', 'system-ui', 'Microsoft YaHei', 'sans-serif'],
        serif: ['Anthropic Serif', 'Source Serif 4 Variable', 'Source Serif 4', 'serif'],
        system: ['system-ui', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'Roboto', 'Oxygen', 'Ubuntu', 'Cantarell', '"Open Sans"', '"Helvetica Neue"', 'sans-serif'],
      }
    }
  },
  plugins: [],
}
