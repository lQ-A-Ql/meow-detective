import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { PlatformCoverageSection } from './PlatformCoverageSection';

describe('PlatformCoverageSection', () => {
  it('renders only supported platform coverage', () => {
    render(
      <PlatformCoverageSection
        data={{
          windowsArtifactFamilies: 4,
          linuxArtifactFamilies: 3,
          crossPlatformArtifactFamilies: 2,
          unknownArtifactFamilies: 0,
          totalFamilies: 9,
          windowsFamilies: ['Registry'],
          linuxFamilies: ['Journal'],
          crossPlatformFamilies: ['BrowserHistory'],
          unknownFamilies: [],
        }}
      />,
    );

    expect(screen.getAllByText('Windows')).not.toHaveLength(0);
    expect(screen.getAllByText('Linux')).not.toHaveLength(0);
    expect(screen.getAllByText('跨平台')).not.toHaveLength(0);
    expect(screen.queryByText('macOS')).toBeNull();
  });
});
