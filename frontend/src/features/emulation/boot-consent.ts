export function confirmEmulationBoot(
  recoveryIsoPath: string,
  confirmationMessage: string,
  confirm: (message: string) => boolean = window.confirm,
): boolean {
  return recoveryIsoPath.length > 0 || confirm(confirmationMessage);
}
