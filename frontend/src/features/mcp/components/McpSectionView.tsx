import { Button } from '@/app/components/ui/button';
import { ActionIcon } from '@/components/controls/ActionIcon';
import type { TFunction } from 'i18next';
import type { ReactNode } from 'react';

interface Props {
  t: TFunction;
  expanded: boolean;
  onToggle: () => void;
  hostToolList: ReactNode;
}

export function McpSectionView({ t, expanded, onToggle, hostToolList }: Props) {
  return (
    <section className="max-w-4xl space-y-5">
      <Button type="button" variant="forensicsGhost" shape="row" aria-expanded={expanded} onClick={onToggle}
        className="h-7 gap-2 px-0 text-[14px] text-forensics-ink">
        <ActionIcon name={expanded ? 'chevron-down' : 'chevron-right'} className="size-3" />
        {t('settings.sections.mcp.title')}
      </Button>
      {expanded ? hostToolList : null}
    </section>
  );
}
