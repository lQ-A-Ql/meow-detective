import { useState } from 'react';
import { Monitor, Server, FolderOpen, Loader2 } from 'lucide-react';
import * as tauriDialog from '@tauri-apps/plugin-dialog';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { Label } from '@/app/components/ui/label';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';
import {
  ToggleGroup,
  ToggleGroupItem,
} from '@/app/components/ui/toggle-group';

type Platform = 'windows' | 'linux';

export interface ImportDataSourceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onImport: (sourcePath: string) => void;
  importPending: boolean;
}

export function ImportDataSourceDialog({
  open,
  onOpenChange,
  onImport,
  importPending,
}: ImportDataSourceDialogProps) {
  const [step, setStep] = useState<'platform' | 'form'>('platform');
  const [platform, setPlatform] = useState<Platform>('windows');
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [error, setError] = useState('');

  function reset() {
    setStep('platform');
    setPlatform('windows');
    setName('');
    setPath('');
    setError('');
  }

  function handleOpenChange(nextOpen: boolean) {
    if (!nextOpen) reset();
    onOpenChange(nextOpen);
  }

  function goNext() {
    if (step === 'platform') {
      setStep('form');
      setError('');
    }
  }

  function goBack() {
    if (step === 'form') {
      setStep('platform');
      setError('');
    }
  }

  async function pickFile() {
    try {
      const selected = await tauriDialog.open({
        directory: false,
        multiple: false,
        filters: [
          { name: 'Data Sources', extensions: ['e01', 'E01', 'dd', 'raw', 'img'] },
        ],
      });
      if (selected) setPath(selected as string);
    } catch {
      // Tauri dialog unavailable in non-tauri mode
    }
  }

  async function pickDirectory() {
    try {
      const selected = await tauriDialog.open({ directory: true, multiple: false });
      if (selected) setPath(selected as string);
    } catch {
      // Tauri dialog unavailable in non-tauri mode
    }
  }

  function handleImport() {
    const trimmedPath = path.trim();
    if (!trimmedPath) {
      setError('请选择数据源路径');
      return;
    }
    setError('');
    onImport(trimmedPath);
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>导入数据源</DialogTitle>
          <DialogDescription>
            {step === 'platform'
              ? '步骤 1/2：选择目标平台'
              : '步骤 2/2：填写数据源信息'}
          </DialogDescription>
        </DialogHeader>

        {step === 'platform' ? (
          <div className="py-4 space-y-4">
            <ToggleGroup
              type="single"
              value={platform}
              onValueChange={(v) => v && setPlatform(v as Platform)}
              className="w-full"
            >
              <ToggleGroupItem
                value="windows"
                aria-label="Windows"
                className="flex-1 gap-2 h-16"
              >
                <Monitor size={20} />
                <span className="text-sm font-medium">Windows</span>
              </ToggleGroupItem>
              <ToggleGroupItem
                value="linux"
                aria-label="Linux"
                className="flex-1 gap-2 h-16"
              >
                <Server size={20} />
                <span className="text-sm font-medium">Linux</span>
              </ToggleGroupItem>
            </ToggleGroup>
            <div className="flex justify-end">
              <Button onClick={goNext} size="sm">
                下一步
              </Button>
            </div>
          </div>
        ) : (
          <div className="py-4 space-y-4">
            <div className="space-y-2">
              <Label htmlFor="ds-name">数据源名称</Label>
              <Input
                id="ds-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="例如：Win10-C盘"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="ds-path">数据源路径</Label>
              <div className="flex gap-2">
                <Input
                  id="ds-path"
                  value={path}
                  onChange={(e) => setPath(e.target.value)}
                  placeholder="镜像路径或逻辑目录路径"
                  className="flex-1"
                />
                <Button variant="outline" size="sm" onClick={pickFile} className="shrink-0 gap-1">
                  <FolderOpen size={12} />
                  文件
                </Button>
                <Button variant="outline" size="sm" onClick={pickDirectory} className="shrink-0 gap-1">
                  <FolderOpen size={12} />
                  目录
                </Button>
              </div>
              {error ? (
                <p className="text-[11px] text-red-600">{error}</p>
              ) : null}
            </div>

            <DialogFooter className="gap-2">
              <Button variant="outline" size="sm" onClick={goBack}>
                上一步
              </Button>
              <Button
                size="sm"
                onClick={handleImport}
                disabled={importPending}
              >
                {importPending ? (
                  <>
                    <Loader2 size={14} className="animate-spin" />
                    导入中...
                  </>
                ) : (
                  '导入'
                )}
              </Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
