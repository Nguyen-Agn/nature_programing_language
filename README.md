# Anature

Anature là một ngôn ngữ lập trình viết bằng tiếng Việt. Điểm khác biệt của nó: **từ vựng và luật ngữ pháp không nằm trong trình dịch mà nằm trong một file cấu hình**, nên bạn tự quyết định ngôn ngữ của mình đọc như thế nào.

```
Lớp Lời Chào {
    Hàm tổng(danh sách số nguyên ds) trả về số nguyên {
        đặt biến kết quả là 0
        với mỗi x trong ds {
            kết quả += x
        }
        trả về kết quả
    }

    Hàm gốc {
        nhập chữ tên với lời nhắc "Bạn tên gì? "
        nhập số nguyên tuổi với lời nhắc "Bao nhiêu tuổi? "

        nếu tuổi lớn hơn hoặc bằng 18 thì {
            in("Chào {tên}, bạn đã trưởng thành.")
        } không thì {
            in("Chào {tên}, còn {18 trừ tuổi} năm nữa.")
        }

        in("Tổng: {tổng([tuổi, 1, 2])}")
    }
}
```

Chương trình được dịch sang Java rồi chạy. Lỗi luôn được báo theo mã tiếng Việt bạn viết, không theo mã Java sinh ra.

## Ý tưởng

- **Nhiều cách nói, một ý nghĩa.** `Khi`, `Mỗi lúc`, `trong khi` đều là vòng lặp; `lớn hơn`, `trên`, `>` đều là phép so sánh. Viết kiểu nào cũng ra cùng một chương trình.
- **Từ đệm cho câu văn tự nhiên.** `Mỗi lúc giá trị a lớn hơn giá trị của b thì { ... }` và `Khi a > b { ... }` là một.
- **Chữ hay ký hiệu đều được**, và trộn lẫn cũng được: `trả về a cộng b` hoặc `-> a + b`.
- **Tên có dấu, nhiều từ:** `điểm trung bình`, `tính tổng(...)`.
- **Luôn xác định.** Không có phỏng đoán hay AI trong trình dịch: cùng một mã nguồn và cùng một cấu hình luôn cho cùng một kết quả.

## Dùng thử

Cần [Rust](https://rustup.rs) để biên dịch và Java 11 trở lên để chạy chương trình.

```sh
cargo build --release
./target/release/anature run vi-du/chao.agn      # dịch rồi chạy
./target/release/anature build vi-du/chao.agn    # chỉ sinh file .java để xem
```

Thư mục `vi-du/` có sẵn vài chương trình mẫu.

## Trong VS Code

Thư mục `vscode/` là phần mở rộng cho VS Code: gợi ý khi gõ, báo lỗi ngay trong lúc soạn, tô màu, và nút chạy tệp `.agn`.

```sh
cd vscode
npm install
npm run package
code --install-extension anature-linux-x64.vsix
```

Chi tiết ở [vscode/README.md](vscode/README.md).

## Đổi từ vựng

Mở `language.toml` và thêm cách nói của bạn:

```toml
[keywords]
WHILE = ["khi", "mỗi lúc", "mỗi khi", "trong khi", "chừng nào"]
```

Từ lúc đó `chừng nào a lớn hơn 0 { ... }` là một vòng lặp hợp lệ, và VS Code gợi ý, tô màu nó ngay. File cấu hình được dùng là `language.toml` gần nhất tính từ thư mục của tệp nguồn đi ngược lên.

## Tài liệu

- [thiet-ke-ngon-ngu-cau-hinh.md](thiet-ke-ngon-ngu-cau-hinh.md): thiết kế đầy đủ, bảng cú pháp, và các giới hạn hiện tại.
- [language.toml](language.toml): toàn bộ từ vựng và luật, có chú thích.

## Trạng thái

Dự án đang ở giai đoạn thử nghiệm. Ngôn ngữ đã có biến, hàm, điều kiện, vòng lặp, danh sách, chuỗi và nhập xuất; chưa có lớp với đối tượng hay chương trình nhiều tệp.
