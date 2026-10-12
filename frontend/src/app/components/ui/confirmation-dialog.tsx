import type { ReactNode } from 'react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './dialog';

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
}: ConfirmationDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <IconButton
            variant="forensicsOutline"
            onClick={() => onOpenChange(false)}
            disabled={pending}
            label={cancelLabel}
          >
            <ActionIcon name="close" />
          </IconButton>
          <IconButton variant={destructive ? 'destructive' : 'solid'} onClick={onConfirm} disabled={pending} label={confirmLabel}><ActionIcon name="check" /></IconButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
