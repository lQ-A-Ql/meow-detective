import { useTranslation } from 'react-i18next';
import { Play, RefreshCw } from 'lucide-react';

interface GqlQueryHeaderProps {
  loading: boolean;
  executeQuery: () => void;
  code: string;
}

export function GqlQueryHeader({ loading, executeQuery, code }: GqlQueryHeaderProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center justify-between px-3 py-2 border-b border-forensics-border bg-forensics-highlight">
      <span className="text-[11px] font-semibold text-forensics-gql-muted uppercase tracking-wider">
        {t('gql.header.title')}
      </span>
      <div className="flex items-center gap-2">
        {loading && (
          <span className="text-[11px] text-forensics-gql-muted flex items-center gap-1">
            <RefreshCw size={12} className="animate-spin" />
            {t('gql.header.running')}
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
          {t('gql.header.run')}
        </button>
      </div>
    </div>
  );
}
