type QueryRefetch = () => Promise<unknown>;

type AnalysisPlatform = 'windows' | 'linux' | undefined;

function queryError(result: unknown): unknown {
  if (!result || typeof result !== 'object' || !('error' in result)) {
    return undefined;
  }
  return result.error;
}

export async function refreshAnalysisQueries(
  platform: AnalysisPlatform,
  windowsQueries: readonly QueryRefetch[],
  linuxQueries: readonly QueryRefetch[] = [],
): Promise<void> {
  const results = platform === 'windows'
    ? await Promise.all(windowsQueries.map((query) => query()))
    : platform === 'linux'
      ? await Promise.all(linuxQueries.map((query) => query()))
      : [];
  const error = results.map(queryError).find(Boolean);
  if (error) {
    throw error;
  }
}
