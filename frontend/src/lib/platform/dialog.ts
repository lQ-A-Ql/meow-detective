import { open, save } from '@tauri-apps/plugin-dialog';

export type DialogPath = string | string[] | null;

export interface OpenDialogOptions {
  directory?: boolean;
  multiple?: boolean;
  filters?: Array<{
    name: string;
    extensions: string[];
  }>;
}

export interface SaveDialogOptions {
  defaultPath?: string;
}

export async function openDialog(options: OpenDialogOptions): Promise<DialogPath> {
  return open(options);
}

export async function saveDialog(options: SaveDialogOptions): Promise<string | null> {
  return save(options);
}

export function singleDialogPath(path: DialogPath): string | null {
  if (Array.isArray(path)) {
    return path[0] ?? null;
  }
  return path;
}
