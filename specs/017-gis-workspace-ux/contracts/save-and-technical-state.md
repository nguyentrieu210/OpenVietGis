# UI Contract: Save State & Technical Details

## Save presentation

| Parent outcome | User-facing state |
| --- | --- |
| no shared changes | Đã lưu |
| shared changes present | Có thay đổi chưa lưu |
| save in flight | Đang lưu… |
| authoritative commit confirmed | Đã lưu phiên bản bản đồ N |
| stale/CAS conflict | Có phiên bản mới — tải lại/đối chiếu trước khi lưu |
| pre-commit failure | Lưu thất bại — phiên bản trước vẫn giữ nguyên |
| lost acknowledgement / unresolved | Chưa xác định kết quả lưu — cần đối chiếu |

Transient pan/zoom/selection/panel layout changes do not mark the shared map dirty.

Navigation away with shared dirty state requires confirmation before discard.

## Technical Details drawer

Secondary, closed by default. May show currently authorized:

- Mã tổ chức
- Mã dự án
- Mã bản đồ
- Layer / Source IDs
- CRS
- Map configuration revision
- Source content/schema revision
- capability/availability diagnostics

It must clear on organization/project/layer scope change and must never expose secrets or unauthorized resource metadata.
