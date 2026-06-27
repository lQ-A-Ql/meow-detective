/**
 * Parse hex/decimal offset input from user.
 *
 * Supports multiple formats:
 * - Hex with 0x prefix: "0x1234", "0xABCD"
 * - Intel hex suffix: "1234h", "ABCDh"
 * - Decimal: "1234"
 * - Bare hex: "ABCD" (detected if contains [a-fA-F])
 *
 * @param input - User input string
 * @returns Parsed offset as non-negative integer, or null if invalid
 */
export function parseOffsetInput(input: string): number | null {
  const trimmed = input.trim();
  if (!trimmed) {
    return null;
  }

  // Hex with 0x prefix
  if (/^0x/i.test(trimmed)) {
    const parsed = Number.parseInt(trimmed.slice(2), 16);
    return Number.isNaN(parsed) || parsed < 0 ? null : parsed;
  }

  // Intel hex suffix (1234h)
  if (/^[0-9a-f]+h$/i.test(trimmed)) {
    const parsed = Number.parseInt(trimmed.slice(0, -1), 16);
    return Number.isNaN(parsed) || parsed < 0 ? null : parsed;
  }

  // Pure decimal (digits only)
  if (/^[0-9]+$/.test(trimmed)) {
    const parsed = Number.parseInt(trimmed, 10);
    return Number.isNaN(parsed) || parsed < 0 ? null : parsed;
  }

  // Bare hex (contains a-f, no prefix/suffix)
  if (/^[0-9a-f]+$/i.test(trimmed)) {
    const parsed = Number.parseInt(trimmed, 16);
    return Number.isNaN(parsed) || parsed < 0 ? null : parsed;
  }

  return null;
}
