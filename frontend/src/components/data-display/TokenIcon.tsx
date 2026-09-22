import type { LucideIcon } from 'lucide-react';

export function TokenIcon({ icon: Icon, color, size = 16 }: { icon: LucideIcon; color: string; size?: number }) {
  return <Icon size={size} style={{ color }} />;
}
