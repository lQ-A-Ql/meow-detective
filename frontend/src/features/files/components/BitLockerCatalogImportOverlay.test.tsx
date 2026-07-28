import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { BitLockerCatalogImportOverlay } from './BitLockerCatalogImportOverlay';

describe('BitLockerCatalogImportOverlay', () => {
  it('shows an indeterminate catalog phase without fabricating a percentage', () => {
    render(
      <BitLockerCatalogImportOverlay
        lifecycle={{ phase: 'catalog', startedAt: Date.now() }}
      />,
    );

    expect(screen.getByTestId('bitlocker-catalog-import-overlay')).toHaveAttribute('role', 'status');
    expect(screen.getByText('正在构建 BitLocker 文件目录')).toBeInTheDocument();
    expect(screen.getByText(/正在遍历解锁后的文件系统/)).toBeInTheDocument();
    expect(screen.getByRole('progressbar')).toHaveAttribute('data-state', 'indeterminate');
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();
  });

  it('reports the refresh phase after the real catalog import returns', () => {
    render(
      <BitLockerCatalogImportOverlay
        lifecycle={{ phase: 'refreshing', startedAt: Date.now() }}
      />,
    );

    expect(screen.getByText('目录遍历完成，正在刷新文件树和当前目录。')).toBeInTheDocument();
  });
});
