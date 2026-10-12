import type { RecentCase } from '@/types/models';

export interface CaseWelcomeFormsProps {
  caseRoot: string;
  setCaseRoot: (v: string) => void;
  caseName: string;
  setCaseName: (v: string) => void;
  onCreateCase: () => void;
  createPending: boolean;
  createError: string | null;
  openCasePath: string;
  setOpenCasePath: (v: string) => void;
  onOpenCase: (path: string) => void;
  openPending: boolean;
  openError: string | null;
  recentCases: RecentCase[];
  onRequestDeleteCase: (recentCase: RecentCase) => void;
}
