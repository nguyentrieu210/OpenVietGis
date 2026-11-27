// Spec 058: display CRS for the status bar (QGIS shows cursor coordinates in the project CRS).
// Map rendering stays Web Mercator; this only converts WGS 84 lon/lat for display.
// VN-2000 uses the WGS 84 ellipsoid with the official 7-parameter shift (TT 973/2001/TT-TCĐC).
export type DisplayCrs = { id: string; name: string; group: string; kind: "geographic" | "mercator" | "tm"; lon0?: number; k0?: number; fe?: number; vn2000?: boolean; epsg?: number };

const A = 6378137, F = 1 / 298.257223563, E2 = F * (2 - F);
const VN_ZONES: [number, string][] = [
  [102, "Lai Châu"], [103, "Điện Biên"], [104, "Sơn La"], [104.5, "Kiên Giang, Cà Mau"], [104.75, "Lào Cai, Phú Thọ, Nghệ An, An Giang"],
  [105, "Vĩnh Phúc, Hà Nam, Ninh Bình, Thanh Hóa, Đồng Tháp, Cần Thơ, Bạc Liêu, Hậu Giang"], [105.5, "Hà Giang, Bắc Ninh, Hải Dương, Hưng Yên, Nam Định, Thái Bình, Hà Tĩnh, Tây Ninh, Vĩnh Long, Trà Vinh"],
  [105.75, "Hà Nội, Hải Phòng, Long An, Tiền Giang, Bến Tre, TP Hồ Chí Minh, Bình Dương"], [106, "Tuyên Quang, Hòa Bình, Quảng Bình"], [106.25, "Quảng Trị, Bình Phước"],
  [106.5, "Bắc Kạn, Thái Nguyên"], [107, "Bắc Giang, Thừa Thiên Huế"], [107.25, "Lạng Sơn"], [107.5, "Kon Tum"], [107.75, "Quảng Ninh, Đồng Nai, Bà Rịa - Vũng Tàu, Quảng Nam, Lâm Đồng, Đà Nẵng"],
  [108, "Quảng Ngãi"], [108.25, "Bình Định, Khánh Hòa, Ninh Thuận"], [108.5, "Gia Lai, Đắk Lắk, Đắk Nông, Phú Yên, Bình Thuận"],
];
const dms = (x: number) => { const d = Math.floor(x), m = Math.round((x - d) * 60); return d + "°" + (m ? String(m).padStart(2, "0") + "′" : ""); };
export const DISPLAY_CRS: DisplayCrs[] = [
  { id: "EPSG:4326", name: "WGS 84 (kinh độ, vĩ độ)", group: "Địa lý", kind: "geographic", epsg: 4326 },
  { id: "EPSG:3857", name: "WGS 84 / Pseudo-Mercator", group: "Toàn cầu", kind: "mercator", epsg: 3857 },
  { id: "EPSG:32648", name: "WGS 84 / UTM múi 48N", group: "UTM", kind: "tm", lon0: 105, k0: 0.9996, epsg: 32648 },
  { id: "EPSG:32649", name: "WGS 84 / UTM múi 49N", group: "UTM", kind: "tm", lon0: 111, k0: 0.9996, epsg: 32649 },
  { id: "EPSG:3405", name: "VN-2000 / UTM múi 48N", group: "VN-2000", kind: "tm", lon0: 105, k0: 0.9996, vn2000: true, epsg: 3405 },
  { id: "EPSG:3406", name: "VN-2000 / UTM múi 49N", group: "VN-2000", kind: "tm", lon0: 111, k0: 0.9996, vn2000: true, epsg: 3406 },
  ...VN_ZONES.map(([lon0, where]) => ({ id: "VN2000-TM3-" + lon0, name: "VN-2000 / TM-3 kinh tuyến trục " + dms(lon0) + " · " + where, group: "VN-2000 TM-3 (bản đồ địa chính)", kind: "tm" as const, lon0, k0: 0.9999, vn2000: true })),
];

