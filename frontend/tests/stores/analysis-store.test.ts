import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ANALYSIS_EXTRACTION_CATEGORIES, isExtractionCategory } from '@/types/analysisUi';
import { labeledProgress, statusFromRun, useAnalysisStore } from '@/stores/analysis-store';

describe('analysis-store', () => {
  beforeEach(() => {
    useAnalysisStore.getState().reset();
  });

  it('starts from the documented default state', () => {
    const state = useAnalysisStore.getState();

    expect(state.extractionRunning).toBe(false);
    expect(state.progressExpanded).toBe(true);
    expect(state.activeTab).toBe('system');
    expect(state.activeLinuxTab).toBe('overview');
    expect(state.activeAndroidTab).toBe('device');
    expect(state.selectedDataSourceId).toBeUndefined();
    expect(state.activePluginId).toBeUndefined();
  });

  it('seeds every extraction category with empty idle progress', () => {
    const { extractionProgress } = useAnalysisStore.getState();

    expect(Object.keys(extractionProgress).sort()).toEqual([...ANALYSIS_EXTRACTION_CATEGORIES].sort());
    for (const category of ANALYSIS_EXTRACTION_CATEGORIES) {
      expect(extractionProgress[category]).toEqual({
        status: 'idle',
        scannedCount: 0,
        artifactCount: 0,
        timelineEventCount: 0,
        warnings: [],
        totalCandidateCount: 0,
        processedCandidateCount: 0,
        structuredCandidateCount: 0,
        unsupportedCandidateCount: 0,
        textFallbackCandidateCount: 0,
        warningCandidateCount: 0,
        checkpointHitCount: 0,
      });
    }
  });

  it('replaces the whole progress record', () => {
    const current = useAnalysisStore.getState().extractionProgress;
    const next = {
      ...current,
      Registry: { ...current.Registry, status: 'running' as const, scannedCount: 7 },
    };

    useAnalysisStore.getState().setExtractionProgress(next);

    expect(useAnalysisStore.getState().extractionProgress.Registry).toMatchObject({
      status: 'running',
      scannedCount: 7,
    });
    expect(useAnalysisStore.getState().extractionProgress.BrowserHistory.status).toBe('idle');
  });

  it('merges a partial patch into one category without dropping defaults', () => {
    useAnalysisStore.getState().updateExtractionProgress('Registry', {
      status: 'running',
      scannedCount: 12,
      warnings: ['unreadable hive'],
    });

    const registry = useAnalysisStore.getState().extractionProgress.Registry;
    expect(registry).toMatchObject({
      status: 'running',
      scannedCount: 12,
      warnings: ['unreadable hive'],
      artifactCount: 0,
      timelineEventCount: 0,
    });
    expect(useAnalysisStore.getState().extractionProgress.BrowserHistory).toEqual({
      status: 'idle',
      scannedCount: 0,
      artifactCount: 0,
      timelineEventCount: 0,
      warnings: [],
      totalCandidateCount: 0,
      processedCandidateCount: 0,
      structuredCandidateCount: 0,
      unsupportedCandidateCount: 0,
      textFallbackCandidateCount: 0,
      warningCandidateCount: 0,
      checkpointHitCount: 0,
    });
  });

  it('updates Linux categories and optional diagnostic fields', () => {
    useAnalysisStore.getState().updateExtractionProgress('LinuxJournal', {
      status: 'failed',
      error: 'journal unreadable',
      currentPath: '/var/log/journal',
    });

    expect(useAnalysisStore.getState().extractionProgress.LinuxJournal).toMatchObject({
      status: 'failed',
      error: 'journal unreadable',
      currentPath: '/var/log/journal',
    });
  });

  it('resets extraction progress without touching unrelated UI state', () => {
    useAnalysisStore.getState().updateExtractionProgress('Email', {
      status: 'success',
      artifactCount: 4,
    });
    useAnalysisStore.getState().setExtractionRunning(true);
    useAnalysisStore.getState().setActiveTab('email');

    useAnalysisStore.getState().resetExtractionProgress();

    const state = useAnalysisStore.getState();
    expect(state.extractionProgress.Email.status).toBe('idle');
    expect(state.extractionProgress.Email.artifactCount).toBe(0);
    expect(state.extractionRunning).toBe(true);
    expect(state.activeTab).toBe('email');
  });

  it('hands out independent progress records on every reset', () => {
    const first = useAnalysisStore.getState().extractionProgress;
    useAnalysisStore.getState().resetExtractionProgress();
    const second = useAnalysisStore.getState().extractionProgress;

    expect(second).not.toBe(first);
    expect(second.Registry).not.toBe(first.Registry);
  });

  it('stores running, expansion and tab selection flags', () => {
    const store = useAnalysisStore.getState();

    store.setExtractionRunning(true);
    store.setProgressExpanded(false);
    store.setActiveTab('registry');
    store.setActiveLinuxTab('journal');
    store.setActiveAndroidTab('packages');

    expect(useAnalysisStore.getState()).toMatchObject({
      extractionRunning: true,
      progressExpanded: false,
      activeTab: 'registry',
      activeLinuxTab: 'journal',
      activeAndroidTab: 'packages',
    });

    store.setExtractionRunning(false);
    store.setProgressExpanded(true);
    expect(useAnalysisStore.getState().extractionRunning).toBe(false);
    expect(useAnalysisStore.getState().progressExpanded).toBe(true);
  });

  it('stores and clears the selected data source and plugin ids', () => {
    const store = useAnalysisStore.getState();

    store.setSelectedDataSourceId('ds-1');
    store.setActivePluginId('plugin.wechat');
    expect(useAnalysisStore.getState().selectedDataSourceId).toBe('ds-1');
    expect(useAnalysisStore.getState().activePluginId).toBe('plugin.wechat');

    store.setSelectedDataSourceId(undefined);
    store.setActivePluginId(undefined);
    expect(useAnalysisStore.getState().selectedDataSourceId).toBeUndefined();
    expect(useAnalysisStore.getState().activePluginId).toBeUndefined();
  });

  it('reset restores defaults while keeping actions usable', () => {
    const store = useAnalysisStore.getState();
    store.setExtractionRunning(true);
    store.setProgressExpanded(false);
    store.setActiveTab('plugin');
    store.setActiveLinuxTab('docker');
    store.setActiveAndroidTab('packages');
    store.setSelectedDataSourceId('ds-1');
    store.setActivePluginId('plugin.one');
    store.updateExtractionProgress('Registry', { status: 'running' });

    store.reset();

    const state = useAnalysisStore.getState();
    expect(state).toMatchObject({
      extractionRunning: false,
      progressExpanded: true,
      activeTab: 'system',
      activeLinuxTab: 'overview',
      activeAndroidTab: 'device',
      selectedDataSourceId: undefined,
      activePluginId: undefined,
    });
    expect(state.extractionProgress.Registry.status).toBe('idle');

    state.setActiveTab('files');
    expect(useAnalysisStore.getState().activeTab).toBe('files');
  });

  it.each([
    ['failed', 'failed'],
    ['unavailable', 'failed'],
    ['partial', 'partial'],
    ['success', 'success'],
    ['running', 'success'],
    ['idle', 'success'],
  ])('maps run status %s to progress state %s', (runStatus, expected) => {
    expect(statusFromRun(runStatus)).toBe(expected);
  });

  it('labels every extraction category without mutating the source record', () => {
    const progress = useAnalysisStore.getState().extractionProgress;
    const t = vi.fn((key: string) => `label:${key}`);

    const labeled = labeledProgress(progress, t);

    expect(t).toHaveBeenCalledTimes(ANALYSIS_EXTRACTION_CATEGORIES.length);
    for (const category of ANALYSIS_EXTRACTION_CATEGORIES) {
      expect(t).toHaveBeenCalledWith(`analysis.extraction.${category}`);
      expect(labeled[category]).toEqual({
        ...progress[category],
        label: `label:analysis.extraction.${category}`,
      });
    }
    expect(Object.keys(labeled).sort()).toEqual([...ANALYSIS_EXTRACTION_CATEGORIES].sort());
    expect(progress.Registry).not.toHaveProperty('label');
  });

  it('recognizes only known extraction category names', () => {
    expect(isExtractionCategory('Registry')).toBe(true);
    expect(isExtractionCategory('LinuxMysqlServices')).toBe(true);
    expect(isExtractionCategory('registry')).toBe(false);
    expect(isExtractionCategory('Unknown')).toBe(false);
    expect(isExtractionCategory('')).toBe(false);
  });

  it('preserves category identity in update patches for all categories', () => {
    const categories = [...ANALYSIS_EXTRACTION_CATEGORIES];
    const store = useAnalysisStore.getState();

    for (const category of categories) {
      store.updateExtractionProgress(category, { scannedCount: 3 });
    }

    const progress = useAnalysisStore.getState().extractionProgress;
    for (const category of categories) {
      expect(progress[category].scannedCount).toBe(3);
      expect(progress[category].status).toBe('idle');
    }
  });
});
