import { useState, useCallback, useRef, useEffect } from 'react';
import { ChevronDown, ChevronRight, Copy, Check } from 'lucide-react';
import type { GraphNode, GraphEdge, GraphQueryResult } from '@/types/models';

export interface GqlResultViewProps {
  result: GraphQueryResult;
}

function CopyButton({ id, copiedId, copyId }: { id: string; copiedId: string | null; copyId: (id: string) => void }) {
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        copyId(id);
      }}
      className="ml-auto p-0.5 rounded hover:bg-[#e0e0e0] transition-colors shrink-0"
      title="Copy ID"
    >
      {copiedId === id ? (
        <Check size={10} className="text-[#2ea44f]" />
      ) : (
        <Copy size={10} className="text-[#586069]" />
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
      className="w-full flex items-center gap-1 px-2 py-1 rounded hover:bg-[#f6f8fa] text-left cursor-pointer"
    >
      {expanded ? (
        <ChevronDown size={12} className="text-[#586069] shrink-0" />
      ) : (
        <ChevronRight size={12} className="text-[#586069] shrink-0" />
      )}
      {children}
    </div>
  );
}

export function GqlResultView({ result }: GqlResultViewProps) {
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
    <div className="border-t border-[#e0e0e0] max-h-[300px] overflow-y-auto">
      {/* Result stats */}
      <div className="flex items-center gap-4 px-3 py-2 bg-[#f6f8fa] border-b border-[#e0e0e0] text-[11px]">
        <span className="text-[#586069]">
          <span className="font-semibold text-[#24292e]">{result.nodes?.length ?? 0}</span> nodes
        </span>
        <span className="text-[#586069]">
          <span className="font-semibold text-[#24292e]">{result.edges?.length ?? 0}</span> edges
        </span>
        <span className="text-[#586069]">
          <span className="font-semibold text-[#24292e]">{result.nodeCount}</span> total matched
        </span>
      </div>

      {/* Node list */}
      {result.nodes && result.nodes.length > 0 && (
        <div className="px-2 py-1">
          <div className="text-[10px] font-semibold text-[#586069] uppercase tracking-wider px-1 py-1">
            Nodes
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
                  <span className="text-[11px] px-1 py-0.5 rounded bg-[#6f42c1]/10 text-[#6f42c1] font-mono shrink-0">
                    {node.nodeType}
                  </span>
                  <span className="text-[12px] font-medium text-[#24292e] truncate">
                    {node.label}
                  </span>
                  <CopyButton id={node.id} copiedId={copiedId} copyId={copyId} />
                </ExpandRow>
                {isExpanded && (
                  <div className="ml-5 px-2 py-1 text-[11px] text-[#586069] font-mono space-y-0.5">
                    <div>id: {node.id}</div>
                    <div>type: {node.nodeType}</div>
                    <div>label: {node.label}</div>
                    {node.summary && <div>summary: {node.summary}</div>}
                    {node.tags && node.tags.length > 0 && (
                      <div>tags: {node.tags.join(', ')}</div>
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
        <div className="px-2 py-1 border-t border-[#f0f0f0]">
          <div className="text-[10px] font-semibold text-[#586069] uppercase tracking-wider px-1 py-1">
            Edges
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
                  <span className="text-[11px] px-1 py-0.5 rounded bg-[#005cc5]/10 text-[#005cc5] font-mono shrink-0">
                    {edge.edgeType}
                  </span>
                  <span className="text-[11px] text-[#586069] font-mono truncate">
                    {edge.sourceId} → {edge.targetId}
                  </span>
                  {edge.confidence != null && (
                    <span className="text-[10px] text-[#586069] ml-auto shrink-0">
                      {Math.round(edge.confidence * 100)}%
                    </span>
                  )}
                </ExpandRow>
                {isExpanded && (
                  <div className="ml-5 px-2 py-1 text-[11px] text-[#586069] font-mono space-y-0.5">
                    <div>id: {edge.id}</div>
                    <div>type: {edge.edgeType}</div>
                    <div>source: {edge.sourceId}</div>
                    <div>target: {edge.targetId}</div>
                    {edge.confidence != null && (
                      <div>confidence: {edge.confidence}</div>
                    )}
                    {edge.provenance != null && (
                      <div>provenance: {edge.provenance}</div>
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
          <div className="px-3 py-4 text-center text-[12px] text-[#586069]">
            Query returned no results.
          </div>
        )}
    </div>
  );
}
