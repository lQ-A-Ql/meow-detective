import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useCurrentCase } from '@/features/case/hooks';
import { getTextPreview } from '@/lib/api/files';
import type { LinuxSystemConfig } from '@/types/models';

export interface LinuxServicesPanelProps {
  rows: LinuxSystemConfig[];
  total: number;
  hasMore?: boolean;
  loadingMore?: boolean;
  loadMoreFailed?: boolean;
  onLoadMore?: () => void;
  onRetryLoadMore?: () => unknown;
  loadContextKey?: string;
  loadStateKey?: string | number;
}

export function useLinuxServicesPanelModel(props: LinuxServicesPanelProps) {
  const currentCase = useCurrentCase();
  const contextKey = JSON.stringify([currentCase.data?.id ?? null, props.loadContextKey ?? null]);
  const [selection, setSelection] = useState<{ contextKey: string; row: LinuxSystemConfig }>();
  const services = props.rows.filter((row) => row.configKind === 'systemdServiceUnit'
    && (row.name ?? row.line).endsWith('.service'));
  const selected = selection?.contextKey === contextKey
    ? services.find((row) => row.artifactId === selection.row.artifactId)
    : undefined;
  const preview = useQuery({
    queryKey: ['analysis', 'linux-service-preview', contextKey, selected?.fileId ?? null],
    queryFn: () => {
      if (!selected?.fileId) throw new Error('A service file must be selected for preview');
      return getTextPreview(selected.fileId, 1024 * 1024);
    },
    enabled: Boolean(currentCase.data?.id && selected?.fileId),
    retry: false,
  });

  return {
    ...props,
    services,
    configured: services.length,
    enabled: services.filter((row) => row.enabled === true).length,
    canLoadMore: Boolean(props.hasMore && props.rows.length < props.total),
    selected,
    selectService: (row: LinuxSystemConfig) => setSelection({ contextKey, row }),
    closePreview: () => setSelection(undefined),
    previewLoading: preview.isFetching,
    previewFailed: preview.isError,
    previewContent: preview.data?.content,
  };
}

export type LinuxServicesPanelModel = ReturnType<typeof useLinuxServicesPanelModel>;
