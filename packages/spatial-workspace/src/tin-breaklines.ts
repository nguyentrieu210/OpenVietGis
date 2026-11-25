type XY = readonly [number, number, ...number[]];
export type BreaklineEdge = readonly [number, number];

export const edgeKey = (a: number, b: number) => a < b ? a + ":" + b : b + ":" + a;

export function constrainTIN(points: readonly XY[], input: Uint32Array, constraints: readonly BreaklineEdge[]) {
    if (!constraints.length) return {triangles: input, locked: new Set<string>()};
    const triangles = input.slice();
    const locked = new Set<string>();
    const edges = new Map<string, Set<number>>();
    const orient = (a: number, b: number, c: number) => {
        const [x, y] = points[a]!, [u, v] = points[b]!, [s, t] = points[c]!;
        return (u - x) * (t - y) - (v - y) * (s - x);
    };
    const crosses = (a: number, b: number, c: number, d: number) =>
        orient(a, b, c) * orient(a, b, d) < 0 && orient(c, d, a) * orient(c, d, b) < 0;
    function indexTriangle(offset: number, add: boolean) {
        for (let j = 0; j < 3; j++) {
            const key = edgeKey(triangles[offset + j]!, triangles[offset + (j + 1) % 3]!);
            const adjacent = edges.get(key) ?? new Set<number>();
            if (add) {
                adjacent.add(offset);
                edges.set(key, adjacent);
            } else {
                adjacent.delete(offset);
                if (!adjacent.size) edges.delete(key);
            }
        }
    }
    for (let i = 0; i < triangles.length; i += 3) indexTriangle(i, true);

    // Points sorted by x: each segment only scans its own bounding box, not every point.
    const byX = Array.from({length: points.length}, (_, i) => i).sort((a, b) => points[a]![0] - points[b]![0]);
    const firstAtOrAfter = (value: number) => {
        let lo = 0, hi = byX.length;
        while (lo < hi) {
            const mid = (lo + hi) >> 1;
            if (points[byX[mid]!]![0] < value) lo = mid + 1; else hi = mid;
        }
        return lo;
    };
    let effort = 0;
    for (const [start, end] of constraints) {
        if (start === end) continue;
        const [x, y, z] = points[start]!, [u, v, w] = points[end]!;
        const length = Math.hypot(u - x, v - y);
        const vertices = [{id: start, fraction: 0}, {id: end, fraction: 1}];
        const pad = 1e-6, maxX = Math.max(x, u) + pad, minY = Math.min(y, v) - pad, maxY = Math.max(y, v) + pad;
        for (let k = firstAtOrAfter(Math.min(x, u) - pad); k < byX.length; k++) {
            const id = byX[k]!;
            const [px, py, pz] = points[id]!;
            if (px > maxX) break;
            if (++effort > 8_000_000) throw Error("Breakline quá phức tạp; hãy chia khu vực.");
            if (py < minY || py > maxY || id === start || id === end || Math.abs(orient(start, end, id)) > length * 1e-8) continue;
            const fraction = ((px - x) * (u - x) + (py - y) * (v - y)) / (length * length);
            if (fraction <= 0 || fraction >= 1) continue;
            if (z !== undefined && w !== undefined && pz !== undefined && Math.abs(pz - (z + fraction * (w - z))) > .001)
                throw Error("Điểm nằm trên breakline có cao độ không khớp.");
            vertices.push({id, fraction});
        }
        vertices.sort((a, b) => a.fraction - b.fraction);
        for (let k = 1; k < vertices.length; k++) {
            const a = vertices[k - 1]!.id, b = vertices[k]!.id, target = edgeKey(a, b);
            const flipped = new Set<string>();
            while (!edges.has(target)) {
                let changed = false;
                for (const [key, adjacent] of edges) {
                    if (++effort > 8_000_000) throw Error("Breakline quá phức tạp; hãy chia khu vực.");
                    const [c, d] = key.split(":").map(Number) as [number, number];
                    if (!crosses(a, b, c, d)) continue;
                    if (locked.has(key)) throw Error("Breakline giao nhau; hãy tách tại nút giao và thống nhất cao độ.");
                    if (adjacent.size !== 2 || flipped.has(key)) continue;
                    const [i, j] = [...adjacent] as [number, number];
                    const opposite = (offset: number) => triangles.slice(offset, offset + 3).find(id => id !== c && id !== d)!;
                    const e = opposite(i), f = opposite(j);
                    if (!crosses(c, d, e, f)) continue;
                    indexTriangle(i, false);
                    indexTriangle(j, false);
                    triangles.set([e, f, c], i);
                    triangles.set([f, e, d], j);
                    indexTriangle(i, true);
                    indexTriangle(j, true);
                    flipped.add(key);
                    changed = true;
                    break;
                }
                if (!changed) throw Error("Không chèn được breakline vào TIN; kiểm tra hình học nguồn.");
            }
            locked.add(target);
        }
    }
    return {triangles, locked};
}
