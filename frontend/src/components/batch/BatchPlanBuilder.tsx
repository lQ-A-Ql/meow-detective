import { useState, useCallback } from 'react';
import { ChevronLeft, ChevronRight, Play, Check, Loader2 } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from '@/app/components/ui/card';
import { Checkbox } from '@/app/components/ui/checkbox';
import { Input } from '@/app/components/ui/input';
import { Label } from '@/app/components/ui/label';
import { Badge } from '@/app/components/ui/badge';
import type { BatchPhaseName, BatchPlanInput, DataSourceSummary, BatchResourceLimits } from '@/types/models';

const ALL_PHASES: { key: BatchPhaseName; label: string; description: string }[] = [
  { key: 'Mount', label: 'Mount', description: 'Attach and mount evidence data sources' },
  { key: 'Catalog', label: 'Catalog', description: 'Enumerate and catalog file system metadata' },
  { key: 'ExtractArtifacts', label: 'Extract Artifacts', description: 'Run artifact parsers (Prefetch, LNK, Registry, etc.)' },
  { key: 'Index', label: 'Index', description: 'Build full-text search index' },
  { key: 'Correlate', label: 'Correlate', description: 'Run correlation rules across artifacts and files' },
  { key: 'Export', label: 'Export', description: 'Generate and export reports' },
];

const DEFAULT_RESOURCE_LIMITS: BatchResourceLimits = {
  maxMemoryMb: 4096,
  maxThreads: 4,
};

interface BatchPlanBuilderProps {
  dataSources: DataSourceSummary[];
  onStart: (plan: BatchPlanInput) => void;
  onCancel: () => void;
  isStarting?: boolean;
}

type WizardStep = 1 | 2 | 3 | 4;

