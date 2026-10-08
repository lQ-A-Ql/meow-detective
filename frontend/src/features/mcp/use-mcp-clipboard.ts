import { useCallback, useState } from 'react';

export function useMcpClipboard() {
  const [copiedValue, setCopiedValue] = useState<string>();
  const [copyError, setCopyError] = useState(false);
  const copy = useCallback(async (value: string) => {
    try {
      await navigator.clipboard?.writeText(value);
      setCopiedValue(value);
      setCopyError(false);
      window.setTimeout(() => setCopiedValue((current) => current === value ? undefined : current), 1500);
    } catch {
      setCopyError(true);
    }
  }, []);
  return { copiedValue, copyError, copy };
}
