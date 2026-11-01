import '@/i18n';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MdRaidEvidencePanel } from '@/features/infrastructure/components/MdRaidEvidencePanel';

const translate = ((key: string) => key) as never;

describe('MdRaidEvidencePanel', () => {
  it('renders the empty state', () => {
    render(<MdRaidEvidencePanel arrays={[]} t={translate} onImport={() => undefined} />);
    expect(screen.getByText('infrastructure.workspace.mdraid.empty')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'infrastructure.workspace.mdraid.import' })).toBeInTheDocument();
  });

  it('opens the RAID import entry', () => {
    const onImport = vi.fn();
    render(<MdRaidEvidencePanel arrays={[]} t={translate} onImport={onImport} />);

    screen.getByRole('button', { name: 'infrastructure.workspace.mdraid.import' }).click();

    expect(onImport).toHaveBeenCalledOnce();
  });

  it('renders array members and capacity mismatch', () => {
    render(
      <MdRaidEvidencePanel
        t={translate}
        arrays={[{
          arrayUuid: 'array-1',
          name: 'mac:0',
          level: 0,
          layout: 1,
          chunkSectors: 1024,
          raidDisks: 2,
          minDataSizeSectors: 10,
          capacityMismatch: true,
          filesystem: 'candidate',
          members: [
            { devNumber: 0, vgName: 'root', lvName: 'data', dataOffsetSectors: 67584, dataSizeSectors: 12, superOffsetSectors: 8 },
            { devNumber: 1, vgName: 'volum', lvName: 'root', dataOffsetSectors: 67584, dataSizeSectors: 10, superOffsetSectors: 8 },
          ],
        }]}
      />,
    );
    expect(screen.getByText('root/data')).toBeInTheDocument();
    expect(screen.getByText('volum/root')).toBeInTheDocument();
    expect(screen.getByText('infrastructure.workspace.mdraid.capacityMismatch')).toBeInTheDocument();
    expect(screen.getByText('infrastructure.workspace.mdraid.filesystemStatus.candidate')).toBeInTheDocument();
  });
});
