import { useCallback, useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router';
import { useTranslation } from 'react-i18next';
import { getApplicationLog, readApplicationLogs, reopenApplicationLogWriter } from '@/lib/api/applicationLog';
import { emptyLogBuffer, mergeLogSnapshot, type LogBuffer } from './log-buffer';
import { errorMessage } from '@/lib/errors';
import type { ApplicationLogFilter, ApplicationLogRecord } from '@/types/applicationLog';

let retainedBuffer = emptyLogBuffer();
let retainedFilter = '';

export function useLogWorkspaceModel() {
  const [params] = useSearchParams();
  const { t } = useTranslation();
  const [filter, setFilter] = useState<ApplicationLogFilter>({
    text: '',
    target: '',
    caseId: params.get('caseId') ?? '',
    dataSourceId: params.get('dataSourceId') ?? '',
    jobId: params.get('jobId') ?? '',
  });
  const [buffer, setBuffer] = useState<LogBuffer>(() =>
    retainedFilter === JSON.stringify(filter) ? retainedBuffer : emptyLogBuffer(),
  );
  const [paused, setPaused] = useState(false);
  const [error, setError] = useState<string>();
  const [selected, setSelected] = useState<ApplicationLogRecord>();
  const [refresh, setRefresh] = useState(0);
  const [busy, setBusy] = useState(false);
  const [copyStatus, setCopyStatus] = useState<string>();
  const bufferRef = useRef(buffer);
  const version = useRef(0);
  const requestActive = useRef(false);
  const clearPending = useRef(false);
  const mounted = useRef(true);
  const detailVersion = useRef(0);
  const filterKey = JSON.stringify(filter);
  const update = useCallback((next: LogBuffer) => {
    bufferRef.current = next;
    retainedBuffer = next;
    setBuffer(next);
  }, []);

  useEffect(() => {
    const caseId = params.get('caseId') ?? '';
    const dataSourceId = params.get('dataSourceId') ?? '';
    const jobId = params.get('jobId') ?? '';
    setFilter((current) =>
      current.caseId === caseId && current.dataSourceId === dataSourceId && current.jobId === jobId
        ? current
        : { ...current, caseId, dataSourceId, jobId },
    );
  }, [params]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    const generation = ++version.current;
    if (retainedFilter !== filterKey) {
      retainedFilter = filterKey;
      clearPending.current = false;
      detailVersion.current++;
      setSelected(undefined);
      update(emptyLogBuffer());
    }
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    function schedule() {
      if (timer) clearTimeout(timer);
      timer = setTimeout(poll, 250);
    }
    async function poll() {
      if (cancelled || (paused && !clearPending.current) || document.hidden) return;
      if (requestActive.current) {
        schedule();
        return;
      }
      requestActive.current = true;
      const current = bufferRef.current;
      try {
        const snapshot = await readApplicationLogs({
          direction: current.forward.length ? 'forward' : 'latest',
          cursors: current.forward,
          filter,
          limitPerLevel: 100,
        });
        if (!cancelled && generation === version.current) {
          const incoming = clearPending.current
            ? { ...snapshot, pages: snapshot.pages.map((page) => ({ ...page, records: [] })) }
            : snapshot;
          update(mergeLogSnapshot(bufferRef.current, incoming));
          clearPending.current = false;
          setError(undefined);
        }
      } catch (failure) {
        if (!cancelled && generation === version.current) {
          setError(errorMessage(failure));
          if ((failure as { code?: string }).code === 'INVALID_INPUT') update(emptyLogBuffer());
        }
      } finally {
        requestActive.current = false;
        if (!cancelled) schedule();
      }
    }
    const visible = () => {
      if (timer) clearTimeout(timer);
      if (!document.hidden) void poll();
    };
    void poll();
    document.addEventListener('visibilitychange', visible);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      document.removeEventListener('visibilitychange', visible);
    };
  }, [filter, filterKey, paused, refresh, update]);

  async function loadHistory() {
    if (requestActive.current || !bufferRef.current.backward.length) return;
    setPaused(true);
    requestActive.current = true;
    setBusy(true);
    const current = bufferRef.current;
    const isCurrent = () =>
      mounted.current && retainedFilter === filterKey && bufferRef.current.backward === current.backward;
    try {
      const snapshot = await readApplicationLogs({
        direction: 'backward',
        cursors: current.backward,
        filter,
        limitPerLevel: 100,
      });
      if (isCurrent()) {
        update(mergeLogSnapshot(bufferRef.current, snapshot, true));
        setError(undefined);
      }
    } catch (failure) {
      if (isCurrent()) setError(errorMessage(failure));
    } finally {
      requestActive.current = false;
      setBusy(false);
    }
  }

  async function reopenWriter() {
    setBusy(true);
    try {
      await reopenApplicationLogWriter();
      setError(undefined);
      setRefresh((value) => value + 1);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }

  async function copySelected() {
    if (!selected) return;
    try {
      await navigator.clipboard.writeText(JSON.stringify(selected, null, 2));
      setCopyStatus(t('logs.copied'));
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  return {
    filter,
    setFilter,
    buffer,
    paused,
    setPaused,
    selected,
    select(record: ApplicationLogRecord) {
      const generation = ++detailVersion.current;
      setSelected(record);
      setCopyStatus(undefined);
      void getApplicationLog(record.sessionId, record.sequence).then(
        (detail) => {
          if (mounted.current && generation === detailVersion.current) {
            setSelected((current) =>
              current?.sessionId === detail.sessionId && current.sequence === detail.sequence ? detail : current,
            );
          }
        },
        (failure) => {
          if (mounted.current && generation === detailVersion.current) setError(errorMessage(failure));
        },
      );
    },
    closeDetails() {
      detailVersion.current++;
      setSelected(undefined);
    },
    clearDisplay() {
      version.current++;
      clearPending.current = true;
      detailVersion.current++;
      setSelected(undefined);
      update({ ...bufferRef.current, rows: { info: [], warning: [], error: [] }, forward: [], backward: [] });
      setRefresh((value) => value + 1);
    },
    loadHistory,
    reopenWriter,
    copySelected,
    copyStatus,
    busy,
    error,
  };
}

export type LogWorkspaceModel = ReturnType<typeof useLogWorkspaceModel>;
