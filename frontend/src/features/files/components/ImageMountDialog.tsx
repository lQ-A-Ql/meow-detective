import {
  CircleCheck,
  Disc3,
  FolderOpen,
  HardDrive,
  LoaderCircle,
  LogOut,
  Play,
  ShieldCheck,
  TriangleAlert,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';
import { Field, FieldError, FieldHint, FieldLabel } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { Tabs, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { formatBytes } from '@/lib/format-bytes';
import type { ImageMountModel } from '@/features/files/hooks/use-image-mount-model';

interface ImageMountDialogProps {
  model: ImageMountModel;
}

function stateLabel(state: string, t: (key: string) => string) {
  if (state === 'mounted') return t('fileBrowser.mount.status.mounted');
  if (state === 'preparing') return t('fileBrowser.mount.status.preparing');
  if (state === 'unmounting') return t('fileBrowser.mount.status.unmounting');
  if (state === 'failed') return t('fileBrowser.mount.status.failed');
  return state;
}

function sourceLabel(source: ImageMountModel['dataSources'][number]) {
  return `${source.name} (${source.platform.toUpperCase()} / ${source.kind.toUpperCase()})`;
}

export function ImageMountDialog({ model }: ImageMountDialogProps) {
  const { t } = useTranslation();
  const selectedMount = model.selectedMount;
  const selectedEmulation = model.selectedEmulation;
  const isEmulation = model.mountMode === 'emulation';
  const canSubmit = Boolean(model.selectedSourceId
    && (model.mountMode !== 'logicalPartition' || model.selectedPartition))
    && !selectedMount
    && !selectedEmulation
    && !model.isSubmitting;

  return (
    <Dialog open={model.dialogOpen} onOpenChange={model.setDialogOpen}>
      <DialogContent className="max-h-[min(720px,calc(100vh-2rem))] overflow-y-auto p-0 sm:max-w-2xl">
        <div className="border-b border-forensics-border bg-forensics-panel-strong px-5 py-4">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2 text-[16px]">
              <ShieldCheck className="size-4 text-forensics-primary-blue" />
              {isEmulation ? t('fileBrowser.mount.emulationTitle') : t('fileBrowser.mount.title')}
            </DialogTitle>
            <DialogDescription>
              {isEmulation
                ? t('fileBrowser.mount.emulationDescription')
                : t('fileBrowser.mount.description')}
            </DialogDescription>
          </DialogHeader>
        </div>

        <form
          className="space-y-4 p-5"
          onSubmit={(event) => {
            event.preventDefault();
            void model.submit();
          }}
        >
          <Tabs
            value={model.mountMode}
            onValueChange={(value) => model.setMountMode(value as ImageMountModel['mountMode'])}
          >
            <TabsList className="w-full">
              <TabsTrigger value="logicalPartition">
                {t('fileBrowser.mount.mode.logical')}
              </TabsTrigger>
              <TabsTrigger value="physicalDisk">
                {t('fileBrowser.mount.mode.physical')}
              </TabsTrigger>
              <TabsTrigger value="emulation">
                {t('fileBrowser.mount.mode.emulation')}
              </TabsTrigger>
            </TabsList>
          </Tabs>

          <div className="grid gap-4 md:grid-cols-[minmax(0,1.4fr)_minmax(220px,1fr)]">
            <section className="space-y-4 border border-forensics-border bg-forensics-surface p-4">
              <div className="flex items-center justify-between gap-3 border-b border-forensics-border pb-3">
                <div className="flex items-center gap-2 text-[12px] text-forensics-text">
                  <HardDrive className="size-4 text-forensics-muted" />
                  {t('fileBrowser.mount.sourceSection')}
                </div>
                <Badge variant="secondary">
                  {isEmulation ? t('fileBrowser.mount.cowOverlay') : t('fileBrowser.mount.readOnly')}
                </Badge>
              </div>

              <Field>
                <FieldLabel htmlFor="image-mount-source">{t('fileBrowser.mount.sourceLabel')}</FieldLabel>
                <Select value={model.selectedSourceId} onValueChange={model.setSelectedSourceId}>
                  <SelectTrigger id="image-mount-source" variant="forensics">
                    <SelectValue placeholder={t('fileBrowser.mount.sourcePlaceholder')} />
                  </SelectTrigger>
                  <SelectContent>
                    {model.dataSources.map((source) => (
                      <SelectItem key={source.id} value={source.id}>
                        {sourceLabel(source)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <FieldHint>{t('fileBrowser.mount.sourceHint')}</FieldHint>
              </Field>

              {model.mountMode === 'logicalPartition' ? (
                <Field>
                  <FieldLabel htmlFor="image-mount-partition">{t('fileBrowser.mount.partitionLabel')}</FieldLabel>
                  <Select
                    value={model.selectedPartitionIndex}
                    onValueChange={model.setSelectedPartitionIndex}
                    disabled={model.partitions.length === 0}
                  >
                    <SelectTrigger id="image-mount-partition" variant="mono">
                      <SelectValue placeholder={t('fileBrowser.mount.partitionPlaceholder')} />
                    </SelectTrigger>
                    <SelectContent>
                      {model.partitions.map((partition) => (
                        <SelectItem key={partition.index} value={String(partition.index)}>
                          {t('fileBrowser.mount.partitionOption', {
                            index: partition.index,
                            name: partition.name,
                            filesystem: partition.filesystem ?? partition.kindLabel,
                            size: formatBytes(partition.length),
                          })}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FieldHint>{t('fileBrowser.mount.partitionHint')}</FieldHint>
                </Field>
              ) : model.mountMode === 'physicalDisk' ? (
                <div className="border border-forensics-border bg-forensics-panel p-3 text-[11px] leading-5 text-forensics-muted">
                  {t('fileBrowser.mount.physicalDescription')}
                </div>
              ) : (
                <div className="border border-forensics-border bg-forensics-panel p-3 text-[11px] leading-5 text-forensics-muted">
                  {t('fileBrowser.mount.emulationSourceHint')}
                </div>
              )}
            </section>

            <section className="space-y-4 border border-forensics-border bg-forensics-panel p-4">
              <div className="flex items-center gap-2 border-b border-forensics-border pb-3 text-[12px] text-forensics-text">
                <HardDrive className="size-4 text-forensics-muted" />
                {t('fileBrowser.mount.optionsSection')}
              </div>

              {model.mountMode === 'logicalPartition' ? (
                <Field>
                  <FieldLabel htmlFor="image-mount-point">{t('fileBrowser.mount.mountPointLabel')}</FieldLabel>
                  <Select value={model.mountPoint} onValueChange={model.setMountPoint}>
                    <SelectTrigger id="image-mount-point" variant="mono">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="auto">{t('fileBrowser.mount.autoDrive')}</SelectItem>
                      {model.mountPointOptions.map((drive) => (
                        <SelectItem key={drive} value={drive}>{drive}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FieldHint>{t('fileBrowser.mount.mountPointHint')}</FieldHint>
                </Field>
              ) : model.mountMode === 'physicalDisk' ? (
                <div className="space-y-1 border border-forensics-border bg-forensics-surface p-3">
                  <div className="text-[11px] text-forensics-text">
                    {t('fileBrowser.mount.physicalTargetTitle')}
                  </div>
                  <p className="text-[11px] leading-5 text-forensics-muted">
                    {t('fileBrowser.mount.physicalTargetHint')}
                  </p>
                </div>
              ) : (
                <Field>
                  <FieldLabel htmlFor="emulation-recovery-iso">
                    {t('fileBrowser.mount.recoveryIsoLabel')}
                  </FieldLabel>
                  <div className="flex min-w-0 gap-2">
                    <Input
                      id="emulation-recovery-iso"
                      value={model.recoveryIsoPath}
                      readOnly
                      placeholder={t('fileBrowser.mount.recoveryIsoPlaceholder')}
                      className="min-w-0 font-mono text-[11px]"
                    />
                    <Button
                      type="button"
                      variant="forensicsOutline"
                      size="icon"
                      onClick={() => void model.pickRecoveryIso()}
                      title={t('fileBrowser.mount.chooseRecoveryIso')}
                      aria-label={t('fileBrowser.mount.chooseRecoveryIso')}
                    >
                      <FolderOpen />
                    </Button>
                  </div>
                  <FieldHint>{t('fileBrowser.mount.recoveryIsoHint')}</FieldHint>
                </Field>
              )}

              <div className="space-y-2 border border-forensics-sakura-300 bg-forensics-sakura-100/25 p-3">
                <div className="flex items-center gap-2 text-[11px] text-forensics-text">
                  <ShieldCheck className="size-4 text-forensics-primary-blue" />
                  {isEmulation
                    ? t('fileBrowser.mount.cowTitle')
                    : t('fileBrowser.mount.readOnlyTitle')}
                </div>
                <p className="text-[11px] leading-5 text-forensics-muted">
                  {isEmulation
                    ? t('fileBrowser.mount.cowDescription')
                    : t('fileBrowser.mount.readOnlyDescription')}
                </p>
              </div>
            </section>
          </div>

          {selectedMount ? (
            <div className="flex flex-wrap items-center justify-between gap-3 border border-forensics-primary-blue/40 bg-forensics-sakura-100/20 p-3">
              <div className="flex min-w-0 items-center gap-2 text-[11px]">
                <CircleCheck className="size-4 shrink-0 text-forensics-success-text" />
                <span className="truncate text-forensics-text">
                  {t('fileBrowser.mount.activeMount', {
                    mountPoint: selectedMount.target.mountPoint,
                    state: stateLabel(selectedMount.state, t),
                  })}
                </span>
              </div>
              <Button
                type="button"
                variant="forensicsOutline"
                size="xs"
                onClick={() => void model.unmount(selectedMount.target.mountId)}
                disabled={model.isSubmitting}
              >
                {model.isUnmounting ? <LoaderCircle className="animate-spin" /> : <LogOut />}
                {t('fileBrowser.mount.unmount')}
              </Button>
            </div>
          ) : null}

          {selectedEmulation ? (
            <div className="flex flex-wrap items-center justify-between gap-3 border border-forensics-primary-blue/40 bg-forensics-sakura-100/20 p-3">
              <div className="flex min-w-0 items-center gap-2 text-[11px]">
                <Disc3 className="size-4 shrink-0 text-forensics-primary-blue" />
                <span className="truncate text-forensics-text">
                  {t('fileBrowser.mount.activeEmulation', { state: selectedEmulation.state })}
                </span>
              </div>
              <Button
                type="button"
                variant="forensicsOutline"
                size="xs"
                onClick={() => void model.releaseEmulation(selectedEmulation.sessionId)}
                disabled={model.isSubmitting}
              >
                {model.isReleasingEmulation ? <LoaderCircle className="animate-spin" /> : <LogOut />}
                {t('fileBrowser.mount.releaseEmulation')}
              </Button>
            </div>
          ) : null}

          {model.dataSources.length === 0 ? (
            <div className="border border-forensics-border bg-forensics-panel p-3 text-[11px] text-forensics-muted">
              {t('fileBrowser.mount.noSources')}
            </div>
          ) : null}

          {model.error ? (
            <FieldError role="alert" className="flex items-start gap-2 border border-forensics-error-border bg-forensics-error-bg p-3">
              <TriangleAlert className="mt-0.5 size-4 shrink-0" />
              <span>{model.error}</span>
            </FieldError>
          ) : null}

          <DialogFooter>
            <Button
              type="button"
              variant="forensicsGhost"
              onClick={() => model.setDialogOpen(false)}
              disabled={model.isSubmitting}
            >
              {t('fileBrowser.mount.close')}
            </Button>
            <Button type="submit" variant="forensicsPrimary" disabled={!canSubmit}>
              {model.isMounting || model.isEmulating
                ? <LoaderCircle className="animate-spin" />
                : isEmulation ? <Play /> : <ShieldCheck />}
              {isEmulation
                ? model.isEmulating
                  ? t('fileBrowser.mount.startingEmulation')
                  : t('fileBrowser.mount.startEmulation')
                : model.isMounting
                  ? t('fileBrowser.mount.mounting')
                  : t('fileBrowser.mount.mount')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
