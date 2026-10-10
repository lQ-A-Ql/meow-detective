import type { ReactNode } from 'react';
import { Check, X } from 'lucide-react';
import { Button } from './button';
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
          <Button
            type="button"
            variant="forensicsOutline"
            onClick={() => onOpenChange(false)}
            disabled={pending}
            aria-label={cancelLabel}
          >
            <X size={15} />
          </Button>
          <Button type="button" variant={destructive ? 'destructive' : 'forensicsPrimary'} onClick={onConfirm} disabled={pending} aria-label={confirmLabel}><Check size={15} /></Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