const rad = Math.PI / 180;
// WGS 84 → VN-2000 geodetic: inverse of the VN-2000 → WGS 84 Bursa-Wolf parameters (small-angle).
const TOWGS = { dx: -191.90441429, dy: -39.30318279, dz: -111.45032835, rx: -0.00928836 / 3600 * rad, ry: 0.01975479 / 3600 * rad, rz: -0.00427372 / 3600 * rad, s: 0.252906278e-6 };
function toVn2000(lon: number, lat: number): [number, number] {
  const p = lat * rad, l = lon * rad, N = A / Math.sqrt(1 - E2 * Math.sin(p) ** 2);
  const X = N * Math.cos(p) * Math.cos(l), Y = N * Math.cos(p) * Math.sin(l), Z = N * (1 - E2) * Math.sin(p);
  const { dx, dy, dz, rx, ry, rz, s } = TOWGS, k = 1 / (1 + s);
  const x = X - dx, y = Y - dy, z = Z - dz;
  const Xv = k * (x + rz * y - ry * z), Yv = k * (-rz * x + y + rx * z), Zv = k * (ry * x - rx * y + z);
  const pr = Math.hypot(Xv, Yv); let lat2 = Math.atan2(Zv, pr * (1 - E2));
  for (let i = 0; i < 5; i++) { const n = A / Math.sqrt(1 - E2 * Math.sin(lat2) ** 2); lat2 = Math.atan2(Zv + E2 * n * Math.sin(lat2), pr); }
  return [Math.atan2(Yv, Xv) / rad, lat2 / rad];
}
// Transverse Mercator (Snyder series), WGS 84 ellipsoid, false easting 500 km, false northing 0.
function tm(lon: number, lat: number, lon0: number, k0: number): [number, number] {
  const p = lat * rad, ep2 = E2 / (1 - E2), N = A / Math.sqrt(1 - E2 * Math.sin(p) ** 2), T = Math.tan(p) ** 2, C = ep2 * Math.cos(p) ** 2, Aa = (lon - lon0) * rad * Math.cos(p);
  const M = A * ((1 - E2 / 4 - 3 * E2 ** 2 / 64 - 5 * E2 ** 3 / 256) * p - (3 * E2 / 8 + 3 * E2 ** 2 / 32 + 45 * E2 ** 3 / 1024) * Math.sin(2 * p) + (15 * E2 ** 2 / 256 + 45 * E2 ** 3 / 1024) * Math.sin(4 * p) - (35 * E2 ** 3 / 3072) * Math.sin(6 * p));
  const x = 500000 + k0 * N * (Aa + (1 - T + C) * Aa ** 3 / 6 + (5 - 18 * T + T * T + 72 * C - 58 * ep2) * Aa ** 5 / 120);
  const y = k0 * (M + N * Math.tan(p) * (Aa * Aa / 2 + (5 - T + 9 * C + 4 * C * C) * Aa ** 4 / 24 + (61 - 58 * T + T * T + 600 * C - 330 * ep2) * Aa ** 6 / 720));
  return [x, y];
}

export function projectForDisplay(lon: number, lat: number, crs: DisplayCrs): [number, number] {
  if (crs.kind === "geographic") return [lon, lat];
  if (crs.kind === "mercator") return [A * lon * rad, A * Math.log(Math.tan(Math.PI / 4 + lat * rad / 2))];
  const [l, p] = crs.vn2000 ? toVn2000(lon, lat) : [lon, lat];
  return tm(l, p, crs.lon0!, crs.k0!);
}
export const findCrs = (id: string | null | undefined) => DISPLAY_CRS.find(c => c.id === id) ?? DISPLAY_CRS[0]!;

/** Conservative display domain, not an EPSG area-of-use polygon or survey certification. */
export function displayCrsWarning(lon:number,lat:number,crs:DisplayCrs):string|null{
 if(!Number.isFinite(lon)||!Number.isFinite(lat)||Math.abs(lon)>180||Math.abs(lat)>90)return "Tọa độ ngoài miền WGS84";
 if(crs.kind==="mercator"&&Math.abs(lat)>85.05112878)return "Ngoài miền Web Mercator";
 if(crs.kind==="tm"){
  const tm3=crs.id.startsWith("VN2000-TM3-"),half=tm3?1.5:3;
  if(lat<0||lat>84||!Number.isFinite(crs.lon0)||Math.abs(lon-crs.lon0!)>half+1e-9)return "Ngoài múi chiếu đã chọn";
 }
 return null;
}
