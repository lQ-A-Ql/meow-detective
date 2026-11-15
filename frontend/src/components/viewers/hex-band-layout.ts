export interface HexBandGeometry {
  leftCh: number;
  widthCh: number;
}

const GROUP_BREAK_BYTE = 8;

function crossesGroupBreak(startIndex: number, count: number) {
  return startIndex < GROUP_BREAK_BYTE && startIndex + count > GROUP_BREAK_BYTE;
}

export function hexBandGeometry(startIndex: number, count: number): HexBandGeometry {
  if (count <= 0) return { leftCh: 0, widthCh: 0 };
  return {
    leftCh: startIndex * 3 + (startIndex >= GROUP_BREAK_BYTE ? 1 : 0),
    widthCh: count * 3 - 1 + (crossesGroupBreak(startIndex, count) ? 1 : 0),
  };
}

export function asciiBandGeometry(startIndex: number, count: number): HexBandGeometry {
  if (count <= 0) return { leftCh: 0, widthCh: 0 };
  return {
    leftCh: startIndex + (startIndex >= GROUP_BREAK_BYTE ? 1 : 0),
    widthCh: count + (crossesGroupBreak(startIndex, count) ? 1 : 0),
  };
}
