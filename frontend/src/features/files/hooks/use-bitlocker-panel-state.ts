import { useEffect, useRef, useState } from 'react';
import type { BitLockerDictionaryBackend } from '@/types/models';
import type { BitLockerUnlockMethod, BitLockerVolumeModel } from './use-bitlocker-volume';

async function wipeClipboardSecret(password?: string) {
  if (!password || !navigator.clipboard) return;
  try {
    if (!navigator.clipboard.readText || await navigator.clipboard.readText() === password) {
      await navigator.clipboard.writeText('');
    }
  } catch { /* Clipboard access can be denied by the operating system. */ }
}
/** Own transient credentials and bounded clipboard lifetime outside the view. */
export function useBitLockerPanelState(model: BitLockerVolumeModel) {
  const [method, setMethod] = useState<BitLockerUnlockMethod>('password');
  const [credential, setCredential] = useState('');
  const [dictionaryBackend, setDictionaryBackend] = useState<BitLockerDictionaryBackend>('cpu');
  const [passwordCopied, setPasswordCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setInterval>>();
  const copyEpoch = useRef(0);
  const mounted = useRef(true);
  const heldSecret = useRef<string>();

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      copyEpoch.current += 1;
      clearInterval(timer.current);
      void wipeClipboardSecret(heldSecret.current);
      heldSecret.current = undefined;
    };
  }, []);

  const copyRecoveredPassword = async (password: string) => {
    if (!password || !navigator.clipboard) return;
    const epoch = ++copyEpoch.current;
    try {
      await navigator.clipboard.writeText(password);
      if (!mounted.current || epoch !== copyEpoch.current) {
        if (heldSecret.current !== password) await wipeClipboardSecret(password);
        return;
      }
      heldSecret.current = password;
      setPasswordCopied(true);
      clearInterval(timer.current);
      timer.current = setInterval(() => {
        setPasswordCopied(false);
        heldSecret.current = undefined;
        void wipeClipboardSecret(password);
        clearInterval(timer.current);
      }, 1500);
    } catch {
      if (mounted.current && epoch === copyEpoch.current) setPasswordCopied(false);
    }
  };

  return {
    method, setMethod, credential, setCredential, dictionaryBackend, setDictionaryBackend,
    passwordCopied, copyRecoveredPassword,
    canSubmit: Boolean(credential) && !model.unlocking && !model.memoryUnlocking && !model.dictionaryAttacking,
    submitCredential: () => {
      const submitted = credential;
      setCredential('');
      void model.unlock(method, submitted);
    },
  };
}

export type BitLockerPanelState = ReturnType<typeof useBitLockerPanelState>;
