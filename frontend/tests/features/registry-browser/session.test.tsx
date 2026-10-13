import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createRegistryBrowserMock } from '../../../mocks/registry-browser/client';
import fixture from '../../../mocks/registry-browser/data.json';
import type { RegistryHiveSession } from '@/types/registryBrowser';
import { useRegistrySession } from '@/features/registry-browser/hooks/use-registry-session';

const context = { caseId: fixture.caseId, dataSourceId: fixture.dataSourceId, sourceRevision: 'synthetic-v1' };

describe('registry hive session scope', () => {
  it('discards and closes a late open response after another hive is selected', async () => {
    const base = createRegistryBrowserMock();
    const old = await base.openHive({ fileId: fixture.hives[0].fileId, view: 'base' });
    const current = await base.openHive({ fileId: fixture.hives[1].fileId, view: 'base' });
    let resolveOld!: (session: RegistryHiveSession) => void;
    const pending = new Promise<RegistryHiveSession>((resolve) => { resolveOld = resolve; });
    const client = { ...base, openHive: vi.fn().mockReturnValueOnce(pending).mockResolvedValueOnce(current),
      closeHive: vi.fn(base.closeHive) };
    const hook = renderHook(() => useRegistrySession(client, context));
    act(() => { void hook.result.current.openHive(old.hive.fileId); void hook.result.current.openHive(current.hive.fileId); });
    await waitFor(() => expect(hook.result.current.session?.sessionId).toBe(current.sessionId));
    await act(async () => { resolveOld(old); await pending; });
    expect(hook.result.current.session?.hive.name).toBe('SYSTEM');
    expect(client.closeHive).toHaveBeenCalledWith({ sessionId: old.sessionId });
    hook.unmount();
    expect(client.closeHive).toHaveBeenCalledWith({ sessionId: current.sessionId });
  });

  it('rejects a session belonging to a different case before exposing its content', async () => {
    const base = createRegistryBrowserMock();
    const response = await base.openHive({ fileId: fixture.hives[0].fileId, view: 'base' });
    const foreign = { ...response, source: { ...response.source, caseId: 'different-synthetic-case' } };
    const client = { ...base, openHive: vi.fn().mockResolvedValue(foreign), closeHive: vi.fn(base.closeHive) };
    const hook = renderHook(() => useRegistrySession(client, context));
    await act(async () => { await hook.result.current.openHive(fixture.hives[0].fileId); });
    expect(hook.result.current.session).toBeUndefined();
    expect(hook.result.current.error).toMatchObject({ code: 'expiredSession' });
    expect(client.closeHive).toHaveBeenCalledWith({ sessionId: response.sessionId });
  });
});
