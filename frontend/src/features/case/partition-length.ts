export function partitionLengthLabel(length: number, unknownLabel: string): string | number {
  return length > 0 ? length : unknownLabel;
}
