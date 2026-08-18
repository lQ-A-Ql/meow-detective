import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SvgRenderer } from './svg-renderer';

describe('SvgRenderer', () => {
  it('renders SVG as an isolated image data URL', () => {
    const svg = '<svg xmlns="http://www.w3.org/2000/svg"><script>bad()</script></svg>';
    const dataBase64 = btoa(svg);
    const { container } = render(<SvgRenderer dataBase64={dataBase64} alt="evidence" />);

    expect(screen.getByRole('img', { name: 'evidence' })).toHaveAttribute(
      'src',
      `data:image/svg+xml;base64,${dataBase64}`,
    );
    expect(container.querySelector('svg')).toBeNull();
    expect(container.querySelector('script')).toBeNull();
  });
});
