import type { SpatialFeature, SpatialSource } from "@openvietgis/client-sdk";
import { useState, type MouseEvent, type PointerEvent } from "react";
import {
  buildSketch, editableVertices, fromScreen, nearestSnap, padBounds, parseSketch, pathsOf, toScreen,
  type NativeBounds, type Sketch, type SketchFamily, type SketchFeature, type SnapCandidate, type XY
} from "./editing-geometry";

export interface NativeGeometryEditorProps {
  source: SpatialSource;
  features: SpatialFeature[];
  partial: boolean;
  family: SketchFamily;
  opType: "create" | "update" | "delete";
  featureId: string;
  onSelectFeature: (feature: SpatialFeature) => void;
  onGeometry: (geometry: Sketch) => void;
  disabled: boolean;
}

const width = 800, height = 420;
function eligibleFeatures(features: SpatialFeature[], code: number): SketchFeature[] {
  return features.flatMap((item) => {
    const geometry = item.geometry;
    const parsed = geometry?.coordinate_reference.code === code && geometry.coordinate_reference.coordinate_order === "xy" &&
      item.geometry_state === "present" ? parseSketch(geometry) : null;
    return parsed ? [{ id: item.feature_id, geometry: parsed }] : [];
  });
}

export function NativeGeometryEditor({
  source, features, partial, family, opType, featureId, onSelectFeature, onGeometry, disabled
}: NativeGeometryEditorProps) {
  const code = source.stored_crs!.code;
  const extent = source.extent;
  const geographic = code === 4326 || code === 4756;
  const initial = padBounds(extent && extent.coordinate_reference.code === code
    ? [extent.min_x, extent.min_y, extent.max_x, extent.max_y] : [0, 0, 0, 0], geographic ? 0.004 : 100);
  const [bounds, setBounds] = useState<NativeBounds>(initial);
  const [points, setPoints] = useState<XY[]>([]);
  const [mode, setMode] = useState<"draw" | "select">(opType === "create" ? "draw" : "select");
  const [snapEnabled, setSnapEnabled] = useState(true);
  const [tolerance, setTolerance] = useState(12);
  const [snap, setSnap] = useState<SnapCandidate | null>(null);
  const [selectedVertex, setSelectedVertex] = useState<number | null>(null);
  const [selectionMessage, setSelectionMessage] = useState("");
  const [history, setHistory] = useState<XY[][]>([]);
  const [redo, setRedo] = useState<XY[][]>([]);
  // Bound UI work even if a single API page contains unusually large geometries.
  const eligible = eligibleFeatures(features, code);
  const vertexCount = eligible.reduce((sum, f) => sum + pathsOf(f.geometry).reduce((n, path) => n + path.length, 0), 0);
  const display = vertexCount <= 20_000 ? eligible : [];
  const usable = vertexCount <= 20_000;
  const drawAllowed = !disabled && usable && opType !== "delete" && mode === "draw";
  const viewport = { bounds, width, height };
  const sketch = buildSketch(family, points);
  const activeUpdate = opType === "update" ? featureId : undefined;

  function setNext(next: XY[]) {
    setHistory(prev => [...prev, points]);
    setRedo([]);
    setPoints(next);
  }
  function chooseFeature(id: string) {
    const feature = features.find(item => item.feature_id === id);
    const geometry = display.find(item => item.id === id)?.geometry;
    if (disabled || opType === "create" || !feature) return;
    onSelectFeature(feature);
    const vertices = geometry && geometry.type === family ? editableVertices(geometry) : null;
    setSelectedVertex(null);
    setSnap(null);
    setHistory([]);
    setRedo([]);
    setPoints(vertices ?? []);
    if (opType === "update" && vertices) {
      setMode("draw");
      setSelectionMessage("Đã tải hình học hiện tại. Chọn một đỉnh rồi bấm vị trí mới để di chuyển.");
    } else if (opType === "update") {
      setSelectionMessage("Không thể chỉnh hình học này bằng công cụ đồ họa. Giữ hình học gốc hoặc dùng GeoJSON nâng cao.");
    }
  }
  function pointer(event: MouseEvent<SVGSVGElement> | PointerEvent<SVGSVGElement>) {
    const rect = event.currentTarget.getBoundingClientRect();
    const p: XY = [event.clientX - rect.left, event.clientY - rect.top];
    const actual = { bounds, width: rect.width, height: rect.height };
    const candidate = snapEnabled ? nearestSnap(p, display, actual, tolerance, activeUpdate) : null;
    return { candidate, position: candidate?.point ?? fromScreen(p, actual) };
  }
  function addPoint(event: MouseEvent<SVGSVGElement>) {
    if (!drawAllowed) return;
    const { candidate, position } = pointer(event);
    setSnap(candidate);
    if (selectedVertex !== null) {
      setNext(points.map((p, i) => i === selectedVertex ? position : p));
      setSelectedVertex(null);
    } else if (family === "Point") {
      setNext([position]);
    } else if (points.length < 4095) {
      setNext([...points, position]);
    }
  }
  function moveView(dx: number, dy: number, scale: number) {
    setBounds(([west, south, east, north]) => {
      const spanX = east - west, spanY = north - south;
      const x = (west + east) / 2 + dx * spanX, y = (south + north) / 2 + dy * spanY;
      return [x - spanX * scale / 2, y - spanY * scale / 2, x + spanX * scale / 2, y + spanY * scale / 2];
    });
    setSnap(null);
  }
  function screen(point: XY) { return toScreen(point, viewport); }
  function path(drawing: Sketch) {
    return pathsOf(drawing).map(line => line.map((p, i) => (i ? "L" : "M") + screen(p).join(" ")).join(" ") +
      (drawing.type === "Polygon" ? " Z" : "")).join(" ");
  }

  return (
    <section className="ovg-sketch" aria-label="Trình biên tập hình học đồ họa">
      <h3>Biên tập trực quan · EPSG:{code}</h3>
      <p>Lưới tọa độ gốc X/Y ({geographic ? "độ" : "mét"}), không chuyển đổi CRS ngầm. Bám điểm/đoạn theo pixel màn hình.</p>
      {partial && <p role="status">Chỉ hiện tối đa 50 đối tượng của trang hiện tại; bám hình học chỉ dùng các đối tượng đang hiển thị.</p>}
      {!usable && <p role="alert">Vượt giới hạn 20.000 đỉnh hiển thị; bản đồ biên tập chưa khả dụng cho trang dữ liệu này.</p>}
      <div className="ovg-form-row">
        {opType !== "create" && <>
          <label>Chọn đối tượng hiện có
            <select value={features.some(f => f.feature_id === featureId) ? featureId : ""} onChange={e => chooseFeature(e.target.value)} disabled={disabled}>
              <option value="">Chọn mã đối tượng</option>
              {features.map(f => <option key={f.feature_id} value={f.feature_id}>{f.feature_id}</option>)}
            </select>
          </label>
          <button type="button" disabled={disabled} onClick={() => setMode("select")}>Chọn trên lưới</button>
        </>}
        {opType !== "delete" && <>
          <button type="button" disabled={disabled || (opType === "update" && points.length === 0)}
            onClick={() => { setMode("draw"); setSelectedVertex(null); }}>Vẽ / sửa đỉnh</button>
          <label className="ovg-inline-check"><input type="checkbox" checked={snapEnabled} disabled={disabled} onChange={e => {setSnapEnabled(e.target.checked);setSnap(null);}} /> Bám điểm và đoạn</label>
          <label>Dung sai bám (px)
            <input aria-label="Dung sai bám (px)" type="number" min={1} max={40} value={tolerance} disabled={disabled} onChange={e => setTolerance(Math.max(1, Math.min(40, Number(e.target.value) || 1)))} />
          </label>
        </>}
        <button type="button" disabled={disabled} onClick={() => moveView(0, 0, 0.5)}>Phóng to</button>
        <button type="button" disabled={disabled} onClick={() => moveView(0, 0, 2)}>Thu nhỏ</button>
        <button type="button" aria-label="Dịch trái" disabled={disabled} onClick={() => moveView(-0.25, 0, 1)}>←</button>
        <button type="button" aria-label="Dịch phải" disabled={disabled} onClick={() => moveView(0.25, 0, 1)}>→</button>
        <button type="button" aria-label="Dịch lên" disabled={disabled} onClick={() => moveView(0, 0.25, 1)}>↑</button>
        <button type="button" aria-label="Dịch xuống" disabled={disabled} onClick={() => moveView(0, -0.25, 1)}>↓</button>
        <button type="button" disabled={disabled} onClick={() => {setBounds(initial);setSnap(null);}}>Về phạm vi nguồn</button>
      </div>
      <svg data-testid="native-edit-canvas" aria-label="Lưới biên tập tọa độ gốc" className={drawAllowed ? "ovg-edit-canvas drawing" : "ovg-edit-canvas"}
        viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none"
        onClick={addPoint} onPointerMove={e => { if (drawAllowed) setSnap(pointer(e).candidate); }} onPointerLeave={() => setSnap(null)}>
        <rect width={width} height={height} fill="#f0f6fa" />
        {[1, 2, 3, 4, 5, 6, 7].map(i => <line key={`x${i}`} x1={i * width / 8} x2={i * width / 8} y1={0} y2={height} stroke="#cbd5e1" />)}
        {[1, 2, 3, 4, 5].map(i => <line key={`y${i}`} x1={0} x2={width} y1={i * height / 6} y2={i * height / 6} stroke="#cbd5e1" />)}
        {display.map(({ id, geometry }) => {
          const selected = opType !== "create" && id === featureId;
          const color = selected ? "#d97706" : "#64748b";
          const handle = (event: MouseEvent<SVGElement>) => {
            if (mode === "select" && opType !== "create") {event.stopPropagation();chooseFeature(id);}
          };
          if (geometry.type === "Point") {
            const p = screen(geometry.coordinates);
            return <circle key={id} data-feature-id={id} cx={p[0]} cy={p[1]} r={7} fill={color} stroke="#fff" strokeWidth={2} onClick={handle}><title>{id}</title></circle>;
          }
          return <path key={id} data-feature-id={id} d={path(geometry)} fill={geometry.type === "Polygon" ? "#a3c4d5" : "none"} fillRule="evenodd" stroke={color} strokeWidth={3} onClick={handle}><title>{id}</title></path>;
        })}
        {points.length > 1 && <polyline points={[...points, ...(sketch?.type === "Polygon" ? [points[0]!] : [])].map(p => screen(p).join(",")).join(" ")}
          stroke="#2563eb" strokeWidth={3} fill={sketch?.type === "Polygon" ? "#93c5fd55" : "none"} pointerEvents="none" />}
        {points.map((point, index) => {
          const p = screen(point);
          return <circle key={index} cx={p[0]} cy={p[1]} r={selectedVertex === index ? 9 : 6}
            stroke="#1d4ed8" strokeWidth={2} fill={selectedVertex === index ? "#fde047" : "white"}
            data-testid={`draft-vertex-${index}`}
            onClick={e => { if (drawAllowed) { e.stopPropagation();setSelectedVertex(index); } }} />;
        })}
        {snap && <circle cx={screen(snap.point)[0]} cy={screen(snap.point)[1]} r={13}
          stroke="#16a34a" strokeWidth={2} fill="none" pointerEvents="none" data-testid="snap-highlight" />}
      </svg>
      <p className="ovg-sketch-coordinates">X: {bounds[0].toFixed(6)} → {bounds[2].toFixed(6)} · Y: {bounds[1].toFixed(6)} → {bounds[3].toFixed(6)}</p>
      <p role="status" data-testid="snap-status">{snap ? `Bám ${snap.kind === "vertex" ? "đỉnh" : "đoạn"}: ${snap.featureId} · ${snap.point.map(n => n.toFixed(6)).join(", ")}` : "Chưa có ứng viên bám hình học."}</p>
      {selectionMessage && <p role="status">{selectionMessage}</p>}
      {opType !== "delete" && <div className="ovg-form-row">
        <span>{family} · {points.length} đỉnh {selectedVertex !== null ? `· di chuyển đỉnh ${selectedVertex + 1}` : ""}</span>
        <button type="button" disabled={disabled || !history.length} onClick={() => {
          const prior = history[history.length - 1]!;setRedo(prev => [points, ...prev]);setPoints(prior);setHistory(prev => prev.slice(0, -1));setSelectedVertex(null);
        }}>Hoàn tác đỉnh</button>
        <button type="button" disabled={disabled || !redo.length} onClick={() => {
          const next = redo[0]!;setHistory(prev => [...prev, points]);setPoints(next);setRedo(prev => prev.slice(1));setSelectedVertex(null);
        }}>Làm lại đỉnh</button>
        <button type="button" disabled={disabled || !points.length} onClick={() => {setNext([]);setSelectedVertex(null);}}>Vẽ lại</button>
        <button type="button" className="ovg-btn" disabled={disabled || !sketch} onClick={() => {if (sketch) onGeometry(sketch);}}>Dùng hình học đã vẽ</button>
      </div>}
    </section>
  );
}
