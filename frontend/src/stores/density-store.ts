import { create } from 'zustand';

export type TableDensity = 'compact' | 'comfortable';

const STORAGE_KEY = 'meowDetective.density';

function readStoredDensity(): TableDensity {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'comfortable' ? 'comfortable' : 'compact';
  } catch {
    return 'compact';
  }
}

function applyDensity(density: TableDensity) {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.density = density;
}

/** Applies the stored density once at module load so the first paint is already correct. */
applyDensity(readStoredDensity());

interface DensityState {
  density: TableDensity;
  setDensity: (density: TableDensity) => void;
  toggleDensity: () => void;
}

export const useDensityStore = create<DensityState>((set, get) => ({
  density: readStoredDensity(),
  setDensity: (density) => {
    applyDensity(density);
    try {
      localStorage.setItem(STORAGE_KEY, density);
    } catch {
      // A blocked storage backend must not break the workbench.
    }
    set({ density });
  },
  toggleDensity: () => get().setDensity(get().density === 'compact' ? 'comfortable' : 'compact'),
}));
