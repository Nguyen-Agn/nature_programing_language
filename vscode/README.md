# Anature cho VS Code

Gợi ý khi gõ, báo lỗi ngay trong lúc soạn, và tô màu cho tệp `.agn`.

Phần mở rộng này chỉ khởi động `anature lsp`. Toàn bộ từ vựng và luật lấy từ
`language.toml` (gần nhất tính từ thư mục của tệp đang mở đi ngược lên, nếu không thì
bản mặc định), nên sửa cấu hình
là gợi ý và màu đổi theo, không cần cài lại gì.

## Cài đặt

```sh
# 1. Biên dịch và đưa chương trình anature vào PATH
cargo build --release
ln -s "$PWD/target/release/anature" ~/.local/bin/anature

# 2. Đóng gói và cài phần mở rộng, rồi mở lại VS Code
cd vscode && npm install
npx @vscode/vsce package --allow-missing-repository --skip-license
code --install-extension anature-0.1.0.vsix
```

Phần mở rộng tìm chương trình theo thứ tự: thiết lập `anature.serverPath`,
`~/.local/bin/anature`, `~/.cargo/bin/anature`, rồi PATH.

Gỡ cài đặt: `code --uninstall-extension anature.anature` và xóa `~/.local/bin/anature`.

## Cách dùng

- **Gợi ý** hiện khi gõ chữ hoặc dấu cách; `Ctrl+Space` để gọi lại. Nhấn `Tab` để
  nhận. `Enter` luôn xuống dòng, không nhận gợi ý.
- **Khung câu lệnh**: ở đầu dòng, chọn một mục có dạng `lặp ‹biến› từ ‹đầu› đến ‹cuối› { }`
  để điền cả câu, rồi `Tab` qua từng ô.
- **Lỗi** được gạch chân ngay khi gõ: lỗi cú pháp kèm "có phải bạn muốn...", và lỗi về
  tên, kiểu (biến chưa khai báo, gọi hàm sai kiểu).
- **Màu** theo đúng từ vựng trong cấu hình: từ khóa, kiểu, toán tử, tên hàm, tên
  biến, số, chuỗi. Từ đệm (`thì`, `giá trị của`, ...) được tô mờ như chú thích, vì
  trình dịch bỏ qua chúng.
- **Chạy**: nút ▶ góc trên bên phải, hoặc lệnh `Anature: Chạy tệp hiện tại`.

## Trình soạn thảo khác

`anature lsp` là một máy chủ LSP chuẩn qua stdio, dùng được với Neovim, Helix,
Zed... bằng cách khai báo lệnh `anature lsp` cho tệp `.agn`.
