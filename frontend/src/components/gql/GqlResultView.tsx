import { useState, useCallback, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronRight, Copy, Check } from 'lucide-react';
import type { GraphNode, GraphEdge, GraphQueryResult } from '@/types/models';

export interface GqlResultViewProps {
  result: GraphQueryResult;
}

function CopyButton({ id, copiedId, copyId }: { id: string; copiedId: string | null; copyId: (id: string) => void }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        copyId(id);
      }}
      className="ml-auto p-0.5 rounded hover:bg-forensics-hover transition-colors shrink-0"
      title={t('gql.result.copyTitle')}
    >
      {copiedId === id ? (
        <Check size={10} className="text-green-600" />
      ) : (
        <Copy size={10} className="text-forensics-muted-light" />
      )}
    </button>
  );
}

function ExpandRow({
  expanded,
  onToggle,
  children,
}: {
  expanded: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onToggle}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onToggle();
        }
      }}
      className="w-full flex items-center gap-1 px-2 py-1 rounded hover:bg-forensics-highlight text-left cursor-pointer"
    >
      {expanded ? (
        <ChevronDown size={12} className="text-forensics-muted-light shrink-0" />
      ) : (
        <ChevronRight size={12} className="text-forensics-muted-light shrink-0" />
      )}
      {children}
    </div>
  );
}

export function GqlResultView({ result }: GqlResultViewProps) {
  const { t } = useTranslation();
  const [expandedNode, setExpandedNode] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (copyTimerRef.current) {
        clearTimeout(copyTimerRef.current);
      }
    };
  }, []);

  const copyId = useCallback(async (id: string) => {
    try {
      await navigator.clipboard.writeText(id);
      setCopiedId(id);
      if (copyTimerRef.current) {
        clearTimeout(copyTimerRef.current);
      }
      copyTimerRef.current = setTimeout(() => setCopiedId(null), 2000);
    } catch {
      // clipboard API not available
    }
  }, []);

  const toggleNodeExpand = useCallback((id: string) => {
    setExpandedNode((prev) => (prev === id ? null : id));
  }, []);

  return (
    <div className="border-t border-forensics-border max-h-[300px] overflow-y-auto">
      {/* Result stats */}
      <div className="flex items-center gap-4 px-3 py-2 bg-forensics-highlight border-b border-forensics-border text-[11px]">
        <span className="text-forensics-muted">
          <span className="font-semibold text-forensics-text">{result.nodes?.length ?? 0}</span> {t('gql.result.nodes')}
        </span>
        <span className="text-forensics-muted">
          <span className="font-semibold text-forensics-text">{result.edges?.length ?? 0}</span> {t('gql.result.edges')}
        </span>
        <span className="text-forensics-muted">
          <span className="font-semibold text-forensics-text">{result.nodeCount}</span> {t('gql.result.totalMatched')}
        </span>
      </div>

      {/* Node list */}
      {result.nodes && result.nodes.length > 0 && (
        <div className="px-2 py-1">
          <div className="text-[10px] font-semibold text-forensics-muted uppercase tracking-wider px-1 py-1">
            {t('gql.result.nodeList')}
          </div>
          {result.nodes.map((node: GraphNode, i: number) => {
            const key = `${node.id}-${i}`;
            const isExpanded = expandedNode === key;
            return (
              <div key={key} className="mb-1">
                <ExpandRow
                  expanded={isExpanded}
                  onToggle={() => toggleNodeExpand(key)}
                >
                  <span className="text-[11px] px-1 py-0.5 rounded bg-forensics-gql-type/10 text-forensics-gql-type font-mono shrink-0">
                    {node.nodeType}
                  </span>
                  <span className="text-[12px] font-medium text-forensics-text truncate">
                    {node.label}
                  </span>
                  <CopyButton id={node.id} copiedId={copiedId} copyId={copyId} />
                </ExpandRow>
                {isExpanded && (
                  <div className="ml-5 px-2 py-1 text-[11px] text-forensics-muted font-mono space-y-0.5">
                    <div>{t('gql.result.fields.id')}: {node.id}</div>
                    <div>{t('gql.result.fields.type')}: {node.nodeType}</div>
                    <div>{t('gql.result.fields.label')}: {node.label}</div>
                    {node.summary && <div>{t('gql.result.fields.summary')}: {node.summary}</div>}
                    {node.tags && node.tags.length > 0 && (
                      <div>{t('gql.result.fields.tags')}: {node.tags.join(', ')}</div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Edge list */}
      {result.edges && result.edges.length > 0 && (
        <div className="px-2 py-1 border-t border-forensics-border-light">
          <div className="text-[10px] font-semibold text-forensics-muted uppercase tracking-wider px-1 py-1">
            {t('gql.result.edgeList')}
          </div>
          {result.edges.map((edge: GraphEdge, i: number) => {
            const key = `${edge.id}-${i}`;
            const isExpanded = expandedNode === key;
            return (
              <div key={key} className="mb-1">
                <ExpandRow
                  expanded={isExpanded}
                  onToggle={() => toggleNodeExpand(key)}
                >
                  <span className="text-[11px] px-1 py-0.5 rounded bg-forensics-gql-variable/10 text-forensics-gql-variable font-mono shrink-0">
                    {edge.edgeType}
                  </span>
                  <span className="text-[11px] text-forensics-muted font-mono truncate">
                    {edge.sourceId} → {edge.targetId}
                  </span>
                  {edge.confidence != null && (
                    <span className="text-[10px] text-forensics-muted ml-auto shrink-0">
                      {Math.round(edge.confidence * 100)}%
                    </span>
                  )}
                </ExpandRow>
                {isExpanded && (
                  <div className="ml-5 px-2 py-1 text-[11px] text-forensics-muted font-mono space-y-0.5">
                    <div>{t('gql.result.fields.id')}: {edge.id}</div>
                    <div>{t('gql.result.fields.type')}: {edge.edgeType}</div>
                    <div>{t('gql.result.fields.source')}: {edge.sourceId}</div>
                    <div>{t('gql.result.fields.target')}: {edge.targetId}</div>
                    {edge.confidence != null && (
                      <div>{t('gql.result.fields.confidence')}: {edge.confidence}</div>
                    )}
                    {edge.provenance != null && (
                      <div>{t('gql.result.fields.provenance')}: {edge.provenance}</div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Empty result */}
      {(!result.nodes || result.nodes.length === 0) &&
        (!result.edges || result.edges.length === 0) && (
          <div className="px-3 py-4 text-center text-[12px] text-forensics-muted">
            {t('gql.result.empty')}
          </div>
        )}
    </div>
  );
}
