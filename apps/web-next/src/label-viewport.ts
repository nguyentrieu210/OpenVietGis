// Screen-space fallback for polygons whose original label anchor leaves the viewport.
export type ScreenPoint = { x: number; y: number };

export function visiblePolygonAnchor(
  rings: readonly (readonly ScreenPoint[])[],
  width: number, height: number, padX: number, padY: number,
): ScreenPoint | null {
  const outer = rings[0];
  if (!outer?.length || width < 2 * padX || height < 2 * padY) return null;
  let minY = Infinity, maxY = -Infinity;
  for (const p of outer) { minY = Math.min(minY, p.y); maxY = Math.max(maxY, p.y); }
  const top = Math.max(padY, minY), bottom = Math.min(height - padY, maxY);
  if (top >= bottom) return null;
  let best: ScreenPoint | null = null, bestWidth = 0;
  for (const f of [.5, .25, .75, .125, .375, .625, .875, .0625, .9375]) {
    const y = top + (bottom - top) * f, crossings: number[] = [];
    for (const ring of rings) {
      for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
        const a = ring[j]!, b = ring[i]!;
        if ((a.y > y) !== (b.y > y)) crossings.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
      }
    }
    crossings.sort((a, b) => a - b);
    for (let i = 0; i + 1 < crossings.length; i += 2) {
      const left = Math.max(padX, crossings[i]!), right = Math.min(width - padX, crossings[i + 1]!);
      if (right - left > bestWidth) { bestWidth = right - left; best = {x: (left + right) / 2, y}; }
    }
  }
  return best;
}
