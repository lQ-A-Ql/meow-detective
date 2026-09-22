export function ColorSwatch({ color, className = '' }: { color: string; className?: string }) {
  return <span aria-hidden="true" className={`inline-block h-2 w-2 rounded-none ${className}`} style={{ backgroundColor: color }} />;
}
