import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';
import SidebarNavigation from './SidebarNavigation';
import { useUIStore } from '../stores/useUIStore';
import en from '../locales/en.json';

function Location() {
  return <output data-testid="location">{useLocation().pathname}</output>;
}

describe('extension navigation', () => {
  it.each([false, true])('opens customization without directory or standalone MCP (collapsed=%s)', collapsed => {
    useUIStore.getState().setLanguage('en');
    render(<MemoryRouter>
      <SidebarNavigation collapsed={collapsed} onNewChat={vi.fn()} onSearch={vi.fn()} onPanel={vi.fn()} onNavigate={vi.fn()} onSwarm={vi.fn()} />
      <Location />
    </MemoryRouter>);
    fireEvent.click(screen.getByRole('button', { name: en.sidebar.extensions }));
    expect(screen.queryByRole('button', { name: en.sidebar.directory })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'MCP' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: en.sidebar.customize }));
    expect(screen.getByTestId('location')).toHaveTextContent('/customize');
    if (collapsed) expect(screen.getByRole('button', { name: en.sidebar.extensions })).toHaveAttribute('aria-expanded', 'false');
  });
});
