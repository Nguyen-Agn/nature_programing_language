# Anature cho VS Code

Anature là một ngôn ngữ lập trình viết bằng tiếng Việt, với từ vựng và luật ngữ pháp
nằm trong một file cấu hình (`language.toml`). Chương trình được dịch sang Java để chạy.

```
Lớp Lời Chào {
    Hàm gốc {
        nhập chữ tên với lời nhắc "Bạn tên gì? "
        lặp i từ 1 đến 3 {
            in("Xin chào {tên}, lần {i}")
        }
    }
}
```

## Tính năng

- **Gợi ý khi gõ**: tại mỗi vị trí, chỉ những cụm hợp lệ ở đó được đề xuất. Ở đầu câu
  còn có khung của cả câu lệnh (`lặp ‹biến› từ ‹đầu› đến ‹cuối› { }`), `Tab` để nhảy
  qua từng ô.
- **Báo lỗi ngay khi gõ**: lỗi cú pháp kèm "có phải bạn muốn...", và lỗi về tên, kiểu
  (biến chưa khai báo, gọi hàm sai kiểu), bằng tiếng Việt.
- **Tô màu theo từ vựng**: từ khóa, kiểu, toán tử, tên hàm, tên biến. Từ đệm (`thì`,
  `giá trị của`...) được tô mờ, vì trình dịch bỏ qua chúng.
- **Chạy tệp**: nút ▶ ở góc trên bên phải, hoặc `Ctrl+F5`.

## Yêu cầu

- Để **chạy** chương trình cần Java 11 trở lên (lệnh `java` trong PATH).
- Gợi ý, báo lỗi và tô màu không cần Java.

Chương trình `anature` đi kèm sẵn trong phần mở rộng; không phải cài gì thêm.

## Cách dùng

1. Tạo một tệp có đuôi `.agn`.
2. Gõ; gợi ý hiện khi gõ chữ hoặc dấu cách. Nhấn `Tab` để nhận gợi ý. `Enter` luôn
   xuống dòng, không nhận gợi ý.
3. Nhấn ▶ hoặc `Ctrl+F5` để chạy.

## Đổi từ vựng

Đặt một file `language.toml` ở thư mục của dự án (hoặc bất kỳ thư mục cha nào của tệp
`.agn`). Phần mở rộng đọc lại file đó ở mỗi lần gõ, nên thêm một từ khóa là gợi ý và
màu đổi theo ngay. Không có file nào thì dùng bộ từ vựng mặc định.

Khi `language.toml` đang có lỗi, lỗi được gạch chân trên chính file đó; các tệp `.agn`
tiếp tục được phân tích bằng bản hợp lệ gần nhất.

## Lệnh

| Lệnh | Tác dụng |
|---|---|
| `Anature: Chạy tệp hiện tại` | Dịch rồi chạy trong terminal |
| `Anature: Sinh file Java` | Ghi file `.java` cạnh tệp nguồn, để xem mã sinh ra |
| `Anature: Khởi động lại máy chủ gợi ý` | Dùng khi gợi ý ngừng phản hồi |

## Thiết lập

| Thiết lập | Mặc định | Ý nghĩa |
|---|---|---|
| `anature.serverPath` | trống | Đường dẫn tới chương trình `anature` khác với bản đi kèm |

## Tự đóng gói

```sh
cd vscode
npm install
npm run package          # tạo anature-<nền tảng>.vsix cho máy đang dùng
code --install-extension anature-linux-x64.vsix
```

Chương trình `anature` là mã máy, nên mỗi nền tảng (Linux, macOS, Windows) cần một gói
riêng, tạo bằng cách chạy lệnh trên ngay trên nền tảng đó. Cần có Rust (`cargo`).

## Trình soạn thảo khác

`anature lsp` là một máy chủ LSP chuẩn qua stdio, dùng được với Neovim, Helix, Zed...
bằng cách khai báo lệnh `anature lsp` cho tệp `.agn`.
