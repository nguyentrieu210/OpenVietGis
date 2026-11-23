"""Interactive Windows receiver; no background polling, no CAD edits or persistent tokens."""
from __future__ import annotations

import json
import os
import queue
import threading
import tkinter as tk
from pathlib import Path
from tkinter import messagebox, ttk

from receiver import Connection, ReceiverError, default_inbox, receive_layer
from v8i_probe import ProbeError, probe


class App:
    def __init__(self, window: tk.Tk):
        self.window = window
        self.window.title("OpenVietGIS | Windows V8i Connector")
        self.window.geometry("770x545")
        self.window.minsize(650, 480)
        self.window.columnconfigure(0, weight=1)
        self.window.rowconfigure(0, weight=1)
        self.events: queue.Queue[tuple[str, str]] = queue.Queue()
        self.busy = False
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
        self.status = tk.StringVar(value="Chưa kết nối. V8i không bắt buộc mở để tải dữ liệu.")
        ttk.Label(frame, textvariable=self.status, wraplength=730).grid(
            row=9, column=0, columnspan=3, sticky="w", pady=(8, 8)
        )
        ttk.Label(frame, text="Kết quả / Nhật ký phiên (không lưu mật khẩu)").grid(
            row=10, column=0, columnspan=3, sticky="w"
        )
        self.log = tk.Text(frame, height=8, state="disabled", wrap="word", font=("Consolas", 10))
        self.log.grid(row=11, column=0, columnspan=3, sticky="nsew")
        frame.rowconfigure(11, weight=1)
        ttk.Label(frame, text=(
            "Lưu ý: DXF chỉ chứa hình học 2D. Thuộc tính nằm trong attributes.gpkg. "
            "Cần đối chiếu CRS/đơn vị với DGN trước khi gắn tham chiếu; KHÔNG tự động sửa/xóa đối tượng."
        ), foreground="#7d4b10", wraplength=730).grid(
            row=12, column=0, columnspan=3, sticky="w", pady=(12, 0)
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
                if kind == "success":
                    result = json.loads(payload)
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