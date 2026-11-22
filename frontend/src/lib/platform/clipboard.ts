export async function writeClipboardText(value: string): Promise<void> {
  if (!navigator.clipboard?.writeText) {
    throw new Error('clipboard unavailable');
  }
  return navigator.clipboard.writeText(value);
}
