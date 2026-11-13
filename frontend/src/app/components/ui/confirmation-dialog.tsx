import type { ReactNode } from 'react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Button } from './button';
import { Tooltip } from './tooltip';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './dialog';

export interface ConfirmationDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: ReactNode;
  cancelLabel: string;
  confirmLabel: string;
  onConfirm: () => void;
  pending?: boolean;
  destructive?: boolean;
  actionShape?: 'circle' | 'pill';
}

/** Shared confirmation primitive for irreversible case operations. */
export function ConfirmationDialog({
  open,
  onOpenChange,
  title,
  description,
  cancelLabel,
  confirmLabel,
  onConfirm,
  pending = false,
  destructive = false,
  actionShape = 'circle',
}: ConfirmationDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Tooltip content={cancelLabel}>
            <Button
              type="button"
              shape={actionShape}
              variant={actionShape === 'pill' ? 'solid' : 'forensicsOutline'}
              onClick={() => onOpenChange(false)}
              disabled={pending}
              aria-label={cancelLabel}
            >
              <ActionIcon name="close" />
              {actionShape === 'pill' ? cancelLabel : null}
            </Button>
          </Tooltip>
          <Tooltip content={confirmLabel}>
            <Button
              type="button"
              shape={actionShape}
              variant={destructive && actionShape === 'circle' ? 'destructive' : 'solid'}
              onClick={onConfirm}
              disabled={pending}
              aria-label={confirmLabel}
            >
              <ActionIcon name="check" />
              {actionShape === 'pill' ? confirmLabel : null}
            </Button>
          </Tooltip>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
