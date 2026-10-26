import { create } from 'zustand';
import type { FileJumpContext } from '@/types/models';
import { useSelectionStore } from './selection-store';
import i18n from '@/i18n';

export interface FileNavigationRequest {
  caseId: string;
  fileId: string;
  origin: 'external' | 'restore';
}

export interface PendingFileNavigation {
  request: FileNavigationRequest;
  context: FileJumpContext;
}

interface FileNavigationState {
  request?: FileNavigationRequest;
  pending?: PendingFileNavigation;
  begin: (caseId: string, fileId: string, origin: FileNavigationRequest['origin']) => FileNavigationRequest;
  resolve: (request: FileNavigationRequest, context: FileJumpContext) => boolean;
  consume: (pending: PendingFileNavigation) => void;
  cancel: () => void;
}

export const useFileNavigationStore = create<FileNavigationState>((set, get) => ({
  begin(caseId, fileId, origin) {
    const request = { caseId, fileId, origin };
    set({ request, pending: undefined });
    return request;
  },
  resolve(request, context) {
    if (get().request !== request) return false;
    if (context.target.id !== request.fileId) throw new Error(i18n.t('fileBrowser.navigationTargetMismatch'));
    useSelectionStore.setState({
      selectedDirectoryId: context.directory.id,
      selectedFileId: context.target.entryType === 'file' ? context.target.id : undefined,
    });
    set({ pending: { request, context } });
    return true;
  },
  consume(pending) {
    if (get().pending === pending) set({ request: undefined, pending: undefined });
  },
  cancel() {
    set({ request: undefined, pending: undefined });
  },
}));
