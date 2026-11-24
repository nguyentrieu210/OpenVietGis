# Ứng viên phát hành thống nhất — kiểm thử và bàn giao

Spec 031–047 là chương trình ổn định hóa nền tảng, không phải 17 chứng chỉ production. Chỉ phát hành từ một SHA đã review. Không dùng kết quả từ SHA cũ để gắn PASS cho ứng viên mới.

## Kiểm tra local có thể tái hiện

Trong clone/worktree sạch, cài dependency khóa bằng pnpm và uv theo TEST_RUNBOOK. Sử dụng Python API và MCP environment riêng. Không dùng dữ liệu production.

```bash
pnpm install --frozen-lockfile
uv sync --locked --project apps/api
uv sync --locked --project apps/mcp
PYTHONPATH=apps/api:. uv run --project apps/api python -m tools.local_candidate_gate \
  --output "/tmp/ovg-candidate-$(git rev-parse --short HEAD)-$(date +%s)" \
  --mcp-python "$(pwd)/apps/mcp/.venv/bin/python"
```

Runner thực hiện kiểm tra local tuần tự, ghi SHA, lockfile hashes, exit code, thời gian, log hashes và kiểm tra artifact của canonical web-next. Thư mục evidence phải mới và ngoài checkout. Log chỉ là evidence kỹ thuật local; cần kiểm tra nội dung nhạy cảm trước khi chia sẻ. CI chạy canonical browser và MCP suite bổ sung vào workflow hiện hữu. Browser synthetic không thay thế API/PostGIS/OIDC E2E.

## Release checklist (giữ fail-closed)

1. Chốt candidate sạch và review độc lập, đối chiếu refs chưa hợp nhất. Build frontend/backend/MCP và QGIS package từ cùng source SHA; lưu image digest, lock hashes và migration heads.
2. Kiểm thử PostGIS mới, quyền tenant/RLS, import/export/reopen, revision/delta và plugin. Chạy benchmark đã chốt, không hạ ngưỡng để qua.
3. Audit manifest chính thức hiện hữu trước rollout Spec039; không sửa/xóa history tự động. Kiểm tra Windows QGIS native, CAD relay, 3D thật và OAuth/consent.
4. Kiểm tra dependency/license, secret/history, security review. Chạy restore gồm DB + artifacts trên hệ cô lập, chứng minh rollback.
5. Ghi bằng chứng CQ-00–17 theo phạm vi và reviewer. Dùng tools/check_release_gate.py để kiểm tra hồ sơ chứng nhận đúng SHA. Gate local không cấp production_approved; không tự thay NOT_RUN thành PASS.
6. Chỉ sau quyết định của maintainer có thẩm quyền mới merge/push/tag/deploy, migrate hoặc đổi live. Giữ phiên bản rollback và không xóa worktree/ref đang dùng.

Xem [RELEASE_POLICY](RELEASE_POLICY.md), [ACCEPTANCE_MATRIX](ACCEPTANCE_MATRIX.md), [TEST_RUNBOOK](TEST_RUNBOOK.md).
