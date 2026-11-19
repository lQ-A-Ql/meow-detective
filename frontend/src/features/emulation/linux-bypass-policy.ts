import { isApiErrorDto } from '@/lib/errors';

/** Backend error code raised when the COW overlay has been poisoned and must be released. */
export const LINUX_BYPASS_POISONED_CODE = 'LINUX_BYPASS_OVERLAY_POISONED';

const SESSION_STATE_PATTERN = /(SESSION_(NOT_FOUND|EXPIRED|RELEASED|BUSY|STATE_MISMATCH)|INVALID_SESSION)/;

/**
 * Only overlay poisoning, fatal transport failures and session-state errors may
 * release the prepared session after a Linux bypass call. Every other failure
 * keeps the session alive so the investigator can pick an alternative exit.
 */
export function shouldReleaseOnBypassError(error: unknown): boolean {
  if (!isApiErrorDto(error)) {
    return false;
  }
  if (error.code === LINUX_BYPASS_POISONED_CODE) {
    return true;
  }
  if (error.category === 'internal' && error.recoverable === false) {
    return true;
  }
  return SESSION_STATE_PATTERN.test(error.code);
}
