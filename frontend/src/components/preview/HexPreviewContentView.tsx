import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { Tooltip } from '@/app/components/ui/tooltip';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { HexViewer } from '@/components/viewers/HexViewer';
import type { useHexPreviewModel } from '@/hooks/use-hex-preview-model';
import { formatBytes } from '@/lib/format-bytes';
export function HexPreviewContentView({ model }: { model: ReturnType<typeof useHexPreviewModel> }) {
 const { t } = useTranslation();
 const { viewer, onHexJumpInputChange, onHexJump, inspectorExpanded, handleNeedMoreRange, toggleInspector } = model;
  if (!viewer) {
    return <div className="text-forensics-muted">{t('viewer.hex.empty')}</div>;
  }
  if (viewer.rawBytes.length === 0 && viewer.lines.length === 0) {
    // A selected file with no displayable bytes is either genuinely empty or its
    // window carried nothing; saying "select a file" here hid the real state.
    return (
      <div className="text-forensics-muted">
        {viewer.fileSize === 0 ? t('viewer.hex.emptyFile') : t('viewer.hex.emptyRange')}
      </div>
    );
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="min-h-0 flex-1">
        <HexViewer
          lines={viewer.lines}
          rawBytes={viewer.rawBytes}
          baseOffset={viewer.baseOffset}
          fileSize={viewer.fileSize}
          activeOffset={viewer.activeOffset}
          loadedRanges={viewer.loadedRanges}
          onNeedMoreRange={handleNeedMoreRange}
        />
      </div>
      {viewer.error ? (
        <div className="mt-2 shrink-0 text-[10px] text-forensics-error-text">{viewer.error}</div>
      ) : null}
      <div className="mt-2 shrink-0 border border-forensics-border bg-forensics-panel">
        <Button
          type="button"
          variant="forensicsGhost"
          shape="row"
          aria-expanded={inspectorExpanded}
          onClick={toggleInspector}
          className="h-auto w-full justify-between gap-3 rounded-none px-3 py-2 text-left text-[10px]"
        >
          <div className="flex min-w-0 items-center gap-3">
            <span className="font-light text-forensics-text">
              {viewer.mode === 'full' ? t('preview.hex.fullMode') : t('preview.hex.chunkedMode')}
            </span>
            <span>
              {t('preview.hex.loaded')}{' '}
              {formatBytes(
                viewer.loadedRanges.reduce((total, range) => total + (range.end - range.start), 0),
              )}{' '}
              / {formatBytes(viewer.fileSize)}
            </span>
            <span>offset={viewer.activeOffset.toString(16).toUpperCase().padStart(8, '0')}</span>
          </div>
          {inspectorExpanded ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
        </Button>
        {inspectorExpanded ? (
          <div className="border-t border-forensics-border px-3 py-2 text-[10px] text-forensics-muted">
            <div className="flex items-center gap-2">
              <Input
                value={viewer.jumpOffsetInput}
                onChange={(event) => onHexJumpInputChange(event.target.value)}
                variant="mono"
                inputSize="inline"
                className="w-32"
                placeholder="0x0"
              />
              <Tooltip content={t('preview.hex.jump')}><Button aria-label={t('preview.hex.jump')}
                type="button"
                variant="forensicsOutline"
                size="compact"
                onClick={() => {
                  void onHexJump();
                }}
              ><ActionIcon name="chevron-left" /></Button></Tooltip>
            </div>
            <div className="mt-2">
              {viewer.mode === 'chunked'
                ? t('preview.hex.chunkedHint')
                : t('preview.hex.fullHint')}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
