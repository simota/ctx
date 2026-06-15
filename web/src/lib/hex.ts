/**
 * Pure hex-dump formatting utilities (no DOM dependencies).
 *
 * Classic hex dump layout:
 *   00000000  de ad be ef 00 11 22 33  44 55 66 77 88 99 aa bb  |....."3DUfw...|
 *   offset    ←── group 1 (8 bytes) ──→  ←── group 2 (8 bytes) ──→  ASCII gutter
 */

const HEX_CHARS = '0123456789abcdef';

/** Format a byte as a lowercase 2-char hex string ("00"–"ff"). */
export function toHex2(b: number): string {
  return HEX_CHARS[(b >>> 4) & 0xf] + HEX_CHARS[b & 0xf];
}

/** Format a 32-bit offset as an 8-char lowercase hex string. */
export function toOffset8(offset: number): string {
  let s = (offset >>> 0).toString(16);
  while (s.length < 8) s = '0' + s;
  return s;
}

/** Return the ASCII gutter char for a byte: printable 0x20–0x7E → literal, else '.'. */
export function asciiChar(b: number): string {
  return b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : '.';
}

export interface HexLine {
  /** 8-char lowercase hex offset, e.g. "00000010" */
  offset: string;
  /** hex body: 16 bytes split into two 8-byte groups, padded on final short row */
  hex: string;
  /** ASCII gutter (16 chars, non-printable replaced with '.'). aria-hidden. */
  ascii: string;
}

/**
 * Format a single hex dump row.
 *
 * @param bytes  The full file buffer.
 * @param offset Byte offset of this row's first byte.
 * @returns      A structured HexLine object.
 */
export function formatHexLine(bytes: Uint8Array, offset: number): HexLine {
  const BYTES_PER_ROW = 16;
  const end = Math.min(offset + BYTES_PER_ROW, bytes.length);
  const count = end - offset;

  // Build hex and ascii
  let group1 = '';
  let group2 = '';
  let ascii = '';

  for (let i = 0; i < BYTES_PER_ROW; i++) {
    if (i < count) {
      const b = bytes[offset + i];
      const h = toHex2(b);
      if (i < 8) {
        group1 += i === 0 ? h : ' ' + h;
      } else {
        group2 += i === 8 ? h : ' ' + h;
      }
      ascii += asciiChar(b);
    } else {
      // Padding for final short row
      if (i < 8) {
        group1 += i === 0 ? '  ' : '   ';
      } else {
        group2 += i === 8 ? '  ' : '   ';
      }
      ascii += ' ';
    }
  }

  // Combine groups with separator (2-space gap between groups)
  const hex = group1 + '  ' + group2;

  return {
    offset: toOffset8(offset),
    hex,
    ascii,
  };
}

/** Total number of rows for a given byte length. */
export function rowCount(byteLength: number): number {
  if (byteLength === 0) return 0;
  return Math.ceil(byteLength / 16);
}
