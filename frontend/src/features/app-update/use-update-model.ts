import { useEffect, useState } from 'react';
import { useUpdateStore, initializeAppUpdates } from './hooks';
import { UPDATE_RELEASES_URL } from '@/design/app-update';

export function useUpdateStartup() {
  useEffect(() => { void initializeAppUpdates(); }, []);
}

export function useUpdateModel() {
  const state = useUpdateStore();
  const [confirming, setConfirming] = useState(false);
  const [copied, setCopied] = useState(false);
  const pending = ['checking', 'downloading', 'verifying', 'restarting'].includes(state.phase);
  async function copyDownloadLink() {
    try { await navigator.clipboard.writeText(UPDATE_RELEASES_URL); setCopied(true); }
    catch { setCopied(false); }
  }
  return { ...state, pending, confirming, setConfirming, copied, copyDownloadLink,
    confirmInstall: () => { setConfirming(false); void state.install(); },
  };
}
