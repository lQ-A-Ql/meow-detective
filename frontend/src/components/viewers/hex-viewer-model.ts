export const BYTES_PER_ROW = 16;

export interface HexParsedLine {
  offset: string;
  offsetValue: number;
  bytes: number[];
  fallbackHex?: string;
}

export function formatOffset(offset: number) {
  return offset.toString(16).toUpperCase().padStart(8, '0');
}

export function formatByte(byte: number) {
  return byte.toString(16).toUpperCase().padStart(2, '0');
}

export function formatAsciiByte(byte: number) {
  return byte >= 32 && byte <= 126 ? String.fromCharCode(byte) : '.';
}

export function formatByteWindowLine(
  rawBytes: number[],
  rowIndex: number,
  baseOffset: number,
): HexParsedLine {
  const rowStart = rowIndex * BYTES_PER_ROW;
  return {
    offset: formatOffset(baseOffset + rowStart),
    offsetValue: baseOffset + rowStart,
    bytes: rawBytes.slice(rowStart, rowStart + BYTES_PER_ROW),
  };
}

export function parseHexLine(line: string): HexParsedLine {
  const match = line.match(/^\s*([0-9A-Fa-f]{1,16})(?:\s{2,}|:\s*)(.*)$/);
  if (match) {
    const bytes: number[] = [];
    for (const token of match[2].trim().split(/\s+/)) {
      if (!/^[0-9A-Fa-f]{2}$/.test(token) || bytes.length === BYTES_PER_ROW) break;
      bytes.push(Number.parseInt(token, 16));
    }
    if (bytes.length > 0) {
      return {
        offset: match[1].toUpperCase(),
        offsetValue: Number.parseInt(match[1], 16),
        bytes,
      };
    }
  }
  return { offset: '', offsetValue: 0, bytes: [], fallbackHex: line };
}
