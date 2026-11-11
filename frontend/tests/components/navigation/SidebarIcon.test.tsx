import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SidebarIcon } from '@/components/navigation/SidebarIcon';

const names = [
  'home',
  'files',
  'emulation',
  'analysis',
  'infrastructure',
  'v2',
  'v3',
  'search',
  'timeline',
  'artifacts',
  'reports',
  'settings',
  'jobs',
] as const;

describe('SidebarIcon', () => {
  it('renders a Font Awesome svg that inherits the rail color', () => {
    render(<SidebarIcon name="home" className="size-[21px]" />);
    const icon = document.querySelector('svg');

    expect(icon).not.toBeNull();
    expect(icon).toHaveClass('svg-inline--fa', 'size-[21px]');
    expect(icon).toHaveAttribute('data-icon', 'house');
  });

  it('renders a distinct glyph for every sidebar destination', () => {
    render(
      <div>
        {names.map((name) => (
          <SidebarIcon key={name} name={name} />
        ))}
      </div>,
    );
    const icons = Array.from(document.querySelectorAll('svg[data-icon]'));
    expect(icons).toHaveLength(names.length);
    expect(new Set(icons.map((icon) => icon.getAttribute('data-icon'))).size).toBe(names.length);
  });
});
