import { useState } from "react";
import type { Bounds, Feature, Geometry, Point } from "./offline-store";

export function OfflineMap({ bounds, features, selection, select }: {
  bounds: Bounds; features: Feature[]; selection: string; select: (id: string) => void;
}) {
  const [view, setView] = useState(bounds);
  const [visible, setVisible] = useState(true);
  const width = view.max_x - view.min_x, height = view.max_y - view.min_y;
  function move(dx: number, dy: number) {
    setView(v => ({ min_x: v.min_x + dx * width, max_x: v.max_x + dx * width,
      min_y: v.min_y + dy * height, max_y: v.max_y + dy * height }));
  }
  function zoom(factor: number) {
    const cx = (view.max_x + view.min_x) / 2, cy = (view.max_y + view.min_y) / 2;
    setView({ min_x: cx - width * factor / 2, max_x: cx + width * factor / 2,
      min_y: cy - height * factor / 2, max_y: cy + height * factor / 2 });
  }
  const point = ([x, y]: Point) => `${x},${-y}`;
  function shape(geometry: Geometry, color: string) {
    if (geometry.type === "Point") {
      const [x, y] = geometry.coordinates;
      return <circle cx={x} cy={-y} r={Math.min(width, height) / 80} fill={color} stroke="#fff" vectorEffect="non-scaling-stroke"/>;
    }
    if (geometry.type === "LineString") return <polyline points={geometry.coordinates.map(point).join(" ")} stroke={color} strokeWidth="3" fill="none" vectorEffect="non-scaling-stroke"/>;
    return <path d={geometry.coordinates.map(ring => "M" + ring.map(point).join(" L") + " Z").join(" ")}
      stroke={color} strokeWidth="1.5" fill={color} fillOpacity=".25" fillRule="evenodd" vectorEffect="non-scaling-stroke"/>;
  }
  return <section>
    <h3>Bản đồ gói đã tải (CRS gốc, không nền trực tuyến)</h3>
    <div className="offline-controls">
      <button type="button" onClick={() => zoom(.5)}>Phóng to +</button>
      <button type="button" onClick={() => zoom(2)}>Thu nhỏ −</button>
      <button type="button" onClick={() => move(-.25, 0)}>←</button>
      <button type="button" onClick={() => move(.25, 0)}>→</button>
      <button type="button" onClick={() => move(0, .25)}>↑</button>
      <button type="button" onClick={() => move(0, -.25)}>↓</button>
      <button type="button" onClick={() => setView(bounds)}>Toàn vùng</button>
      <label><input type="checkbox" checked={visible} onChange={e => setVisible(e.target.checked)}/> Lớp đã tải</label>
    </div>
    <svg role="img" aria-label="Bản đồ đối tượng gói ngoại tuyến" data-testid="offline-map"
      viewBox={`${view.min_x} ${-view.max_y} ${width} ${height}`} className="offline-map">
      {visible && features.map(f => <g key={f.feature_id} onClick={() => select(f.feature_id)} style={{ cursor: "pointer" }}>
        <title>{f.feature_id}</title>{shape(f.geometry, selection === f.feature_id ? "#c2410c" : "#166534")}
      </g>)}
    </svg>
  </section>;
}
