if (import.meta.env.PROD) {
  throw new Error('Browser event adapter is restricted to the development server.');
}

export type UnlistenFn = () => void;

/** Browser fixtures have no host event stream; query invalidation still uses the real frontend. */
export async function listen<T>(_topic: string, _handler: (event: { payload: T }) => void): Promise<UnlistenFn> {
  return () => undefined;
}
