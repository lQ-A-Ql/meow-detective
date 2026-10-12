import { useEffect, useState } from 'react';

type Listener = (message: string | null) => void;

const listeners = new Set<Listener>();
let dismissTimer: ReturnType<typeof setTimeout> | undefined;

/** One shared transient confirmation channel; components never render their own. */
export function toast(message: string) {
  for (const listener of listeners) listener(message);
  if (dismissTimer) clearTimeout(dismissTimer);
  dismissTimer = setTimeout(() => {
    for (const listener of listeners) listener(null);
  }, 3000);
}

export function Toaster() {
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    const listener: Listener = (next) => setMessage(next);
    listeners.add(listener);
    return () => { listeners.delete(listener); };
  }, []);

  if (!message) return null;
  return (
    <div
      role="status"
      aria-live="polite"
      className="ui-overlay-surface pointer-events-none fixed bottom-12 left-1/2 z-50 -translate-x-1/2 rounded-ctl border border-forensics-border px-3 py-2 text-[11px] text-forensics-ink"
    >
      {message}
    </div>
  );
}
