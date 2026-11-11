import type { IconComponent } from '@/components/icons';

export function TokenIcon({ icon: Icon, color, size = 16 }: { icon: IconComponent; color: string; size?: number }) {
  return <Icon size={size} style={{ color }} />;
}
