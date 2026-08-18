import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { FolderOpen, Loader2, Play, RefreshCw } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { errorMessage } from '@/lib/errors';
import { openDialog, singleDialogPath } from '@/lib/platform/dialog';
import { usePluginActions, useRecoverWeChatKeys } from '@/features/analysis/plugin-action-hooks';
import { useRunAnalysisExtraction } from '@/features/analysis/hooks';
import type { PluginActionDescriptor, WeChatKeyRecoveryResult, WeChatRecoveredKey } from '@/types/models';

/**
 * Action id → host command wiring. The plugin only self-describes its
 * actions; the host command surface is fixed, so the only runnable action
 * today is the dump-scan key recovery behind `recover_wechat_keys`.
 */
const RECOVER_KEYS_ACTION_ID = 'recoverKeys';

/** Capability key that re-runs plugin extraction for a data source. */
const PLUGIN_RERUN_CATEGORIES = ['PluginArtifacts'];

export type PickFilePath = () => Promise<string | undefined>;

/** Default file picker: the shared Tauri dialog primitive. */
async function defaultPickFilePath(): Promise<string | undefined> {
  try {
    return singleDialogPath(await openDialog({ directory: false, multiple: false })) ?? undefined;
  } catch {
    return undefined;
  }
}

function NameList({ label, names }: { label: string; names: string[] }) {
  if (names.length === 0) return null;
  return (
    <div className="mt-1 text-[11px]">
      <span className="text-forensics-muted">{label}:</span>
      <span className="ml-1 break-all font-mono text-forensics-text-secondary">
        {names.join(', ')}
      </span>
    </div>
  );
}

function RecoveryResult({
  result,
  rerunPending,
  rerunDone,
  rerunError,
  onRerun,
}: {
  result: WeChatKeyRecoveryResult;
  rerunPending: boolean;
  rerunDone: boolean;
  rerunError: unknown;
  onRerun: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="mt-2 rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px]">
      <div className="flex flex-wrap gap-3 font-mono text-forensics-text-secondary">
        <span>
          {t('pluginModule.actions.candidates')}: {result.candidatesSeen}
        </span>
        <span>
          {t('pluginModule.actions.recovered')}: {result.recoveredCount}
        </span>
      </div>
      <NameList label={t('pluginModule.actions.matched')} names={result.matchedDbNames} />
      <NameList label={t('pluginModule.actions.unmatched')} names={result.unmatchedDbNames} />
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <span className="text-forensics-muted">{t('pluginModule.actions.rerunHint')}</span>
        <Button
          type="button"
          variant="forensicsOutline"
          size="xs"
          disabled={rerunPending}
          onClick={onRerun}
        >
          {rerunPending ? <Loader2 className="animate-spin" size={12} /> : <RefreshCw size={12} />}
          {t('pluginModule.actions.rerun')}
        </Button>
        {rerunDone && !rerunPending ? (
          <span className="text-forensics-success-text">{t('pluginModule.actions.rerunDone')}</span>
        ) : null}
      </div>
      {rerunError ? (
        <div className="mt-1 break-words text-forensics-error-text">{errorMessage(rerunError)}</div>
      ) : null}
    </div>
  );
}

