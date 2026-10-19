import { create } from 'zustand';
import { checkAppUpdate, getAppUpdateInfo, installAppUpdate } from '@/lib/api/app-update';
import { UPDATE_CHECK_INTERVAL_MS, UPDATE_ERROR_KEYS, UPDATE_STORAGE_KEY } from '@/design/app-update';
import type { AppUpdateInfo, AppUpdateProgress } from '@/types/appUpdate';

type Phase = 'idle' | 'checking' | 'current' | 'available' | 'downloading' | 'verifying' | 'restarting' | 'failed';
interface UpdateState {
  info?: AppUpdateInfo;
  phase: Phase;
  progress?: AppUpdateProgress;
  errorKey?: string;
  check: (automatic?: boolean) => Promise<void>;
  install: () => Promise<void>;
}
const busy = (phase: Phase) => ['checking', 'downloading', 'verifying', 'restarting'].includes(phase);
function errorKey(error: unknown): string {
  const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : '';
  return UPDATE_ERROR_KEYS[code] ?? 'appUpdate.errors.failed';
}

export const useUpdateStore = create<UpdateState>((set, get) => ({
  phase: 'idle',
  check: async (automatic = false) => {
    if (busy(get().phase)) return;
    if (automatic) {
      try {
        const previous = Number(localStorage.getItem(UPDATE_STORAGE_KEY));
        const elapsed = Date.now() - previous;
        if (previous > 0 && elapsed >= 0 && elapsed < UPDATE_CHECK_INTERVAL_MS) return;
        localStorage.setItem(UPDATE_STORAGE_KEY, String(Date.now()));
      } catch { /* Storage restrictions must not prevent offline use. */ }
    }
    set({ phase: 'checking', errorKey: undefined });
    try {
      const info = await checkAppUpdate();
      set({ info, phase: info.available ? 'available' : info.configured ? 'current' : 'idle' });
    } catch (error) {
      set({ phase: automatic ? 'idle' : 'failed', errorKey: automatic ? undefined : errorKey(error) });
    }
  },
  install: async () => {
    const { info, phase } = get();
    if (!info?.available?.schemaCompatible || !info.portable || !info.configured || busy(phase)) return;
    set({ phase: 'downloading', progress: undefined, errorKey: undefined });
    try {
      await installAppUpdate(info.available.version, (progress) => {
        const phase = ['verifying', 'restarting'].includes(progress.phase) ? progress.phase as 'verifying' | 'restarting' : 'downloading';
        set({ phase, progress });
      });
      set({ phase: 'restarting' });
    } catch (error) { set({ phase: 'failed', errorKey: errorKey(error) }); }
  },
}));

let initialized = false;
export async function initializeAppUpdates() {
  if (initialized) return;
  initialized = true;
  try {
    const info = await getAppUpdateInfo();
    useUpdateStore.setState({ info });
    if (info.configured) await useUpdateStore.getState().check(true);
  } catch { /* No update transport in browser previews; keep normal startup. */ }
}
