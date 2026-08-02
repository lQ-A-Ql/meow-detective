import type { ReactNode } from 'react';
import { Badge } from '@/app/components/ui/badge';

export function RuleFamilyList({
  title,
  families,
  icon,
  titleClassName,
  badgeClassName,
  outlined = false,
}: {
  title: string;
  families: string[];
  icon: ReactNode;
  titleClassName: string;
  badgeClassName: string;
  outlined?: boolean;
}) {
  return (
    <div>
      <div className={`mb-2 flex items-center gap-1.5 text-[11px] font-light ${titleClassName}`}>
        {icon}
        {title}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {families.map((family) => (
          <Badge
            key={family}
            variant={outlined ? 'outline' : 'default'}
            className={`text-[10px] ${badgeClassName}`}
          >
            {family}
          </Badge>
        ))}
        {families.length === 0 ? (
          <span className="text-[10px] text-forensics-muted-lighter">无</span>
        ) : null}
      </div>
    </div>
  );
}