function PluginActionCard({
  dataSourceId,
  action,
  pickFilePath,
  onRecoveredKeys,
}: {
  dataSourceId: string;
  action: PluginActionDescriptor;
  pickFilePath: PickFilePath;
  onRecoveredKeys: (keys: WeChatRecoveredKey[]) => void;
}) {
  const { t } = useTranslation();
  const [filePath, setFilePath] = useState('');
  const [inputError, setInputError] = useState('');
  const recovery = useRecoverWeChatKeys();
  const rerun = useRunAnalysisExtraction();

  const supported = action.id === RECOVER_KEYS_ACTION_ID;
  const needsFile = action.inputKind === 'file';
  const running = recovery.isPending;

  async function handlePickFile() {
    const selected = await pickFilePath();
    if (selected) {
      setFilePath(selected);
      setInputError('');
    }
  }

  async function handleRun() {
    if (!supported) return;
    const trimmed = filePath.trim();
    if (needsFile && !trimmed) {
      setInputError(t('pluginModule.actions.fileRequired'));
      return;
    }
    setInputError('');
    rerun.reset();
    const result = await recovery
      .mutateAsync({ dataSourceId, dumpPath: trimmed })
      .catch(() => undefined);
    if (result) onRecoveredKeys(result.recoveredKeys);
  }

  async function handleRerun() {
    await rerun
      .mutateAsync({ dataSourceId, categories: PLUGIN_RERUN_CATEGORIES })
      .catch(() => undefined);
  }

  return (
    <div className="rounded-none border border-forensics-border bg-forensics-panel px-3 py-2">
      <div className="text-[12px] font-light text-forensics-text">{action.label}</div>
      {action.description ? (
        <div className="mt-0.5 text-[11px] leading-5 text-forensics-muted">{action.description}</div>
      ) : null}
      {needsFile ? (
        <div className="mt-2 flex items-center gap-2">
          <Input
            variant="path"
            inputSize="compact"
            value={filePath}
            disabled={running}
            placeholder={t('pluginModule.actions.filePlaceholder')}
            aria-label={t('pluginModule.actions.filePlaceholder')}
            onChange={(event) => {
              setFilePath(event.target.value);
              setInputError('');
            }}
          />
          <Button
            type="button"
            variant="forensicsOutline"
            size="xs"
            disabled={running}
            onClick={() => {
              void handlePickFile();
            }}
          >
            <FolderOpen size={12} />
            {t('pluginModule.actions.selectFile')}
          </Button>
        </div>
      ) : null}
      <div className="mt-2 flex items-center gap-2">
        <Button
          type="button"
          variant="forensicsPrimary"
          size="xs"
          disabled={!supported || running}
          onClick={() => {
            void handleRun();
          }}
        >
          {running ? <Loader2 className="animate-spin" size={12} /> : <Play size={12} />}
          {running ? t('pluginModule.actions.running') : t('pluginModule.actions.run')}
        </Button>
        {!supported ? (
          <span className="text-[11px] text-forensics-muted">
            {t('pluginModule.actions.unsupported')}
          </span>
        ) : null}
      </div>
      {inputError ? (
        <div className="mt-1 text-[11px] text-forensics-error-text">{inputError}</div>
      ) : null}
      {recovery.error ? (
        <div className="mt-1 break-words text-[11px] text-forensics-error-text">
          {errorMessage(recovery.error)}
        </div>
      ) : null}
      {recovery.data ? (
        <RecoveryResult
          result={recovery.data}
          rerunPending={rerun.isPending}
          rerunDone={rerun.isSuccess}
          rerunError={rerun.error}
          onRerun={() => {
            void handleRerun();
          }}
        />
      ) : null}
    </div>
  );
}

export interface PluginActionsSectionProps {
  dataSourceId: string;
  pluginId: string;
  /** Injectable for tests; defaults to the shared Tauri open dialog. */
  pickFilePath?: PickFilePath;
  onRecoveredKeys?: (keys: WeChatRecoveredKey[]) => void;
}

/**
 * "Plugin actions" block of a plugin module panel. Renders nothing when the
 * plugin declares no actions (or the descriptor query has not resolved yet).
 */
export function PluginActionsSection({
  dataSourceId,
  pluginId,
  pickFilePath = defaultPickFilePath,
  onRecoveredKeys = () => undefined,
}: PluginActionsSectionProps) {
  const { t } = useTranslation();
  const actionsQuery = usePluginActions(pluginId);
  const actions = actionsQuery.data ?? [];

  if (actions.length === 0) {
    return null;
  }

  return (
    <section>
      <div className="mb-2 text-[12px] font-light text-forensics-text">
        {t('pluginModule.actions.title')}
      </div>
      <div className="space-y-2">
        {actions.map((action) => (
          <PluginActionCard
            key={action.id}
            dataSourceId={dataSourceId}
            action={action}
            pickFilePath={pickFilePath}
            onRecoveredKeys={onRecoveredKeys}
          />
        ))}
      </div>
    </section>
  );
}
