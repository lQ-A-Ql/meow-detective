import { useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useCurrentCase } from '@/features/case/hooks';
import { getLinuxEvidenceEvents, getLinuxEvidenceSetSummary, listLinuxEvidenceSets } from '@/lib/api/analysis';
import { useInfrastructureWorkspace } from '../hooks/useInfrastructureWorkspace';
import { capabilitiesForSummary, provenanceState } from './cluster-status-map';

export type ClusterWorkspaceSection = 'overview' | 'members' | 'topology' | 'findings' | 'provenance';

export function useLinuxClusterWorkspaceModel() {
  const currentCase = useCurrentCase();
  const infrastructure = useInfrastructureWorkspace();
  const evidenceSets = useQuery({
    queryKey: ['linux-evidence-sets', currentCase.data?.id ?? null],
    queryFn: listLinuxEvidenceSets,
    enabled: Boolean(currentCase.data),
  });
  const [selectedSetId, setSelectedSetId] = useState<string>();
  const [section, setSection] = useState<ClusterWorkspaceSection>('overview');
  const [selectedMemberIndex, setSelectedMemberIndex] = useState<number>();
  useEffect(() => {
    if (!selectedSetId && evidenceSets.data?.[0]) setSelectedSetId(evidenceSets.data[0].importSetId);
  }, [evidenceSets.data, selectedSetId]);
  const summary = useQuery({
    queryKey: ['linux-evidence-set-summary', currentCase.data?.id ?? null, selectedSetId ?? null],
    queryFn: () => getLinuxEvidenceSetSummary(selectedSetId as string),
    enabled: Boolean(selectedSetId),
  });
  const events = useQuery({
    queryKey: ['linux-evidence-set-events', currentCase.data?.id ?? null, selectedSetId ?? null],
    queryFn: () => getLinuxEvidenceEvents(selectedSetId as string, 0, 100),
    enabled: Boolean(selectedSetId),
  });
  const selectedMember = useMemo(
    () => summary.data?.members.find((member) => member.memberIndex === selectedMemberIndex),
    [selectedMemberIndex, summary.data],
  );
  const capabilities = useMemo(
    () => (summary.data ? capabilitiesForSummary(summary.data) : []),
    [summary.data],
  );
  const provenance = summary.data ? provenanceState(summary.data) : 'partial';
  const fallback = !evidenceSets.isLoading && (evidenceSets.data?.length ?? 0) === 0;
  return {
    currentCase,
    infrastructure,
    evidenceSets,
    selectedSetId,
    setSelectedSetId,
    section,
    setSection,
    summary,
    events,
    selectedMember,
    selectedMemberIndex,
    setSelectedMemberIndex,
    capabilities,
    provenance,
    fallback,
  };
}

export type LinuxClusterWorkspaceModel = ReturnType<typeof useLinuxClusterWorkspaceModel>;
