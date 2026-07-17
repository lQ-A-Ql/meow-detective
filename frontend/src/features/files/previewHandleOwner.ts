import { useEffect, useMemo } from 'react';

import { closeFileHandle } from '@/lib/api/files';

interface PreviewHandleOwner {
  disposed: boolean;
  handles: Set<string>;
  mountVersion: number;
}

export function usePreviewHandleOwner(scopeKey: string) {
  const owner = useMemo<PreviewHandleOwner>(
    () => ({ disposed: false, handles: new Set<string>(), mountVersion: 0 }),
    [scopeKey],
  );
  useEffect(() => {
    owner.disposed = false;
    owner.mountVersion += 1;
    const mountedVersion = owner.mountVersion;
    return () => {
      queueMicrotask(() => {
        if (owner.mountVersion !== mountedVersion) {
          return;
        }
        owner.disposed = true;
        const handles = [...owner.handles];
        owner.handles.clear();
        for (const handleId of handles) {
          void closeFileHandle(handleId).catch(() => undefined);
        }
      });
    };
  }, [owner]);
  return owner;
}

export async function adoptPreviewHandle(
  owner: PreviewHandleOwner,
  handleId: string,
  signal?: AbortSignal,
) {
  if (owner.disposed || signal?.aborted) {
    await closeFileHandle(handleId).catch(() => undefined);
    throw cancelledPreviewRequest();
  }
  owner.handles.add(handleId);
  if (owner.disposed || signal?.aborted) {
    owner.handles.delete(handleId);
    await closeFileHandle(handleId).catch(() => undefined);
    throw cancelledPreviewRequest();
  }
}

export async function releasePreviewHandle(owner: PreviewHandleOwner, handleId?: string) {
  if (!handleId) {
    return;
  }
  owner.handles.delete(handleId);
  await closeFileHandle(handleId).catch(() => undefined);
}

export async function ensurePreviewRequestActive(
  owner: PreviewHandleOwner,
  handleId: string,
  signal: AbortSignal,
) {
  if (owner.disposed || signal.aborted) {
    await releasePreviewHandle(owner, handleId);
    throw cancelledPreviewRequest();
  }
}

function cancelledPreviewRequest() {
  return new DOMException('Preview request cancelled', 'AbortError');
}
