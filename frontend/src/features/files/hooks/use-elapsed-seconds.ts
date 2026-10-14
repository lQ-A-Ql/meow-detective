import { useEffect, useState } from 'react';

export function useElapsedSeconds(startedAt: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [startedAt]);
  return Math.max(0, Math.floor((now - startedAt) / 1_000));
}
