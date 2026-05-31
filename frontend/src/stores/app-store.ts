import { create } from 'zustand';

/**
 * Import progress state for tracking background import jobs.
 */
interface ImportProgress {
  jobId: string;
  progress: number;
  detail: string;
  partitionName?: string;
  partitionIndex?: number;
  totalPartitions?: number;
  partitionProgress?: number;
}

/**
 * Application-level state store.
 *
 * Manages global application state that doesn't fit into
 * UI or selection stores.
 */
interface AppState {
  // === Import State ===
  /** Current import progress (null when no import running) */
  importProgress: ImportProgress | null;
  /** Set import progress */
  setImportProgress: (progress: ImportProgress | null) => void;
  /** Clear import progress */
  clearImportProgress: () => void;

  // === Error State ===
  /** Global error message (null when no error) */
  globalError: string | null;
  /** Set global error */
  setGlobalError: (error: string | null) => void;
  /** Clear global error */
  clearGlobalError: () => void;

  // === Loading State ===
  /** Whether the app is currently loading something */
  isLoading: boolean;
  /** Loading message */
  loadingMessage: string;
  /** Set loading state */
  setLoading: (loading: boolean, message?: string) => void;

  // === Notification State ===
  /** Recent notifications */
  notifications: Array<{
    id: string;
    type: 'info' | 'success' | 'warning' | 'error';
    message: string;
    timestamp: number;
  }>;
  /** Add a notification */
  addNotification: (type: 'info' | 'success' | 'warning' | 'error', message: string) => void;
  /** Remove a notification by ID */
  removeNotification: (id: string) => void;
  /** Clear all notifications */
  clearNotifications: () => void;
}

let notificationId = 0;

export const useAppStore = create<AppState>((set) => ({
  // Import state
  importProgress: null,
  setImportProgress: (progress) => set({ importProgress: progress }),
  clearImportProgress: () => set({ importProgress: null }),

  // Error state
  globalError: null,
  setGlobalError: (error) => set({ globalError: error }),
  clearGlobalError: () => set({ globalError: null }),

  // Loading state
  isLoading: false,
  loadingMessage: '',
  setLoading: (loading, message = '') => set({ isLoading: loading, loadingMessage: message }),

  // Notification state
  notifications: [],
  addNotification: (type, message) =>
    set((state) => ({
      notifications: [
        ...state.notifications,
        {
          id: `notif-${++notificationId}`,
          type,
          message,
          timestamp: Date.now(),
        },
      ],
    })),
  removeNotification: (id) =>
    set((state) => ({
      notifications: state.notifications.filter((n) => n.id !== id),
    })),
  clearNotifications: () => set({ notifications: [] }),
}));

/**
 * Convenience hooks for common operations
 */

/** Show an info notification */
export const useNotifyInfo = () => {
  const addNotification = useAppStore((s) => s.addNotification);
  return (message: string) => addNotification('info', message);
};

/** Show a success notification */
export const useNotifySuccess = () => {
  const addNotification = useAppStore((s) => s.addNotification);
  return (message: string) => addNotification('success', message);
};

/** Show a warning notification */
export const useNotifyWarning = () => {
  const addNotification = useAppStore((s) => s.addNotification);
  return (message: string) => addNotification('warning', message);
};

/** Show an error notification */
export const useNotifyError = () => {
  const addNotification = useAppStore((s) => s.addNotification);
  return (message: string) => addNotification('error', message);
};
