import { describe, expect, it } from 'vitest';
import { parseOffsetInput } from './hex-offset-parser';

describe('parseOffsetInput', () => {
  describe('hex with 0x prefix', () => {
    it('parses lowercase hex', () => {
      expect(parseOffsetInput('0x1234')).toBe(0x1234);
      expect(parseOffsetInput('0xabcd')).toBe(0xabcd);
    });

    it('parses uppercase hex', () => {
      expect(parseOffsetInput('0X1234')).toBe(0x1234);
      expect(parseOffsetInput('0XABCD')).toBe(0xabcd);
    });

    it('parses mixed case hex', () => {
      expect(parseOffsetInput('0xAbCd')).toBe(0xabcd);
    });

    it('handles large values', () => {
      expect(parseOffsetInput('0xFFFFFFFF')).toBe(0xffffffff);
    });

    it('rejects negative values', () => {
      expect(parseOffsetInput('0x-1234')).toBeNull();
    });

    it('rejects invalid hex digits', () => {
      expect(parseOffsetInput('0xGHIJ')).toBeNull();
    });
  });

  describe('Intel hex suffix (h)', () => {
    it('parses lowercase hex with h suffix', () => {
      expect(parseOffsetInput('1234h')).toBe(0x1234);
      expect(parseOffsetInput('abcdh')).toBe(0xabcd);
    });

    it('parses uppercase hex with H suffix', () => {
      expect(parseOffsetInput('1234H')).toBe(0x1234);
      expect(parseOffsetInput('ABCDH')).toBe(0xabcd);
    });

    it('parses mixed case', () => {
      expect(parseOffsetInput('AbCdH')).toBe(0xabcd);
    });

    it('rejects invalid format', () => {
      expect(parseOffsetInput('GHIJh')).toBeNull();
      expect(parseOffsetInput('12 34h')).toBeNull();
    });
  });

  describe('decimal', () => {
    it('parses pure decimal numbers', () => {
      expect(parseOffsetInput('1234')).toBe(1234);
      expect(parseOffsetInput('0')).toBe(0);
      expect(parseOffsetInput('999999')).toBe(999999);
    });

    it('rejects negative decimals', () => {
      expect(parseOffsetInput('-1234')).toBeNull();
    });
  });

  describe('bare hex (no prefix/suffix)', () => {
    it('parses hex when letters present', () => {
      expect(parseOffsetInput('ABCD')).toBe(0xabcd);
      expect(parseOffsetInput('1a2b')).toBe(0x1a2b);
      expect(parseOffsetInput('DEADBEEF')).toBe(0xdeadbeef);
    });

    it('disambiguates from decimal - bare digits parsed as decimal', () => {
      // "1234" contains no [a-f], so it's parsed as decimal, not hex
      expect(parseOffsetInput('1234')).toBe(1234); // decimal
      expect(parseOffsetInput('0x1234')).toBe(0x1234); // hex explicit
    });
  });

  describe('edge cases', () => {
    it('returns null for empty string', () => {
      expect(parseOffsetInput('')).toBeNull();
    });

    it('returns null for whitespace only', () => {
      expect(parseOffsetInput('   ')).toBeNull();
      expect(parseOffsetInput('\t\n')).toBeNull();
    });

    it('trims leading/trailing whitespace', () => {
      expect(parseOffsetInput('  0x1234  ')).toBe(0x1234);
      expect(parseOffsetInput('\t1234h\n')).toBe(0x1234);
    });

    it('returns null for invalid formats', () => {
      expect(parseOffsetInput('xyz')).toBeNull();
      expect(parseOffsetInput('12.34')).toBeNull();
      expect(parseOffsetInput('0x')).toBeNull();
      expect(parseOffsetInput('h')).toBeNull();
    });

    it('handles zero correctly', () => {
      expect(parseOffsetInput('0')).toBe(0);
      expect(parseOffsetInput('0x0')).toBe(0);
      expect(parseOffsetInput('0h')).toBe(0);
    });

    it('handles maximum safe integer boundary', () => {
      expect(parseOffsetInput('9007199254740991')).toBe(9007199254740991); // Number.MAX_SAFE_INTEGER
    });
  });

  describe('real-world examples', () => {
    it('parses typical MBR offset', () => {
      expect(parseOffsetInput('0x0')).toBe(0);
    });

    it('parses typical partition offset', () => {
      expect(parseOffsetInput('0x100000')).toBe(1048576);
    });

    it('parses DOS header offset', () => {
      expect(parseOffsetInput('3Ch')).toBe(60);
    });

    it('parses file offset in forensic context', () => {
      expect(parseOffsetInput('0xDEADBEEF')).toBe(0xdeadbeef);
    });
  });
});
