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
import { Progress } from '@/app/components/ui/progress';
import { useLoadedRulePacks, useLoadRulePack, useValidateRulePack } from '@/features/rule-packs/hooks';
import type { RulePackSummary } from '@/types/models';

const STATUS_CONFIG: Record<RulePackSummary['status'], { label: string; icon: typeof Shield; tone: string }> = {
  loaded: { label: '已加载', icon: CheckCircle, tone: 'bg-green-50 border-green-200 text-green-700' },
  validating: { label: '校验中', icon: Loader2, tone: 'bg-amber-50 border-amber-200 text-amber-700' },
  error: { label: '错误', icon: XCircle, tone: 'bg-red-50 border-red-200 text-red-700' },
};

const ALL_KNOWN_FAMILIES = [
  'Prefetch',
  'LNK',
  'JumpList',
  'Registry',
  'EventLog',
  'BrowserHistory',
  'UserAssist',
  'RecycleBin',
  'Thumbcache',
  'SRU',
  'Amcache',
  'BAM',
  'MFT',
  'FileSystem',
  'NetworkArtifacts',
];

function formatTimestamp(iso: string) {
  try {
    const d = new Date(iso);
    return d.toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
}

export function RulePackManager() {
  const { data: packs = [], isLoading, isError, refetch } = useLoadedRulePacks();
  const loadMutation = useLoadRulePack();
  const validateMutation = useValidateRulePack();
  const [selectedPackId, setSelectedPackId] = useState<string | null>(null);
  const [loadPath, setLoadPath] = useState('');

  const selectedPack = packs.find((p) => p.id === selectedPackId) ?? null;

  const totalRules = packs.reduce((sum, p) => sum + p.ruleCount, 0);
  const totalCovered = new Set<string>();
  packs.forEach((p) => p.coveredFamilies.forEach((f) => totalCovered.add(f)));
  const coveragePercent =
    ALL_KNOWN_FAMILIES.length > 0
      ? Math.round((totalCovered.size / ALL_KNOWN_FAMILIES.length) * 100)
      : 0;

  const handleLoad = () => {
    const path = loadPath.trim();
    if (path) {
      loadMutation.mutate(path, {
        onSuccess: () => setLoadPath(''),
      });
    }
  };

  const handleValidate = (packId: string) => {
    validateMutation.mutate(packId);
  };

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center text-[#999]">
        <Loader2 size={24} className="mr-2 animate-spin" />
        正在加载规则包...
      </div>
    );
  }

  if (isError) {
    return (
      <div className="flex h-64 flex-col items-center justify-center gap-3">
        <XCircle size={32} className="text-red-400" />
        <div className="text-[13px] text-[#666]">无法加载规则包列表</div>
        <Button
          type="button"
          variant="outline"
          onClick={() => refetch()}
          className="h-8 rounded border-[#ddd] bg-white px-4 text-[12px] hover:bg-[#f5f5f5]"
        >
          重试
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="shrink-0 border-b border-[#e0e0e0] bg-[#fafafa] p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <div className="font-serif text-xl tracking-tight text-[#111]">规则包管理</div>
            <div className="mt-1 font-mono text-[11px] text-[#666]">
              加载、校验并查看规则包覆盖范围
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => refetch()}
              disabled={isLoading}
              className="h-8 rounded border-[#ddd] bg-white px-3 text-[12px] hover:bg-[#f5f5f5]"
            >
              <RefreshCw size={14} className={isLoading ? 'animate-spin' : ''} />
              刷新
            </Button>
          </div>
        </div>

        {/* Summary strip */}
        <div className="mt-4 grid grid-cols-3 gap-4">
          <div className="rounded border border-[#e0e0e0] bg-white px-4 py-3 text-center">
            <div className="text-2xl font-bold text-[#111]">{packs.length}</div>
            <div className="mt-1 text-[11px] text-[#666]">规则包</div>
          </div>
          <div className="rounded border border-[#e0e0e0] bg-white px-4 py-3 text-center">
            <div className="text-2xl font-bold text-[#111]">{totalRules}</div>
            <div className="mt-1 text-[11px] text-[#666]">规则总数</div>
          </div>
          <div className="rounded border border-[#e0e0e0] bg-white px-4 py-3 text-center">
            <div className="text-2xl font-bold text-[#111]">{coveragePercent}%</div>
            <div className="mt-1 text-[11px] text-[#666]">覆盖率</div>
          </div>
        </div>
      </div>

      {/* Load section */}
      <Card className="border-[#e0e0e0] bg-[#fcfcfc]">
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
            <input
              type="text"
              value={loadPath}
              onChange={(e) => setLoadPath(e.target.value)}
              placeholder="C:/rules/my-rule-pack.yaml"
              className="flex-1 rounded border border-[#e0e0e0] bg-white px-3 py-1.5 font-mono text-[12px] text-[#333] outline-none placeholder:text-[#bbb] focus:border-[#999]"
            />
            <Button
              type="button"
              onClick={handleLoad}
              disabled={!loadPath.trim() || loadMutation.isPending}
              className="h-8 rounded border border-[#111] bg-[#111] px-4 text-[12px] text-white hover:bg-[#333]"
            >
              {loadMutation.isPending ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <PackageOpen size={14} />
              )}
              加载
            </Button>
          </div>
          {loadMutation.isError && (
            <div className="mt-2 rounded border border-red-200 bg-red-50 px-3 py-1.5 text-[11px] text-red-700">
              {(loadMutation.error as Error)?.message ?? '加载失败'}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Pack list */}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {packs.map((pack) => {
          const config = STATUS_CONFIG[pack.status] ?? STATUS_CONFIG.loaded;
          const StatusIcon = config.icon;
          const isSelected = selectedPackId === pack.id;

          return (
            <Card
              key={pack.id}
              className={`cursor-pointer border-[#e0e0e0] transition-colors hover:border-[#999] ${
                isSelected ? 'border-[#111] bg-[#fafafa]' : 'bg-white'
              }`}
              onClick={() => setSelectedPackId(isSelected ? null : pack.id)}
            >
              <CardHeader className="pb-2">
                <div className="flex items-start justify-between gap-3">
                  <div className="flex-1 min-w-0">
                    <CardTitle className="flex items-center gap-2 text-[13px]">
                      <PackageOpen size={15} />
                      <span className="truncate">{pack.name}</span>
                      <span className="font-mono text-[11px] text-[#888]">v{pack.version}</span>
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
                      className={`mr-1 ${pack.status === 'validating' ? 'animate-spin' : ''}`}
                    />
                    {config.label}
                  </Badge>
                </div>
                {pack.description && (
                  <p className="mt-1 text-[11px] leading-5 text-[#666]">{pack.description}</p>
                )}
              </CardHeader>
              <CardContent>
                <div className="flex flex-wrap items-center gap-4 text-[11px] text-[#666]">
                  <span>
                    规则数: <span className="font-mono font-semibold text-[#111]">{pack.ruleCount}</span>
                  </span>
                  <span className="text-[#bbb]">|</span>
                  <span>加载时间: {formatTimestamp(pack.loadedAt)}</span>
                </div>

                {/* Covered families */}
                {pack.coveredFamilies.length > 0 && (
                  <div className="mt-2 flex flex-wrap items-center gap-1.5">
                    {pack.coveredFamilies.map((family) => (
                      <Badge
                        key={family}
                        variant="secondary"
                        className="bg-[#f0f0f0] text-[10px] text-[#555]"
                      >
                        {family}
                      </Badge>
                    ))}
                  </div>
                )}

                {/* Warnings */}
                {pack.warnings.length > 0 && (
                  <div className="mt-2 rounded border border-amber-200 bg-amber-50 px-3 py-1.5 text-[10px] text-amber-800">
                    {pack.warnings.slice(0, 2).map((w, i) => (
                      <div key={i} className="flex items-start gap-1">
                        <AlertTriangle size={10} className="mt-0.5 shrink-0" />
                        <span>{w}</span>
                      </div>
                    ))}
                    {pack.warnings.length > 2 && (
                      <div className="mt-0.5 text-amber-600">
                        ...以及 {pack.warnings.length - 2} 条更多
                      </div>
                    )}
                  </div>
                )}

                {/* Errors */}
                {pack.errors.length > 0 && (
                  <div className="mt-2 rounded border border-red-200 bg-red-50 px-3 py-1.5 text-[10px] text-red-700">
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
                    disabled={validateMutation.isPending}
                    className="h-7 rounded border-[#ddd] bg-white px-3 text-[11px] hover:bg-[#f5f5f5]"
                  >
                    {validateMutation.isPending && validateMutation.variables === pack.id ? (
                      <Loader2 size={12} className="animate-spin" />
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

      {packs.length === 0 && (
        <div className="flex h-40 flex-col items-center justify-center rounded border border-dashed border-[#d8d8d8] bg-[#fcfcfc]">
          <PackageOpen size={32} className="text-[#ccc]" />
          <div className="mt-3 text-[13px] text-[#777]">暂未加载任何规则包</div>
          <div className="mt-1 text-[11px] text-[#999]">使用上方输入框加载您的第一个规则包</div>
        </div>
      )}

      {/* Coverage panel */}
      <CoveragePanel
        covered={Array.from(totalCovered)}
        allFamilies={ALL_KNOWN_FAMILIES}
        coveragePercent={coveragePercent}
      />
    </div>
  );
}

function CoveragePanel({
  covered,
  allFamilies,
  coveragePercent,
}: {
  covered: string[];
  allFamilies: string[];
  coveragePercent: number;
}) {
  const coveredSet = new Set(covered);
  const uncovered = allFamilies.filter((f) => !coveredSet.has(f));

  return (
    <Card className="border-[#e0e0e0] bg-[#fcfcfc]">
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center gap-2 text-[14px]">
          <Shield size={16} />
          覆盖范围摘要
        </CardTitle>
        <CardDescription className="text-[11px]">
          所有已加载规则包的合并覆盖范围
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="mb-4">
          <div className="mb-1 flex items-center justify-between text-[11px]">
            <span className="text-[#666]">整体覆盖率</span>
            <span className="font-mono font-semibold text-[#111]">{coveragePercent}%</span>
          </div>
          <Progress value={coveragePercent} className="h-1.5 rounded-none bg-[#eee]" />
        </div>

        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div>
            <div className="mb-2 flex items-center gap-1.5 text-[11px] font-semibold text-green-700">
              <CheckCircle size={12} />
              已覆盖 ({covered.length})
            </div>
            <div className="flex flex-wrap gap-1.5">
              {covered.map((f) => (
                <Badge key={f} className="bg-green-50 text-[10px] text-green-700 hover:bg-green-100">
                  {f}
                </Badge>
              ))}
              {covered.length === 0 && (
                <span className="text-[10px] text-[#999]">无</span>
              )}
            </div>
          </div>

          <div>
            <div className="mb-2 flex items-center gap-1.5 text-[11px] font-semibold text-red-700">
              <AlertTriangle size={12} />
              未覆盖 ({uncovered.length})
            </div>
            <div className="flex flex-wrap gap-1.5">
              {uncovered.map((f) => (
                <Badge
                  key={f}
                  variant="outline"
                  className="border-amber-200 bg-amber-50 text-[10px] text-amber-800"
                >
                  {f}
                </Badge>
              ))}
              {uncovered.length === 0 && (
                <span className="text-[10px] text-[#999]">无</span>
              )}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
