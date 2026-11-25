// Spec 058: QGIS-style keyboard shortcuts (plus MapInfo-style "A" for pan/select).
// Typing commands needs Enter first, so plain keys are free for shortcuts. Owner can switch them off.
export type Hotkey = { combo: string; label: string; action: string };
export const HOTKEYS: Hotkey[] = [
  { combo: "A", label: "Bàn tay ↔ con trỏ chọn", action: "@PAN_TOGGLE" },
  { combo: "Ctrl+S", label: "Lưu bản đồ", action: "SAVE" },
  { combo: "Ctrl+L", label: "Quản lý nguồn dữ liệu", action: "@DSM" },
  { combo: "F6", label: "Bảng thuộc tính", action: "ATTR" },
  { combo: "F7", label: "Thuộc tính / ký hiệu lớp", action: "LAYER" },
  { combo: "Ctrl+1", label: "Ẩn / hiện khung Lớp", action: "LAYERS" },
  { combo: "Ctrl+=", label: "Phóng to", action: "ZI" },
  { combo: "Ctrl+-", label: "Thu nhỏ", action: "ZO" },
  { combo: "Ctrl+Shift+F", label: "Toàn bản đồ", action: "ZE" },
  { combo: "Ctrl+J", label: "Phóng tới đối tượng đang chọn", action: "ZS" },
  { combo: "Ctrl+Alt+A", label: "Bỏ chọn tất cả", action: "DESELECT" },
  { combo: "Ctrl+.", label: "Thêm đối tượng (bật chỉnh sửa)", action: "EDIT" },
  { combo: "Ctrl+Shift+M", label: "Đo khoảng cách", action: "DIST" },
  { combo: "Ctrl+Shift+J", label: "Đo diện tích", action: "AREA" },
  { combo: "Delete", label: "Xóa đối tượng đang chọn", action: "@DELETE" },
  { combo: "Shift+?", label: "Bảng phím tắt", action: "@HELP" },
  { combo: "/", label: "Chat nhanh trên bản đồ (hiện cạnh con trỏ)", action: "@CHAT" },
];
export const FIXED_KEYS: Hotkey[] = [
  { combo: "Giữ Space", label: "Giữ Space và rê chuột để di chuyển bản đồ", action: "" },
  { combo: "Enter", label: "Vào ô lệnh · hoàn tất khi đang vẽ/đo", action: "" },
  { combo: "Esc", label: "Hủy thao tác", action: "" },
  { combo: "Ctrl+Z / Ctrl+Shift+Z", label: "Hoàn tác / làm lại", action: "" },
];
const KEY = "ovg:hotkeys";
export const hotkeysEnabled = () => { try { return localStorage.getItem(KEY) !== "off"; } catch { return true; } };
export const setHotkeysEnabled = (on: boolean) => { try { localStorage.setItem(KEY, on ? "on" : "off"); } catch { /* storage unavailable */ } window.dispatchEvent(new Event("ovg:hotkeys-changed")); };

export function comboOf(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">): string {
  const k = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  // Shifted symbols are reported as the symbol itself: "+" is Shift+=, "_" is Shift+-, "?" is Shift+/.
  const shifted: Record<string, string> = { "+": "=", "_": "-" };
  const base = shifted[k] ?? k, shift = e.shiftKey && !(k in shifted) ? "Shift" : "";
  return [e.ctrlKey || e.metaKey ? "Ctrl" : "", e.altKey ? "Alt" : "", shift, base].filter(Boolean).join("+");
}
export const findHotkey = (e: Parameters<typeof comboOf>[0]) => { const c = comboOf(e); return HOTKEYS.find(h => h.combo === c); };
