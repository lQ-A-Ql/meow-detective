import { useState } from 'react';
import {
  AlertTriangle,
  CheckCircle,
  FileUp,
  Loader2,
  PackageOpen,
  RefreshCw,
  Shield,
  XCircle,
} from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/app/components/ui/card';
import { Badge } from '@/app/components/ui/badge';
import { Input } from '@/app/components/ui/input';
import { RulePackCoveragePanel } from '@/features/rule-packs/components/RulePackCoveragePanel';
import type { RulePackManagerModel } from '@/features/rule-packs/use-rule-pack-manager-model';
import type { RulePackSummary } from '@/types/models';

const STATUS_CONFIG: Record<RulePackSummary['status'], { label: string; icon: typeof Shield; tone: string }> = {
  loaded: { label: '已加载', icon: CheckCircle, tone: 'bg-forensics-success-bg border-forensics-success-border text-forensics-success-text' },
  validating: { label: '校验中', icon: Loader2, tone: 'bg-forensics-warning-bg border-forensics-warning-border text-forensics-warning-text' },
  error: { label: '错误', icon: XCircle, tone: 'bg-forensics-error-bg border-forensics-error-border text-forensics-error-text' },
};

function formatTimestamp(iso: string) {
  try {
    const d = new Date(iso);
    return d.toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
}

export function RulePackManager({ model }: { model: RulePackManagerModel }) {
  const [selectedPackId, setSelectedPackId] = useState<string | null>(null);
  const [loadPath, setLoadPath] = useState('');

  const handleLoad = () => {
    const path = loadPath.trim();
    if (path) {
      model.load(path, () => setLoadPath(''));
    }
  };

  const handleValidate = (packId: string) => {
    model.validate(packId);
  };

  if (model.loading) {
    return (
      <div className="flex h-64 items-center justify-center text-forensics-muted-lighter">
        <Loader2 size={24} className="mr-2 opacity-70" />
        正在加载规则包...
      </div>
    );
  }

  if (model.error) {
    return (
      <div className="flex h-64 flex-col items-center justify-center gap-3">
        <XCircle size={32} className="text-forensics-error-text" />
        <div className="text-[13px] text-forensics-muted">无法加载规则包列表</div>
        <Button
          type="button"
          variant="outline"
          onClick={model.retry}
          className="h-8 rounded-none border-forensics-border bg-forensics-surface px-4 text-[12px] hover:bg-forensics-panel-strong"
        >
          重试
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="shrink-0 border-b border-forensics-border bg-forensics-panel p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <div className="font-serif text-xl tracking-tight text-forensics-text">规则包管理</div>
            <div className="mt-1 font-mono text-[11px] text-forensics-muted">
              加载、校验并查看规则包覆盖范围
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={model.retry}
              disabled={model.loading}
              className="h-8 rounded-none border-forensics-border bg-forensics-surface px-3 text-[12px] hover:bg-forensics-panel-strong"
            >
              <RefreshCw size={14} className={model.loading ? 'opacity-70' : ''} />
              刷新
            </Button>
          </div>
        </div>

        {/* Summary strip */}
        <div className="mt-4 grid grid-cols-3 gap-4">
          <div className="rounded-none border border-forensics-border bg-forensics-surface px-4 py-3 text-center">
            <div className="text-2xl font-light text-forensics-text">{model.packs.length}</div>
            <div className="mt-1 text-[11px] text-forensics-muted">规则包</div>
          </div>
          <div className="rounded-none border border-forensics-border bg-forensics-surface px-4 py-3 text-center">
            <div className="text-2xl font-light text-forensics-text">{model.totalRules}</div>
            <div className="mt-1 text-[11px] text-forensics-muted">规则总数</div>
          </div>
          <div className="rounded-none border border-forensics-border bg-forensics-surface px-4 py-3 text-center">
            <div className="text-2xl font-light text-forensics-text">{model.coveragePercent}%</div>
            <div className="mt-1 text-[11px] text-forensics-muted">覆盖率</div>
          </div>
        </div>
      </div>

      {/* Load section */}
      <Card className="border-forensics-border bg-forensics-surface">
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-[14px]">
            <FileUp size={16} />
            加载新规则包
          </CardTitle>
          <CardDescription className="text-[11px]">
            选择规则包文件（.yaml / .yml / .json），Tauri 文件对话框将在后端接入后启用。
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center gap-3">
            <Input
              type="text"
              value={loadPath}
              onChange={(e) => setLoadPath(e.target.value)}
              placeholder="C:/rules/my-rule-pack.yaml"
              variant="path"
              inputSize="compact"
              className="flex-1"
            />
            <Button
              type="button"
              onClick={handleLoad}
              disabled={!loadPath.trim() || model.loadPending}
              className="h-8 rounded-none border border-forensics-text bg-forensics-text px-4 text-[12px] text-white hover:bg-forensics-text-secondary"
            >
              {model.loadPending ? (
                <Loader2 size={14} className="opacity-70" />
              ) : (
                <PackageOpen size={14} />
              )}
              加载
            </Button>
          </div>
          {model.loadError && (
            <div className="mt-2 rounded-none border border-forensics-error-border bg-forensics-error-bg px-3 py-1.5 text-[11px] text-forensics-error-text">
              {model.loadError}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Pack list */}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {model.packs.map((pack) => {
          const config = STATUS_CONFIG[pack.status] ?? STATUS_CONFIG.loaded;
          const StatusIcon = config.icon;
          const isSelected = selectedPackId === pack.id;

          return (
            <Card
              key={pack.id}
              className={`cursor-pointer border-forensics-border transition-colors hover:border-forensics-border-strong ${
                isSelected ? 'border-forensics-text bg-forensics-panel' : 'bg-forensics-surface'
              }`}
              onClick={() => setSelectedPackId(isSelected ? null : pack.id)}
            >
              <CardHeader className="pb-2">
                <div className="flex items-start justify-between gap-3">
                  <div className="flex-1 min-w-0">
                    <CardTitle className="flex items-center gap-2 text-[13px]">
                      <PackageOpen size={15} />
                      <span className="truncate">{pack.name}</span>
                      <span className="font-mono text-[11px] text-forensics-muted-light">v{pack.version}</span>
                    </CardTitle>
                    {pack.author && (
                      <CardDescription className="mt-0.5 text-[10px]">
                        {pack.author}
                      </CardDescription>
                    )}
                  </div>
                  <Badge
                    variant="outline"
                    className={`shrink-0 text-[10px] ${config.tone}`}
                  >
                    <StatusIcon
                      size={12}
                      className={`mr-1 ${pack.status === 'validating' ? 'opacity-70' : ''}`}
                    />
                    {config.label}
                  </Badge>
                </div>
                {pack.description && (
                  <p className="mt-1 text-[11px] leading-5 text-forensics-muted">{pack.description}</p>
                )}
              </CardHeader>
              <CardContent>
                <div className="flex flex-wrap items-center gap-4 text-[11px] text-forensics-muted">
                  <span>
                    规则数: <span className="font-mono font-light text-forensics-text">{pack.ruleCount}</span>
                  </span>
                  <span className="text-forensics-muted-lighter">|</span>
                  <span>加载时间: {formatTimestamp(pack.loadedAt)}</span>
                </div>

                {/* Covered families */}
                {pack.coveredFamilies.length > 0 && (
                  <div className="mt-2 flex flex-wrap items-center gap-1.5">
                    {pack.coveredFamilies.map((family) => (
                      <Badge
                        key={family}
                        variant="secondary"
                        className="bg-forensics-panel-strong text-[10px] text-forensics-text-tertiary"
                      >
                        {family}
                      </Badge>
                    ))}
                  </div>
                )}

                {/* Warnings */}
                {pack.warnings.length > 0 && (
                  <div className="mt-2 rounded-none border border-forensics-warning-border bg-forensics-warning-bg px-3 py-1.5 text-[10px] text-forensics-warning-text">
                    {pack.warnings.slice(0, 2).map((w, i) => (
                      <div key={i} className="flex items-start gap-1">
                        <AlertTriangle size={10} className="mt-0.5 shrink-0" />
                        <span>{w}</span>
                      </div>
                    ))}
                    {pack.warnings.length > 2 && (
                      <div className="mt-0.5 text-forensics-warning-text">
                        ...以及 {pack.warnings.length - 2} 条更多
                      </div>
                    )}
                  </div>
                )}

                {/* Errors */}
                {pack.errors.length > 0 && (
                  <div className="mt-2 rounded-none border border-forensics-error-border bg-forensics-error-bg px-3 py-1.5 text-[10px] text-forensics-error-text">
                    {pack.errors.slice(0, 3).map((e, i) => (
                      <div key={i} className="flex items-start gap-1">
                        <XCircle size={10} className="mt-0.5 shrink-0" />
                        <span>{e}</span>
                      </div>
                    ))}
                  </div>
                )}

                {/* Validate button */}
                <div className="mt-3 flex items-center gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleValidate(pack.id);
                    }}
                    disabled={model.validatePending}
                    className="h-7 rounded-none border-forensics-border bg-forensics-surface px-3 text-[11px] hover:bg-forensics-panel-strong"
                  >
                    {model.validatePending && model.validatingPackId === pack.id ? (
                      <Loader2 size={12} className="opacity-70" />
                    ) : (
                      <Shield size={12} />
                    )}
                    校验
                  </Button>
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>

      {model.packs.length === 0 && (
        <div className="flex h-40 flex-col items-center justify-center rounded-none border border-dashed border-forensics-border-strong bg-forensics-surface">
          <PackageOpen size={32} className="text-forensics-muted-lighter" />
          <div className="mt-3 text-[13px] text-forensics-muted">暂未加载任何规则包</div>
          <div className="mt-1 text-[11px] text-forensics-muted-lighter">使用上方输入框加载您的第一个规则包</div>
        </div>
      )}

      {/* Coverage panel */}
      <RulePackCoveragePanel
        covered={model.coveredFamilies}
        uncovered={model.uncoveredFamilies}
        coveragePercent={model.coveragePercent}
      />
    </div>
  );
}
