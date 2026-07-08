import { useTranslation } from 'react-i18next';
import { Play, RefreshCw } from 'lucide-react';
import { Button } from '@/app/components/ui/button';

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
        <Button
          type="button"
          variant="forensicsPrimary"
          size="xs"
          onClick={executeQuery}
          disabled={loading || !code.trim()}
          className="bg-[#2ea44f] font-medium hover:bg-[#2c974b]"
        >
          <Play size={12} />
          {t('gql.header.run')}
        </Button>
      </div>
    </div>
  );
}
