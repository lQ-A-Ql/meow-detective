import { Avatar, AvatarFallback } from '@/app/components/ui/avatar';

function initials(name: string): string {
  const normalized = name.trim();
  if (!normalized) return '?';
  return [...normalized].slice(0, 2).join('').toLocaleUpperCase();
}

export function WeChatIdentity({ name, large = false }: { name: string; large?: boolean }) {
  return (
    <Avatar className={large ? 'size-10' : 'size-8'}>
      <AvatarFallback className={large ? 'text-[12px]' : undefined}>
        {initials(name)}
      </AvatarFallback>
    </Avatar>
  );
}
