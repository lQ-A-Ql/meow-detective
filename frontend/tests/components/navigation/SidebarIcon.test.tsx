import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SidebarIcon } from '@/components/navigation/SidebarIcon';

const names = ['home', 'files', 'emulation', 'analysis', 'infrastructure', 'v2', 'v3', 'search', 'timeline', 'artifacts', 'reports', 'settings', 'jobs'] as const;

describe('SidebarIcon', () => {
  it('applies a quoted mask url so a Vite ?import query cannot invalidate the declaration', () => {
    render(<SidebarIcon name="home" className="size-[21px]" />);
    const icon = document.querySelector('span[aria-hidden="true"]') as HTMLElement;
    expect(icon).not.toBeNull();

    const style = icon.getAttribute('style') ?? '';
    // Unquoted url(...?import) is rejected by the CSS parser, so the mask declaration was
    // dropped and the span painted a solid block instead of the icon.
    expect(style).toContain('url("');
    const mask = icon.style.maskImage || icon.style.webkitMaskImage;
    expect(mask.startsWith('url("')).toBe(true);
    expect(mask.endsWith('")')).toBe(true);
    expect(icon.style.maskSize).toBe('contain');
    expect(icon.style.maskRepeat).toBe('no-repeat');
  });

  it('renders every sidebar icon name with its own shared asset mask', () => {
    render(
      <div>
        {names.map((name) => (
          <SidebarIcon key={name} name={name} />
        ))}
      </div>,
    );
    const icons = Array.from(document.querySelectorAll('span[aria-hidden="true"]')) as HTMLElement[];
    expect(icons).toHaveLength(names.length);
    // The URL form differs between dev (?import query), test (data: URL) and build (hashed
    // path), so assert the contract that matters: one quoted url() per icon, all distinct.
    const masks = icons.map((icon) => icon.style.maskImage || icon.style.webkitMaskImage);
    masks.forEach((mask) => {
      expect(mask.startsWith('url("')).toBe(true);
      expect(mask.endsWith('")')).toBe(true);
    });
    expect(new Set(masks).size).toBe(names.length);
  });
});
