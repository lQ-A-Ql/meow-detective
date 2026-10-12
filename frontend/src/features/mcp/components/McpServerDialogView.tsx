import { Checkbox } from '@/app/components/ui/checkbox';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/app/components/ui/dialog';
import { Field, FieldLabel } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import type { TFunction } from 'i18next';
import type { McpServerDialogProps } from '../model/mcp-server-dialog';
import type { ReturnTypeModel } from '../containers/McpServerDialogController';

export function McpServerDialogView({ t, model, props }: { t: TFunction; model: ReturnTypeModel; props: McpServerDialogProps }) {
  const set = model.updatePermission;
  return (
    <Dialog open onOpenChange={(open) => { if (!open) props.onClose(); }}>
      <DialogContent aria-describedby={undefined} className="flex max-h-[min(760px,calc(100dvh-48px))] min-h-0 flex-col sm:max-w-[540px]">
        <DialogHeader><DialogTitle>{t('mcpUi.dialog.title')}</DialogTitle></DialogHeader>
        <ScrollArea className="min-h-0 flex-1" viewportClassName="pr-2">
          <div className="space-y-5">
            <Field>
              <FieldLabel htmlFor="mcp-server-name">{t('mcpUi.dialog.name')} *</FieldLabel>
              <Input id="mcp-server-name" value={model.name} onChange={(event) => model.setName(event.target.value)}
                placeholder={t('mcpUi.dialog.namePlaceholder')} required />
            </Field>
            <Field>
              <FieldLabel htmlFor="mcp-server-transport">{t('mcpUi.dialog.transport')}</FieldLabel>
              <Select value={model.transportType} onValueChange={(value) => model.setTransportType(value as 'sse' | 'stdio')}>
                <SelectTrigger id="mcp-server-transport"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="sse">{t('mcpUi.dialog.http')}</SelectItem>
                  <SelectItem value="stdio">{t('mcpUi.dialog.stdio')}</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            {model.transportType === 'sse' ? <Field>
              <FieldLabel htmlFor="mcp-server-url">{t('mcpUi.dialog.url')} *</FieldLabel>
              <Input id="mcp-server-url" value={model.url} onChange={(event) => model.setUrl(event.target.value)}
                placeholder={t('mcpUi.dialog.urlPlaceholder')} variant="path" required />
            </Field> : <>
              <Field>
                <FieldLabel htmlFor="mcp-server-command">{t('mcpUi.dialog.command')} *</FieldLabel>
                <Input id="mcp-server-command" value={model.command} onChange={(event) => model.setCommand(event.target.value)}
                  placeholder={t('mcpUi.dialog.commandPlaceholder')} variant="mono" required />
              </Field>
              <Field>
                <FieldLabel htmlFor="mcp-server-arguments">{t('mcpUi.dialog.arguments')}</FieldLabel>
                <Input id="mcp-server-arguments" value={model.argsStr} onChange={(event) => model.setArgsStr(event.target.value)}
                  placeholder={t('mcpUi.dialog.argumentsPlaceholder')} variant="mono" />
              </Field>
            </>}
            <div className="flex flex-wrap gap-5 text-[12px]">
              <label className="flex items-center gap-2 text-[12px]">
                <Checkbox checked={model.enabled} onCheckedChange={(value) => model.setEnabled(value === true)} />
                {t('mcpUi.dialog.enabled')}
              </label>
              <label className="flex items-center gap-2 text-[12px]">
                <Checkbox checked={model.autoConnect} onCheckedChange={(value) => model.setAutoConnect(value === true)} />
                {t('mcpUi.dialog.autoConnect')}
              </label>
            </div>
            <div className="space-y-4 pt-2">
              <h3 className="text-[12px] text-forensics-ink">{t('mcpUi.dialog.permissions')}</h3>
              <PermissionSelect t={t} label="mcpUi.dialog.resourceAccess" value={model.permissions.resourceAccess}
                options={[['readOnly', 'mcpUi.dialog.readOnly'], ['disabled', 'mcpUi.dialog.disabled']]}
                onChange={(value) => set('resourceAccess', value as 'readOnly' | 'disabled')} />
              <PermissionSelect t={t} label="mcpUi.dialog.promptAccess" value={model.permissions.promptAccess}
                options={[['readOnly', 'mcpUi.dialog.readOnly'], ['disabled', 'mcpUi.dialog.disabled']]}
                onChange={(value) => set('promptAccess', value as 'readOnly' | 'disabled')} />
              <PermissionSelect t={t} label="mcpUi.dialog.toolAccess" value={model.permissions.toolAccess}
                options={[['allowAll', 'mcpUi.dialog.allowAll'], ['disabled', 'mcpUi.dialog.disabled'], ['allowList', 'mcpUi.dialog.allowList']]}
                onChange={(value) => set('toolAccess', value as 'allowAll' | 'disabled' | 'allowList')} />
              {model.permissions.toolAccess === 'allowList' ? <Field>
                <FieldLabel htmlFor="mcp-server-allow-list">{t('mcpUi.dialog.allowListLabel')}</FieldLabel>
                <Input id="mcp-server-allow-list" value={model.permissions.allowedTools.join(', ')}
                  onChange={(event) => set('allowedTools', event.target.value.split(/[\s,]+/).filter(Boolean))}
                  placeholder={t('mcpUi.dialog.allowListPlaceholder')} variant="mono" />
              </Field> : null}
              <PermissionSelect t={t} label="mcpUi.dialog.networkPolicy" value={model.permissions.networkPolicy}
                options={[['localhostOnly', 'mcpUi.dialog.localhost'], ['privateLanAllowed', 'mcpUi.dialog.privateLan'], ['anyHost', 'mcpUi.dialog.anyHost']]}
                onChange={(value) => set('networkPolicy', value as 'localhostOnly' | 'privateLanAllowed' | 'anyHost')} />
            </div>
            {model.testResult ? <div role="status" className="text-[12px] text-forensics-muted">
              {t(model.testResult.success ? 'mcpUi.dialog.testSuccess' : 'mcpUi.dialog.testFailure')}
              {model.testResult.error ? <div className="mt-1 text-[11px] text-forensics-error-text">{model.testResult.error}</div> : null}
            </div> : null}
            {model.error ? <div role="alert" className="text-[12px] text-forensics-error-text">{t(model.error)}</div> : null}
          </div>
        </ScrollArea>
        <DialogFooter>
          <IconButton label={t(model.testing ? 'mcpUi.dialog.testing' : 'mcpUi.dialog.test')}
            onClick={() => void model.test()} disabled={model.testing}>
            <ActionIcon name="plug" />
          </IconButton>
          <IconButton label={t('mcpUi.dialog.cancel')} onClick={props.onClose}><ActionIcon name="close" /></IconButton>
          <IconButton label={t(model.saving ? 'mcpUi.dialog.saving' : 'mcpUi.dialog.add')} variant="solid"
            onClick={() => void model.save(props.onAdd, props.onClose)} disabled={model.saving}>
            <ActionIcon name="plus" />
          </IconButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function PermissionSelect({ t, label, value, options, onChange }: {
  t: TFunction; label: string; value: string; options: [string, string][]; onChange: (value: string) => void;
}) {
  return <Field layout="row">
    <FieldLabel htmlFor={label}>{t(label)}</FieldLabel>
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger id={label} size="sm"><SelectValue /></SelectTrigger>
      <SelectContent>{options.map(([option, key]) => <SelectItem key={option} value={option}>{t(key)}</SelectItem>)}</SelectContent>
    </Select>
  </Field>;
}
