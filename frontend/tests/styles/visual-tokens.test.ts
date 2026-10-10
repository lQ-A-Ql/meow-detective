import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

/**
 * The v3.1 sheet is a contract, not a taste: these assertions pin the palette, the radius
 * ladder, the single gold usage and the contrast of every token that carries text.
 */
const themeCss = fs.readFileSync(path.resolve(process.cwd(), 'src/styles/theme.css'), 'utf8');

function token(name: string): string {
  const match = themeCss.match(new RegExp('--' + name + ':\\s*([^;]+);'));
  if (!match) throw new Error('missing token ' + name);
  return match[1].trim();
}

function luminance(hex: string): number {
  const value = hex.replace('#', '');
  const channels = [0, 2, 4].map((offset) => parseInt(value.slice(offset, offset + 2), 16) / 255);
  const [r, g, b] = channels.map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(foreground: string, background: string): number {
  const a = luminance(foreground);
  const b = luminance(background);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

describe('v3.1 visual tokens', () => {
  it('keeps the white + red palette', () => {
    expect(token('forensics-paper')).toBe('#ffffff');
    expect(token('forensics-base')).toBe('#f2f1ee');
    expect(token('forensics-red')).toBe('#ff0000');
    expect(token('forensics-red-ink')).toBe('#c8102e');
    expect(token('forensics-red-deep')).toBe('#7a1622');
    expect(token('forensics-red-tint')).toBe('#fff2f2');
    expect(token('forensics-rail-active')).toBe('rgba(255, 0, 0, 0.8)');
  });

  it('keeps exactly one gold usage: the 40% section divider', () => {
    expect(token('forensics-divider')).toBe('rgba(168, 128, 42, 0.4)');
    const goldValues = themeCss
      .split('\n')
      .filter((line) => !line.includes('--forensics-divider'))
      .filter((line) => /#[0-9a-f]{6}/i.test(line))
      .filter((line) => {
        const hex = line.match(/#([0-9a-f]{6})/i)?.[1] ?? '';
        const r = parseInt(hex.slice(0, 2), 16);
        const g = parseInt(hex.slice(2, 4), 16);
        const b = parseInt(hex.slice(4, 6), 16);
        // warm gold: red and green clearly above blue, green dominant
        return r > 120 && r < 220 && g > 90 && g < 190 && b < 110 && g >= b + 60 && r >= b + 80;
      });
    expect(goldValues).toEqual([]);
  });

  it('keeps the radius ladder at 20 / 6 / 4', () => {
    expect(token('forensics-r-shell')).toBe('20px');
    expect(token('forensics-r-ctl')).toBe('6px');
    expect(token('forensics-r-badge')).toBe('4px');
  });

  it('keeps every text token readable on paper', () => {
    const paper = token('forensics-paper');
    expect(contrast(token('forensics-ink'), paper)).toBeGreaterThan(7);
    expect(contrast(token('forensics-ink-3'), paper)).toBeGreaterThan(4.5);
    expect(contrast(token('forensics-red-ink'), paper)).toBeGreaterThan(4.5);
    expect(contrast(token('forensics-red-deep'), paper)).toBeGreaterThan(7);
    // pure red is an indicator colour: it only has to clear the 3:1 graphics threshold
    expect(contrast(token('forensics-red'), paper)).toBeGreaterThan(3);
  });
});
