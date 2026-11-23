"""Opt-in Windows V8i receiver and manual, reviewed R026 geometry writeback."""
from __future__ import annotations

import json
import os
import queue
import threading
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox, simpledialog, ttk

from receiver import Connection, ReceiverError, default_inbox, receive_layer
from outbound import CadSyncError, bind_selected, draft_outbound, receipt
from sync_transport import preview_cloud, commit_approved
from v8i_probe import ProbeError, probe


class App:
    def __init__(self, window: tk.Tk):
        self.window = window
        self.window.title("OpenVietGIS | Windows V8i Connector")
        self.window.geometry("870x665")
        self.window.minsize(750, 600)
        self.window.columnconfigure(0, weight=1)
        self.window.rowconfigure(0, weight=1)
        self.events: queue.Queue[tuple[str, str]] = queue.Queue()
        self.busy = False
        self.bundle = None
        self.preview_digest = None
        frame = ttk.Frame(window, padding=18)
        frame.grid(sticky="nsew")
        frame.columnconfigure(1, weight=1)
        ttk.Label(frame, text="OpenVietGIS Desktop Connector", font=("Segoe UI", 17, "bold")).grid(
            row=0, column=0, columnspan=3, sticky="w", pady=(0, 6)
        )
        ttk.Label(
            frame, text="Nhận bản đồ từ WebGIS về Windows · bản xem trước an toàn, KHÔNG ghi đè DGN",
            foreground="#455468", wraplength=730
        ).grid(row=1, column=0, columnspan=3, sticky="w", pady=(0, 12))
        self.server = tk.StringVar(value="https://openvietgis.cloud")
        self.org = tk.StringVar()
        self.project = tk.StringVar()
        self.layer = tk.StringVar()
        self.token = tk.StringVar()
        self.inbox = tk.StringVar(value=str(default_inbox()))
        inputs = [
            ("Server HTTPS", self.server, False),
            ("Organization UUID", self.org, False),
            ("Project UUID", self.project, False),
            ("Layer UUID", self.layer, False),
            ("Bearer API Key", self.token, True),
            ("Thư mục nhận", self.inbox, False),
        ]
        for idx, (label, var, secret) in enumerate(inputs, start=2):
            ttk.Label(frame, text=label).grid(row=idx, column=0, sticky="w", pady=4, padx=(0, 10))
            e = ttk.Entry(frame, textvariable=var, show="●" if secret else "")
            e.grid(row=idx, column=1, columnspan=2, sticky="ew", pady=4)
        actions = ttk.Frame(frame)
        actions.grid(row=8, column=0, columnspan=3, sticky="ew", pady=(15, 8))
        self.connect_button = ttk.Button(actions, text="Kiểm tra V8i đang mở", command=self.check_v8i)
        self.connect_button.pack(side="left", padx=(0, 8))
        self.receive_button = ttk.Button(actions, text="Nhận dữ liệu từ OpenVietGIS", command=self.receive)
        self.receive_button.pack(side="left", padx=(0, 8))
        ttk.Button(actions, text="Mở thư mục", command=self.open_inbox).pack(side="left")
        extra = ttk.Frame(frame)
        extra.grid(row=9, column=0, columnspan=3, sticky="w", pady=(5, 12))
        ttk.Button(extra, text="Chọn phiên đã nhận", command=self.choose_bundle).pack(side="left", padx=(0, 6))
        ttk.Button(extra, text="Mở DXF để xem", command=self.open_dxf).pack(side="left", padx=(0, 6))
        ttk.Button(extra, text="Ánh xạ đối tượng đang chọn", command=self.map_selected).pack(side="left", padx=(0, 6))
        ttk.Button(extra, text="Tạo bản thay đổi", command=self.draft_changes).pack(side="left")
        self.crs_verified = tk.BooleanVar(value=False)
        ttk.Checkbutton(frame, text="Đã đối chiếu điểm khống chế, CRS và đơn vị DGN với GeoPackage (bắt buộc khi ánh xạ)",
                        variable=self.crs_verified).grid(row=10, column=0, columnspan=3, sticky="w")
        send = ttk.Frame(frame)
        send.grid(row=11, column=0, columnspan=3, sticky="w", pady=(8, 10))
        ttk.Button(send, text="Xem trước thay đổi trên Cloud", command=self.cloud_preview).pack(side="left", padx=(0, 8))
        ttk.Button(send, text="XÁC NHẬN GỬI lên Cloud", command=self.cloud_commit).pack(side="left")
        self.status = tk.StringVar(value="Chưa kết nối. V8i không bắt buộc mở để tải dữ liệu.")
        ttk.Label(frame, textvariable=self.status, wraplength=730).grid(
            row=12, column=0, columnspan=3, sticky="w", pady=(8, 8)
        )
        ttk.Label(frame, text="Kết quả / Nhật ký phiên (không lưu mật khẩu)").grid(
            row=13, column=0, columnspan=3, sticky="w"
        )
        self.log = tk.Text(frame, height=8, state="disabled", wrap="word", font=("Consolas", 10))
        self.log.grid(row=14, column=0, columnspan=3, sticky="nsew")
        frame.rowconfigure(14, weight=1)
        ttk.Label(frame, text=(
            "Lưu ý: DXF chỉ chứa hình học 2D. Thuộc tính nằm trong attributes.gpkg. "
            "Cần đối chiếu CRS/đơn vị với DGN trước khi gắn tham chiếu; KHÔNG tự động sửa/xóa đối tượng."
        ), foreground="#7d4b10", wraplength=730).grid(
            row=15, column=0, columnspan=3, sticky="w", pady=(12, 0)
        )
        self.window.after(100, self.drain)

    def write(self, line: str) -> None:
        self.log.configure(state="normal")
        self.log.insert("end", line + "\n")
        self.log.see("end")
        self.log.configure(state="disabled")

    def check_v8i(self) -> None:
        try:
            result = probe()
            self.status.set(f"V8i: {result['drawing']} / {result['model']} (read-only)")
            self.write("Đã bám vào phiên V8i đang mở. Không thay đổi bản vẽ.")
        except ProbeError as exc:
            self.status.set("V8i chưa kết nối (vẫn có thể tải dữ liệu từ Cloud).")
            self.write(str(exc))

    def choose_bundle(self) -> None:
        selected = filedialog.askdirectory(title="Chọn thư mục chứa geometry.dxf + attributes.gpkg", initialdir=self.inbox.get())
        if selected:
            try:
                folder, meta = receipt(selected)
                self.bundle = folder
                self.preview_digest = None
                self.layer.set(meta["layer_id"])
                self.org.set(meta["organization_id"])
                self.project.set(meta["project_id"])
                self.server.set(meta["server_origin"])
                self.crs_verified.set(False)
                self.write("Đang sử dụng phiên nhận: " + str(folder))
            except (CadSyncError, ReceiverError) as exc:
                self.write("Không thể chọn phiên: " + str(exc))

    def _require_bundle(self) -> Path:
        if self.bundle is None:
            raise CadSyncError("Hãy nhận dữ liệu hoặc chọn một phiên đã nhận trước.")
        return Path(self.bundle)

    def open_dxf(self) -> None:
        try:
            folder, meta = receipt(self._require_bundle())
            self.window.clipboard_clear()
            self.window.clipboard_append(str(folder / "geometry.dxf"))
            self.window.update()
            self.write("Đã sao chép đường dẫn DXF vào clipboard: " + str(folder / "geometry.dxf"))
            messagebox.showinfo(
                "Mở DXF trong MicroStation V8i",
                "Trong V8i: File → Open → chọn AutoCAD DXF (*.dxf), dán đường dẫn đã sao chép, bật Open as read-only. "
                "Không mở vào bản DGN đang chỉnh sửa. Kiểm tra CRS/đơn vị trước khi sử dụng."
            )
        except (CadSyncError, ReceiverError) as exc:
            self.write("Không mở được DXF: " + str(exc))

    def map_selected(self) -> None:
        if not self.crs_verified.get():
            messagebox.showwarning("Chưa xác minh tọa độ", "Phải đối chiếu CRS, Master Units và điểm khống chế trước khi ánh xạ.")
            return
        try:
            folder = self._require_bundle()
            fid = simpledialog.askstring("Ánh xạ V8i ↔ OpenVietGIS", "Nhập chính xác ovg_feature_id tương ứng với 1 đối tượng đang chọn trong V8i:")
            if not fid:
                return
            mapping = bind_selected(folder, fid.strip(), verified_crs_units=True)
            self.preview_digest = None
            self.write(f"Ánh xạ {mapping['feature_id']} ↔ V8i {mapping['cad_element_id']}; tổng {mapping['mapped_count']} đối tượng.")
        except (CadSyncError, ReceiverError) as exc:
            self.write("Ánh xạ bị chặn: " + str(exc))

    def draft_changes(self) -> None:
        try:
            result = draft_outbound(self._require_bundle())
            self.preview_digest = None
            self.write(f"Đã chuẩn bị {result['changes']} chỉnh sửa (chưa gửi): " + result["file"])
        except (CadSyncError, ReceiverError) as exc:
            self.write("Không tạo bản thay đổi: " + str(exc))

    def _connection_once(self) -> Connection:
        c = Connection(self.server.get().strip(), self.org.get().strip(),
                       self.project.get().strip(), self.token.get().strip())
        c.validated()
        self.token.set("")
        return c

    def cloud_preview(self) -> None:
        if self.busy:
            return
        try:
            folder = self._require_bundle()
            conn = self._connection_once()
        except (CadSyncError, ReceiverError) as exc:
            self.write("Không thể xem trước Cloud: " + str(exc))
            return
        self.busy = True
        self.preview_digest = None
        self.status.set("Đang hỏi máy chủ về diff, không ghi dữ liệu...")
        def job():
            try:
                value = preview_cloud(conn, folder)
                self.events.put(("preview", json.dumps(value)))
            except (CadSyncError, ReceiverError) as exc:
                self.events.put(("error", str(exc)))
        threading.Thread(target=job, daemon=True).start()

    def cloud_commit(self) -> None:
        if self.busy:
            return
        try:
            folder = self._require_bundle()
            if self.preview_digest is None:
                raise CadSyncError("Hãy chạy 'Xem trước thay đổi trên Cloud' trong phiên hiện tại trước.")
            phrase = simpledialog.askstring("Duyệt ghi vào OpenVietGIS", "Nhập chính xác 12 ký tự đầu của diff SHA-256 để xác nhận GỬI lên máy chủ:\n" + self.preview_digest[:12])
            if phrase != self.preview_digest[:12]:
                raise CadSyncError("Mã xác nhận không đúng; từ chối gửi.")
            if not messagebox.askyesno("XÁC NHẬN GHI CLOUD", "Ghi các hình học V8i ĐÃ ÁNH XẠ lên OpenVietGIS qua R006 Changeset? Không sửa DGN. Thao tác có thể ảnh hưởng dữ liệu trên Cloud."):
                return
            conn = self._connection_once()
        except (CadSyncError, ReceiverError) as exc:
            self.write("Không gửi dữ liệu: " + str(exc))
            return
        self.busy = True
        self.status.set("Đang xác nhận lại diff và gửi changeset có nhật ký...")
        sha = self.preview_digest
        def job():
            try:
                result = commit_approved(conn, folder, sha)
                self.events.put(("committed", json.dumps(result)))
            except (CadSyncError, ReceiverError) as exc:
                self.events.put(("error", str(exc)))
        threading.Thread(target=job, daemon=True).start()

    def receive(self) -> None:
        if self.busy:
            return
        self.busy = True
        self.receive_button.state(["disabled"])
        connection = Connection(
            self.server.get().strip(), self.org.get().strip(),
            self.project.get().strip(), self.token.get().strip()
        )
        layer_id, folder = self.layer.get().strip(), self.inbox.get().strip()
        # Release token from GUI input immediately; worker holds it in memory for one request.
        self.token.set("")
        self.status.set("Đang kiểm tra quyền và tải hai bản dữ liệu...")

        def job():
            try:
                received = receive_layer(connection, layer_id, folder)
                receipt = json.loads((received / "receipt.json").read_text(encoding="utf-8"))
                self.events.put(("success", json.dumps({
                    "path": str(received), "revision": receipt["content_revision"],
                    "crs": receipt["crs"], "count": receipt["feature_count"]
                }, ensure_ascii=False)))
            except ReceiverError as exc:
                self.events.put(("error", str(exc)))
            except OSError:
                self.events.put(("error", "Không thể lưu dữ liệu. Kiểm tra quyền thư mục và dung lượng ổ đĩa."))
        threading.Thread(target=job, daemon=True).start()

    def drain(self) -> None:
        try:
            while True:
                kind, payload = self.events.get_nowait()
                self.busy = False
                self.receive_button.state(["!disabled"])
                if kind == "preview":
                    result = json.loads(payload)
                    self.preview_digest = result["diff_sha256"]
                    self.status.set(f"Diff Cloud đã xác nhận: {result['update_count']} đối tượng; 0 tạo/xóa.")
                    self.write("Xem trước thành công. Diff SHA-256: " + self.preview_digest)
                elif kind == "committed":
                    result = json.loads(payload)
                    self.preview_digest = None
                    self.status.set("Changeset đã được máy chủ ghi nhận.")
                    self.write("Changeset: " + str(result["changeset_id"]) + " | operation=" + str(result["operation_id"]))
                elif kind == "success":
                    result = json.loads(payload)
                    self.bundle = Path(result["path"])
                    self.crs_verified.set(False)
                    self.preview_digest = None
                    self.status.set("Đã nhận hai tệp cùng phiên bản, SHA-256 hợp lệ.")
                    self.write(
                        f"Nhận: {result['count']} đối tượng | {result['crs']} | "
                        f"revision={result['revision']}\n  {result['path']}"
                    )
                    messagebox.showinfo(
                        "Đã nhận dữ liệu", "Mở thư mục để lấy geometry.dxf và attributes.gpkg.\n"
                        "Gắn DXF như reference/ mở riêng trong V8i sau khi kiểm tra đơn vị và CRS.\n"
                        "Không có thao tác ghi ngược DGN."
                    )
                else:
                    self.status.set("Không nhận được dữ liệu; chưa tạo phiên nhập mới.")
                    self.write("Lỗi: " + payload)
        except queue.Empty:
            pass
        self.window.after(100, self.drain)

    def open_inbox(self) -> None:
        path = Path(self.inbox.get()).expanduser()
        if not path.is_dir():
            messagebox.showinfo("Chưa có dữ liệu", "Hãy nhận dữ liệu trước khi mở thư mục.")
            return
        if os.name == "nt":
            os.startfile(str(path))  # type: ignore[attr-defined]
        else:
            self.write("Thư mục: " + str(path))


def main() -> None:
    root = tk.Tk()
    try:
        ttk.Style().theme_use("clam")
    except tk.TclError:
        pass
    App(root)
    root.mainloop()


if __name__ == "__main__":
    main()