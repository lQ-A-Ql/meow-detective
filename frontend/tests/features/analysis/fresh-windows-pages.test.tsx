import { readFileSync } from 'node:fs';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { RegistryExtractionPanel } from '@/features/analysis/components/panels/RegistryExtractionPanel';
import { FileListPanel } from '@/features/files/components/FileListPanel';
import { VirtualFileTree } from '@/features/files/components/VirtualFileTree';
import type { FileChildrenPage, FileRowsPage, FileTreeNode, RegistryStructuredSummary } from '@/types/models';

interface FreshWindowsPageReport {
  roots: FileTreeNode[];
  children: FileChildrenPage;
  visibleChildren: FileChildrenPage;
  rows: FileRowsPage;
  registry: RegistryStructuredSummary;
}

const reportPath = process.env.FORENSICS_FRESH_WINDOWS_PAGE_REPORT;

function report(): FreshWindowsPageReport {
  if (!reportPath) throw new Error('Set FORENSICS_FRESH_WINDOWS_PAGE_REPORT');
  return JSON.parse(readFileSync(reportPath, 'utf8')) as FreshWindowsPageReport;
}

function ignoreAction() {}

describe.runIf(Boolean(reportPath))('fresh Windows source pages', () => {
  it('renders imported hardware in the registry hardware tab', () => {
    const data = report();
    expect(data.registry.hardwareDevices.length).toBeGreaterThan(0);
    render(<RegistryExtractionPanel structured={data.registry} />);
    fireEvent.mouseDown(screen.getByRole('tab', { name: '系统硬件' }), { button: 0 });
    const device = data.registry.hardwareDevices[0];
    expect(screen.getAllByText(device.name).length).toBeGreaterThan(0);
    expect(screen.queryByText('正在读取检材硬件信息…')).not.toBeInTheDocument();
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
  });

  it('renders the imported partition tree and its orphan group', () => {
    const data = report();
    expect(data.children.totalCount).toBeLessThan(500);
    expect(data.children.children.some((node) => node.name === '$DeletedOrphans')).toBe(true);
    const nodes = [...data.roots, ...data.children.children];
    render(<VirtualFileTree nodes={nodes} onNodeClick={ignoreAction} overscan={nodes.length} />);
    expect(screen.getAllByText('$DeletedOrphans').length).toBeGreaterThan(0);
    expect(screen.queryByText('$dpx$.tmp')).not.toBeInTheDocument();
  });

  it('renders imported root rows with unique identities and applies hidden visibility', () => {
    const data = report();
    expect(new Set(data.rows.rows.map((row) => row.id)).size).toBe(data.rows.rows.length);
    expect(data.visibleChildren.children.every((node) => !node.hidden && !node.system)).toBe(true);
    render(
      <FileListPanel
        sortedRows={data.rows.rows}
        selectedFileId={undefined}
        fileSortKey="name"
        fileSortDirection="asc"
        handleSort={ignoreAction}
        onActivateEntry={ignoreAction}
        rowsPage={data.rows}
        canGoToPreviousRows={false}
        canGoToNextRows={false}
        goToPreviousRows={ignoreAction}
        goToNextRows={ignoreAction}
        onExtractFile={ignoreAction}
      />,
    );
    expect(screen.getAllByText('$DeletedOrphans').length).toBeGreaterThan(0);
    expect(screen.queryByText('$dpx$.tmp')).not.toBeInTheDocument();
  });
});