export function BatchPlanBuilder({ dataSources, onStart, onCancel, isStarting }: BatchPlanBuilderProps) {
  const [step, setStep] = useState<WizardStep>(1);
  const [selectedDataSourceIds, setSelectedDataSourceIds] = useState<Set<string>>(new Set());
  const [selectedPhases, setSelectedPhases] = useState<Set<BatchPhaseName>>(
    new Set<BatchPhaseName>(['Mount', 'Catalog', 'ExtractArtifacts', 'Index', 'Correlate', 'Export']),
  );
  const [resourceLimits, setResourceLimits] = useState<BatchResourceLimits>(DEFAULT_RESOURCE_LIMITS);
  const [planName, setPlanName] = useState('');

  const toggleDataSource = useCallback((id: string) => {
    setSelectedDataSourceIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const togglePhase = useCallback((phase: BatchPhaseName) => {
    setSelectedPhases((prev) => {
      const next = new Set(prev);
      if (next.has(phase)) {
        next.delete(phase);
      } else {
        next.add(phase);
      }
      return next;
    });
  }, []);

  const toggleAllDataSources = useCallback(() => {
    setSelectedDataSourceIds((prev) => {
      if (prev.size === dataSources.length) {
        return new Set();
      }
      return new Set(dataSources.map((ds) => ds.id));
    });
  }, [dataSources]);

  const toggleAllPhases = useCallback(() => {
    setSelectedPhases((prev) => {
      if (prev.size === ALL_PHASES.length) {
        return new Set();
      }
      return new Set(ALL_PHASES.map((p) => p.key));
    });
  }, []);

  const canGoNext = (): boolean => {
    switch (step) {
      case 1:
        return selectedDataSourceIds.size > 0 && planName.trim().length > 0;
      case 2:
        return selectedPhases.size > 0;
      case 3:
        return (resourceLimits.maxMemoryMb ?? 0) >= 256 && (resourceLimits.maxThreads ?? 0) >= 1;
      default:
        return true;
    }
  };

  const handleStart = () => {
    const plan: BatchPlanInput = {
      name: planName.trim(),
      dataSourceIds: Array.from(selectedDataSourceIds),
      phases: Array.from(selectedPhases),
      resourceLimits,
    };
    onStart(plan);
  };

  const stepTitles: Record<WizardStep, string> = {
    1: 'Select Data Sources',
    2: 'Select Phases',
    3: 'Resource Limits',
    4: 'Review & Start',
  };

  return (
    <Card className="w-full max-w-2xl">
      <CardHeader>
        <div className="flex items-center justify-between">
          <div>
            <CardTitle className="text-[14px] font-semibold">New Batch Job</CardTitle>
            <CardDescription className="text-[12px]">
              Step {step}/4: {stepTitles[step]}
            </CardDescription>
          </div>
          <div className="flex gap-1">
            {([1, 2, 3, 4] as WizardStep[]).map((s) => (
              <div
                key={s}
                className={`h-2 w-8 rounded-full transition-colors ${
                  s <= step ? 'bg-primary' : 'bg-muted'
                }`}
              />
            ))}
          </div>
        </div>
      </CardHeader>

      <CardContent className="space-y-4">
        {/* Step 1: Select Data Sources */}
        {step === 1 && (
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label className="text-[12px] font-medium">Plan Name</Label>
              <Input
                value={planName}
                onChange={(e) => setPlanName(e.target.value)}
                placeholder="e.g. Full Case Analysis"
                className="text-[12px]"
              />
            </div>
            <div className="flex items-center justify-between">
              <Label className="text-[12px] font-medium">Data Sources</Label>
              <button
                type="button"
                onClick={toggleAllDataSources}
                className="text-[11px] text-primary hover:underline"
              >
                {selectedDataSourceIds.size === dataSources.length ? 'Deselect All' : 'Select All'}
              </button>
            </div>
            <div className="max-h-60 space-y-1 overflow-auto rounded border p-2">
              {dataSources.length === 0 ? (
                <p className="py-4 text-center text-[12px] text-muted-foreground">
                  No data sources available. Import evidence first.
                </p>
              ) : (
                dataSources.map((ds) => (
                  <label
                    key={ds.id}
                    className="flex cursor-pointer items-start gap-2 rounded px-2 py-1.5 hover:bg-muted/50"
                  >
                    <Checkbox
                      checked={selectedDataSourceIds.has(ds.id)}
                      onCheckedChange={() => toggleDataSource(ds.id)}
                    />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[12px] font-medium">{ds.name}</div>
                      <div className="truncate text-[11px] text-muted-foreground">
                        {ds.kind} &middot; {ds.sourcePath}
                      </div>
                    </div>
                    <Badge variant="secondary" className="shrink-0 text-[10px]">
                      {ds.fileCount?.toLocaleString() ?? '?'} files
                    </Badge>
                  </label>
                ))
              )}
            </div>
          </div>
        )}

        {/* Step 2: Select Phases */}
        {step === 2 && (
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label className="text-[12px] font-medium">Pipeline Phases</Label>
              <button
                type="button"
                onClick={toggleAllPhases}
                className="text-[11px] text-primary hover:underline"
              >
                {selectedPhases.size === ALL_PHASES.length ? 'Deselect All' : 'Select All'}
              </button>
            </div>
            <div className="space-y-2">
              {ALL_PHASES.map((phase) => (
                <label
                  key={phase.key}
                  className="flex cursor-pointer items-start gap-3 rounded border p-3 hover:bg-muted/30 transition-colors"
                >
                  <Checkbox
                    checked={selectedPhases.has(phase.key)}
                    onCheckedChange={() => togglePhase(phase.key)}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="text-[12px] font-medium">{phase.label}</div>
                    <div className="text-[11px] text-muted-foreground">{phase.description}</div>
                  </div>
                </label>
              ))}
            </div>
          </div>
        )}

        {/* Step 3: Resource Limits */}
        {step === 3 && (
          <div className="space-y-4">
            <div className="space-y-1.5">
              <Label className="text-[12px] font-medium" htmlFor="rl-memory">
                Memory Limit (MB)
              </Label>
              <Input
                id="rl-memory"
                type="number"
                min={256}
                step={256}
                value={resourceLimits.maxMemoryMb ?? ''}
                onChange={(e) =>
                  setResourceLimits((prev) => ({
                    ...prev,
                    maxMemoryMb: Math.max(256, parseInt(e.target.value, 10) || 256),
                  }))
                }
                className="text-[12px]"
              />
              <p className="text-[11px] text-muted-foreground">
                Recommended: 2048-8192 MB depending on data size.
              </p>
            </div>
            <div className="space-y-1.5">
              <Label className="text-[12px] font-medium" htmlFor="rl-threads">
                Thread Count
              </Label>
              <Input
                id="rl-threads"
                type="number"
                min={1}
                max={64}
                step={1}
                value={resourceLimits.maxThreads ?? ''}
                onChange={(e) =>
                  setResourceLimits((prev) => ({
                    ...prev,
                    maxThreads: Math.min(64, Math.max(1, parseInt(e.target.value, 10) || 1)),
                  }))
                }
                className="text-[12px]"
              />
              <p className="text-[11px] text-muted-foreground">
                Recommended: 4-16 threads. Affects parsing and indexing parallelism.
              </p>
            </div>
          </div>
        )}

        {/* Step 4: Review & Start */}
        {step === 4 && (
          <div className="space-y-4">
            <div className="rounded border p-3 space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-[11px] text-muted-foreground">Plan Name</span>
                <span className="text-[12px] font-medium">{planName || '(unnamed)'}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-[11px] text-muted-foreground">Data Sources</span>
                <Badge variant="secondary" className="text-[10px]">
                  {selectedDataSourceIds.size} selected
                </Badge>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-[11px] text-muted-foreground">Phases</span>
                <div className="flex flex-wrap gap-1">
                  {Array.from(selectedPhases).map((p) => (
                    <Badge key={p} variant="outline" className="text-[10px]">
                      {p}
                    </Badge>
                  ))}
                </div>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-[11px] text-muted-foreground">Memory</span>
                <span className="text-[12px] font-mono">{resourceLimits.maxMemoryMb} MB</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-[11px] text-muted-foreground">Threads</span>
                <span className="text-[12px] font-mono">{resourceLimits.maxThreads}</span>
              </div>
            </div>
            <div className="rounded bg-muted/50 p-3 text-[11px] text-muted-foreground">
              <Check size={14} className="inline mr-1" />
              Phases will run sequentially. You can pause or cancel during execution.
            </div>
          </div>
        )}
      </CardContent>

      <CardFooter className="flex items-center justify-between">
        <div>
          {step > 1 && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => setStep((s) => (s - 1) as WizardStep)}
            >
              <ChevronLeft size={14} />
              Back
            </Button>
          )}
        </div>
        <div className="flex gap-2">
          <Button variant="ghost" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          {step < 4 ? (
            <Button size="sm" disabled={!canGoNext()} onClick={() => setStep((s) => (s + 1) as WizardStep)}>
              Next
              <ChevronRight size={14} />
            </Button>
          ) : (
            <Button size="sm" onClick={handleStart} disabled={isStarting}>
              {isStarting ? (
                <>
                  <Loader2 size={14} className="animate-spin" />
                  Starting...
                </>
              ) : (
                <>
                  <Play size={14} />
                  Start Batch
                </>
              )}
            </Button>
          )}
        </div>
      </CardFooter>
    </Card>
  );
}
