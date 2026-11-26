import { describe, expect, it } from 'vitest';
import { REPORT_ACTIONS, DEFAULT_REPORT_SCOPE } from '@/features/reports/report-config';

describe('report history terminal actions', () => {
  it('offers saved file access only for completed reports', () => {
    expect(REPORT_ACTIONS.completed).toBe('download');
    expect(REPORT_ACTIONS.running).toBeNull();
    expect(REPORT_ACTIONS.failed).toBe('retry');
    expect(REPORT_ACTIONS.cancelled).toBe('retry');
  });

  it('selects the four structured report sections by default', () => {
    expect(DEFAULT_REPORT_SCOPE).toEqual({ operatingSystem: true, hardware: true, browsers: true, email: true });
  });
});
