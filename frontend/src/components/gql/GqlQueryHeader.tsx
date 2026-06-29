import { Play, RefreshCw } from 'lucide-react';

interface GqlQueryHeaderProps {
  loading: boolean;
  executeQuery: () => void;
  code: string;
}

export function GqlQueryHeader({ loading, executeQuery, code }: GqlQueryHeaderProps) {
  return (
    <div className="flex items-center justify-between px-3 py-2 border-b border-[#e0e0e0] bg-[#f6f8fa]">
      <span className="text-[11px] font-semibold text-[#586069] uppercase tracking-wider">
        GQL Query
      </span>
      <div className="flex items-center gap-2">
        {loading && (
          <span className="text-[11px] text-[#586069] flex items-center gap-1">
            <RefreshCw size={12} className="animate-spin" />
            Running...
          </span>
        )}
        <button
          onClick={executeQuery}
          disabled={loading || !code.trim()}
          className="flex items-center gap-1 px-3 py-1 rounded text-[11px] font-medium
                     bg-[#2ea44f] text-white hover:bg-[#2c974b] transition-colors
                     disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <Play size={12} />
          Run
        </button>
      </div>
    </div>
  );
}
