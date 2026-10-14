/** Only buffer text that could still become a delimiter when the next chunk arrives. */
export function pendingMarkerLength(text: string, marker: string): number {
  for (let length = Math.min(text.length, marker.length - 1); length > 0; length--) {
    if (text.endsWith(marker.slice(0, length))) return length;
  }
  return 0;
}
