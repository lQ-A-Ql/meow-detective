import { AlertTriangle, CheckCircle, Shield } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/app/components/ui/card';
import { Progress } from '@/app/components/ui/progress';
import { RuleFamilyList } from '@/features/rule-packs/components/RuleFamilyList';

export function RulePackCoveragePanel({
  covered,
  uncovered,
  coveragePercent,
}: {
  covered: string[];
  uncovered: string[];
  coveragePercent: number;
}) {
  return (
    <Card className="border-forensics-border bg-forensics-surface">
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center gap-2 text-[14px]">
          <Shield size={16} />
          覆盖范围摘要
        </CardTitle>
        <CardDescription className="text-[11px]">所有已加载规则包的合并覆盖范围</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="mb-4">
          <div className="mb-1 flex items-center justify-between text-[11px]">
            <span className="text-forensics-muted">整体覆盖率</span>
            <span className="font-mono font-light text-forensics-text">{coveragePercent}%</span>
          </div>
          <Progress value={coveragePercent} className="h-1.5 rounded-none bg-forensics-panel-strong" />
        </div>

        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <RuleFamilyList
            title={`已覆盖 (${covered.length})`}
            families={covered}
            icon={<CheckCircle size={12} />}
            titleClassName="text-forensics-success-text"
            badgeClassName="bg-forensics-success-bg text-forensics-success-text hover:bg-forensics-success-bg"
          />
          <RuleFamilyList
            title={`未覆盖 (${uncovered.length})`}
            families={uncovered}
            icon={<AlertTriangle size={12} />}
            titleClassName="text-forensics-error-text"
            badgeClassName="border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text"
            outlined
          />
        </div>
      </CardContent>
    </Card>
  );
}
