import { useCallback, useState } from 'react';

export function useLogPanelModel() {
  const [following, setFollowing] = useState(true);
  const stopFollowing = useCallback((value: boolean) => setFollowing(value), []);
  return { following, setFollowing: stopFollowing, returnToLatest: () => setFollowing(true) };
}
