# Thiết kế hệ thống ngôn ngữ lập trình với cú pháp cấu hình được

**Phiên bản:** 2.8
**Trạng thái:** Đã triển khai lõi (Rust → Java) và hỗ trợ soạn thảo (LSP, VS Code)
**Phạm vi:** Đặc tả kiến trúc cho một trình biến đổi nguồn-sang-nguồn (source-to-source) cho phép người dùng tự định nghĩa từ vựng và luật ngữ pháp bề mặt của ngôn ngữ.

Phiên bản 1.0 là bản thiết kế trước khi viết code. Phiên bản này mô tả hệ thống như nó đang chạy; những chỗ khác với 1.0 được liệt kê ở Mục 14.

---

## 1. Tổng quan

### 1.1. Mục tiêu

Hệ thống cho phép người dùng viết chương trình bằng một tập từ vựng do chính họ (hoặc người bảo trì ngôn ngữ) định nghĩa, sau đó biến đổi mã nguồn đó sang Java để thực thi. Mục tiêu là làm cho mã nguồn đọc gần với tiếng Việt tự nhiên, trong khi vẫn giữ các tính chất bắt buộc của một ngôn ngữ lập trình.

```
Lớp TínhToán {
    Hàm tổng(số nguyên a và số nguyên b) trả về số nguyên {
        trả về a cộng b
    }

    Hàm gốc {
        đặt biến kết_quả là tổng(2 và 3)
        Mỗi lúc giá trị kết_quả lớn hơn 0 thì {
            in ra màn hình "còn " cộng kết_quả
            kết_quả là kết_quả trừ 1
        }
    }
}
```

### 1.2. Nguyên tắc thiết kế

Ba nguyên tắc chi phối mọi quyết định trong tài liệu này:

1. **Tính xác định (determinism).** Cùng một đầu vào và cùng một file cấu hình phải luôn cho ra cùng một kết quả. Không có thành phần nào ở thời điểm chạy được phép trả về kết quả gần đúng hoặc theo xác suất.
2. **Khả năng báo lỗi chính xác.** Mọi lỗi phải trỏ về đúng vị trí trong mã nguồn gốc của người dùng, không phải vị trí trong mã đã sinh ra. Điều này áp dụng cho cả lỗi cú pháp lẫn lỗi do Java báo khi biên dịch và khi chạy.
3. **Từ vựng là tập đóng.** Người dùng chọn cách diễn đạt trong một tập đã định nghĩa trước, chứ không phát minh ngữ pháp mới một cách tùy ý. Sự tự do nằm ở *phong cách diễn đạt*, không ở *cấu trúc ngữ pháp*.

### 1.3. Phân loại hệ thống

Hệ thống **không phải** là một trình biên dịch sinh mã máy. Nó là một transpiler viết bằng Rust, phát ra mã Java. Độ phức tạp nằm ở tầng phân tích cú pháp bề mặt, không ở phần sinh mã hay tối ưu hóa. Việc kiểm tra kiểu được giao lại cho `javac`.

### 1.4. Hai mức sử dụng

| Lệnh | Dành cho | Kết quả |
|---|---|---|
| `anature build <tệp.agn>` | Người phát triển ngôn ngữ, khi cần debug | Ghi file `.java` cạnh tệp nguồn, mỗi câu lệnh kèm ghi chú `// dòng N` |
| `anature run <tệp.agn>` | Người dùng | Dịch rồi chạy luôn; không để lại file, mọi thông báo của Java được quy về mã nguồn gốc |
| `anature lsp` | Trình soạn thảo | Máy chủ gợi ý, báo lỗi và tô màu (Mục 11) |

---

## 2. Kiến trúc tổng thể

Hệ thống vận hành theo hai pha tách biệt rõ ràng, phân biệt theo việc *có con người hiện diện hay không*.

### 2.1. Pha soạn thảo (authoring-time)

Có người dùng hiện diện. Cho phép các thành phần trợ giúp mang tính gợi ý: gợi ý khi gõ, thông báo "có phải bạn muốn..." (xem Mục 11).

### 2.2. Pha thực thi (run-time)

Không có người dùng hiện diện. Chỉ gồm các thao tác xác định: tra bảng cấu hình, phân tích cú pháp, sinh mã đích.

**Ranh giới giữa hai pha là bất biến kiến trúc quan trọng nhất của hệ thống.** Mọi thành phần gần đúng hoặc theo xác suất chỉ được tồn tại trong pha soạn thảo.

### 2.3. Luồng xử lý

```
mã nguồn thô
  → [Chuẩn hóa Unicode]  đưa toàn bộ văn bản về dạng NFC
  → [Lexer]              tách token, giữ vị trí (span); gộp đồng nghĩa về token nội bộ
  → [Parser]             áp luật trong cấu hình; loại từ đệm theo vị trí
  → [Cây trung gian]     chỉ chứa token nội bộ, không còn từ tiếng Việt của ngôn ngữ
  → [Kiểm tra ngữ nghĩa] tên đã khai báo chưa, hàm gọi đúng chưa, kiểu có khớp không
  → [Sinh mã]            phát mã Java theo bảng ánh xạ cuối; đổi tên sang ASCII
  → mã Java
  → [Dịch thông báo]     (chỉ khi run) quy lỗi của java/javac về mã nguồn gốc
```

Ánh xạ sang module trong `src/`:

| Module | Vai trò |
|---|---|
| `config.rs` | Đọc `language.toml`, kiểm tra mâu thuẫn, dựng bảng tra cứu và luật |
| `lexer.rs` | Tách token, khớp cụm dài nhất, gộp đồng nghĩa |
| `parser.rs` | Bộ thông dịch luật kiểu PEG và bộ phân tích biểu thức |
| `semantics.rs` | Kiểm tra tên và kiểu trên cây trung gian (Mục 7) |
| `codegen.rs` | Sinh Java từ cây trung gian, quản lý bảng tên |
| `report.rs` | Dịch thông báo của Java về mã nguồn gốc |
| `lsp.rs` | Pha soạn thảo: máy chủ LSP cho gợi ý, chẩn đoán lỗi và tô màu |
| `error.rs` | Span và cách trình bày lỗi |
| `main.rs` | Dòng lệnh `build` / `run` |

Lõi Rust không chứa một từ tiếng Việt nào của ngôn ngữ, và cũng không chứa mã Java nào ngoài danh sách từ khóa Java cần né khi đặt tên.

---

## 3. Mô hình từ vựng

### 3.1. Các loại thành phần

Mọi cụm trong mã nguồn được phân vào một trong các loại sau, mỗi loại là một bảng riêng trong cấu hình.

| Loại | Bảng | Vai trò | Ví dụ |
|---|---|---|---|
| **Từ khóa** | `[keywords]` | Mang nghĩa cấu trúc lệnh | `khi`, `nếu`, `là`, `trả về` |
| **Kiểu** | `[types]` | Kiểu dữ liệu | `số nguyên`, `chữ`, `luận lý` |
| **Hằng** | `[literals]` | Giá trị cố định trong biểu thức | `đúng`, `sai` |
| **Toán tử** | `[operators]` | Phép toán, có độ ưu tiên | `cộng`, `lớn hơn`, `và` |
| **Từ đệm** | `[fillers]` | Không mang nghĩa, tăng tính tự nhiên | `thì`, `giá trị của`, `màn hình` |
| **Chỗ trống** | — | Dữ liệu người dùng: tên, số, chuỗi | `kết_quả`, `42`, `"xin chào"` |

Nguyên tắc: thành phần có rủi ro nhận sai càng cao thì xử lý càng cứng. Mọi cụm đều được tra bảng chính xác, không bao giờ qua suy đoán.

### 3.2. Đồng nghĩa và token nội bộ

Nhiều cách diễn đạt bề mặt ánh xạ về một token nội bộ. `Khi`, `Mỗi lúc`, `Mỗi khi`, `trong khi` cùng là `WHILE`; `lớn hơn`, `trên`, `>` cùng là `GT`.

**Ký hiệu là một cách viết đồng nghĩa như mọi cách viết khác.** Một cụm trong cấu hình hoặc gồm toàn chữ, hoặc gồm toàn ký hiệu; cả hai loại nằm chung một danh sách của nhóm. Vì vậy `trả về` và `->` là cùng một token `RETURN`, và người viết có thể dùng chữ, dùng ký hiệu, hoặc trộn cả hai trong cùng một tệp (Mục 10.2).

Từ khóa và toán tử nằm ở hai bảng riêng vì toán tử cần thêm độ ưu tiên và vị trí (đứng giữa hai toán hạng, đứng trước một toán hạng, hoặc cả hai):

```toml
[operators]
ADD = { words = ["cộng", "+"], prec = 5 }
SUB = { words = ["trừ", "-"], prec = 5, prefix = true }
NOT = { words = ["không phải", "!"], prec = 7, prefix = true, infix = false }
```

### 3.3. Một cụm, một nhóm

**Một cụm chỉ được thuộc đúng một nhóm.** Cấu hình vi phạm bị từ chối ngay khi nạp, với thông báo nêu rõ hai nhóm tranh chấp. Đây là cách hệ thống tránh nhập nhằng ở tầng từ vựng mà không cần ngữ cảnh.

Hệ quả cụ thể: `bằng` là từ gán (`ASSIGN`), nên phép so sánh bằng phải viết `bằng với`, `giống` hoặc `==`.

Có ba ngoại lệ, đều có chủ đích:

- Một từ đệm có thể thuộc nhiều nhóm đệm (xem 3.4).
- Một cụm ngăn cách có thể trùng với một toán tử (xem 3.6).
- Một từ khóa có thể đóng vai một toán tử khi đứng giữa biểu thức, khai báo ở `[expression] keyword_operators` (xem 6.5): `bằng` mở đầu phép gán nhưng giữa biểu thức là so sánh; `thêm` mở đầu câu là lệnh thêm vào danh sách nhưng `a thêm 1` là phép cộng. Khi đã có ánh xạ, ghi cụm ở cả hai nhóm cũng hợp lệ. Khi chưa có, thông báo lỗi chỉ luôn dòng cần thêm.

Một cụm chữ có thể xen số ở các từ phía sau (`in 1 dòng`), miễn là từ đầu là chữ.

### 3.4. Từ đệm theo vị trí

Không có danh sách từ đệm dùng chung. Từ đệm được chia thành **các nhóm có tên**, và mỗi nhóm chỉ có hiệu lực tại vị trí mà một luật nhắc tới nó:

```toml
[fillers]
TOAN_HANG = ["giá trị", "giá trị của", "của"]   # trước một toán hạng
THI       = ["thì"]                             # sau điều kiện
SAU_IN    = ["màn hình"]                        # sau lệnh in
```

```
(WHILE) <điều_kiện:biểu_thức> [THI]? <thân:khối>
(PRINT) [SAU_IN]* <giá_trị:biểu_thức>
```

Từ đệm xuất hiện sai vị trí là lỗi cú pháp, không bị lặng lẽ bỏ qua: `đặt biến a thì là 5` bị từ chối.

Riêng biểu thức có một nhóm đệm được bỏ qua trước mỗi toán hạng, khai báo ở `[expression] operand_fillers`. Nhờ đó `giá trị a lớn hơn giá trị của b` đọc thành `a > b`.

**Đệm trước tên.** Nhóm khai báo ở `[expression] name_fillers` được bỏ qua ngay trước mọi cái tên, ở cả chỗ khai báo lẫn chỗ dùng: `nhập số nguyên cho biến tuổi`, `danh sách số nguyên tên ds`, `in(biến tuổi)`. Hai quy tắc đi kèm:

- **Từ đệm chỉ là đệm khi có tên theo sau; đứng một mình thì chính nó là tên.** `nhập chữ tên` tạo biến «tên», còn `nhập chữ tên học sinh` tạo biến «học sinh». Nhờ vậy thêm `tên` và `biến` vào nhóm đệm không lấy mất hai cái tên hay dùng nhất.
- **Một từ khóa cũng có thể được khai là đệm trước tên** (`cho biến`): nó mở đầu câu lệnh như thường, và chỉ thành đệm khi đứng ở vị trí của một cái tên và ngay sau nó là tên. Nó không bao giờ tự mình thành tên.

**Đệm giữa tên hàm và danh sách tham số.** `[lists] before_arguments` liệt kê các nhóm (từ khóa hoặc đệm) được phép chen giữa tên hàm và dấu `(`, cả khi khai báo lẫn khi gọi: `Hàm tổng từ (số nguyên a, số nguyên b)`, `tổng với tham số (2, 3)`. Từ `từ` vẫn là từ khóa của vòng lặp; ở vị trí này nó chỉ được bỏ qua.

Việc loại từ đệm thuộc về parser, không phải một bước tiền xử lý. Lexer chỉ gắn cho token danh sách các nhóm đệm mà nó thuộc về.

### 3.5. Từ ngăn cách

Một tên kết thúc khi gặp một cụm của ngôn ngữ. Trong `đặt biến tổng điểm là 123`, từ `là` thuộc nhóm từ khóa `ASSIGN` (cùng `bằng`, `có giá trị`, `=`). Nó không phải từ đệm: bỏ đi thì parser không biết phần tên dừng ở đâu.

### 3.6. Cụm ngăn cách trong danh sách

`và` tương đương dấu phẩy khi đứng ngay trong ngoặc của hàm, cả lúc khai báo lẫn lúc gọi, và trong danh sách `[...]`:

```
tổng(2 và 3)                  →  tong(2, 3)
nếu f(a) và g(b)              →  if (f(a) && g(b))
kiểm((a và b)), kiểm(a && b)  →  phép "và" logic bên trong danh sách đối số
```

Các cụm này khai báo ở `[lists] separators`. Ngoài danh sách, hoặc bên trong một cặp ngoặc con, cụm trở lại nghĩa toán tử của nó.

### 3.7. Tên

- **Không phân biệt hoa/thường**, cho cả từ vựng lẫn tên do người dùng đặt. `Tổng`, `tổng`, `TỔNG` là một tên.
- **Mọi cụm trong cấu hình là từ dành riêng**: `trên`, `của`, `in`, `chia`, `từ`... không dùng làm tên được.
- **Tên có thể gồm nhiều từ**: `tổng điểm`, `điểm trung bình`, `tính tổng(...)`. Các từ liền nhau không thuộc cụm nào của ngôn ngữ được gộp thành một tên, và tên dừng lại ở cụm đầu tiên của ngôn ngữ. Cái giá: không cụm nào của ngôn ngữ được nằm trong tên. `danh sách điểm` không phải một tên, vì `danh sách` là từ khóa; `số nguyên tố` bị đọc thành kiểu `số nguyên` theo sau là tên `tố`.
- **Tên được đổi sang ASCII khi sinh Java** (xem 8.3): `tổng điểm` thành `tong_diem`.

---

## 4. Tầng phân tích từ vựng (Lexer)

### 4.1. Chuẩn hóa

Trước khi tách token, toàn bộ mã nguồn được đưa về Unicode NFC. Tiếng Việt có hai cách mã hóa cùng một chữ (dựng sẵn và tổ hợp); không chuẩn hóa thì việc tra bảng sẽ trượt một cách khó hiểu. Khi so khớp, cụm được đưa về chữ thường và các từ cách nhau đúng một dấu cách.

### 4.2. Hai bước

**Bước 1 — cắt mảnh thô.** Chuỗi ký tự `"..."`, ký tự `'x'`, số, chú thích `//`, từ, và ký hiệu được nhận diện ở đây. Vì vậy nội dung chuỗi không bao giờ bị tra bảng từ khóa: trong `in("Khi nào xong?")`, từ `Khi` được giữ nguyên.

**Bước 2 — gộp cụm.** Với mỗi dãy từ liền nhau, lexer thử khớp cụm dài nhất có trong bảng trước khi lùi về cụm ngắn hơn: `lớn hơn hoặc bằng` được xét trước `lớn hơn`, `số nguyên lớn` trước `số nguyên`. Từ không khớp cụm nào trở thành tên.

Do việc khớp diễn ra trên từ nguyên vẹn, từ khóa không bao giờ khớp vào giữa một tên: `Khi` không khớp trong `Khiem`. Các từ liền nhau không khớp cụm nào được gộp thành một tên nhiều từ (Mục 3.7).

**Chuỗi nội suy.** Trong một chuỗi `"..."`, phần nằm giữa `{` và `}` là một biểu thức: `"Xin chào {tên}, {tuổi cộng 1} tuổi"`. Lexer tách chuỗi thành các mảnh chữ và các dãy token riêng cho từng biểu thức, với span vẫn tính theo tệp gốc, nên lỗi bên trong `{}` trỏ đúng vào trong chuỗi. Viết `\{` và `\}` để có dấu ngoặc nhọn thật. Biểu thức trong `{}` không được chứa dấu `"`.

Cụm ký hiệu cũng theo quy tắc dài nhất trước: `<=` được xét trước `<`, `+=` trước `+`, `:=` trước `:`, `->` trước `-`. Ký hiệu không cần dấu cách ngăn với chữ hay số bên cạnh (`a<=-1` đọc là `a <= -1`), và `..` không lẫn với dấu chấm thập phân (`1.5..n`).

### 4.3. Thông tin vị trí (span)

Mỗi token mang theo dòng, cột và khoảng byte của nó trong mã nguồn. Thông tin này đi xuyên suốt các tầng sau và là cơ sở cho việc báo lỗi (Mục 9).

### 4.4. Kết thúc câu lệnh

Xuống dòng kết thúc câu lệnh; dấu `;` cũng vậy, nên viết được hai lệnh trên một dòng. Như Go, nếu dòng kết thúc bằng dấu phẩy, một toán tử, `(` hoặc `[` thì câu lệnh còn tiếp ở dòng sau.

Parser nới thêm ở những chỗ xuống dòng không thể mang nghĩa nào khác:

- **Trước `{`**, vì không câu lệnh nào mở đầu bằng nó.
- **Trước một từ khóa giữa luật mà không mở đầu câu lệnh nào**, như `không thì`. Từ khóa có mở đầu một câu lệnh (như `trả về`) thì không được nới, để hai lệnh liền nhau vẫn là hai lệnh.
- **Bên trong ngoặc** của lời gọi, danh sách, và chỉ số: `)` hay `]` được nằm ở dòng riêng, và dấu phẩy cuối được chấp nhận.

Chú thích viết bằng `//` đến hết dòng, hoặc `/* ... */` qua nhiều dòng.

---

## 5. Dạng trung gian

Ánh xạ không đi thẳng từ bề mặt sang mã đích:

```
Bề mặt     →   Token nội bộ   →   Mã đích
"Khi"          WHILE              "while"
"Mỗi lúc"      WHILE              "while"
```

Cây trung gian mà parser tạo ra chỉ chứa tên luật, token nội bộ và dữ liệu của người dùng. Mọi câu biến thể cùng nghĩa cho ra cùng một cây, do đó cùng một mã Java. Bộ kiểm thử xác nhận điều này trực tiếp.

Toàn bộ phần phụ thuộc Java nằm trong các bảng `[java.*]` của cấu hình. Đổi ngôn ngữ đích là thay các bảng đó; lexer và parser không đổi.

---

## 6. Tầng phân tích cú pháp (Parser)

### 6.1. Mô hình

Luật ngữ pháp nằm trong file cấu hình, không viết cứng trong lõi. Lõi là một bộ thông dịch luật theo mô hình **PEG**: các luật được thử theo đúng thứ tự trong file, luật khớp trước thắng. Thứ tự là một phần của ngữ pháp, nên không có nhập nhằng.

Một luật chỉ được coi là khớp khi nó khớp **trọn đến hết câu lệnh**. Nhờ vậy `nếu ... { } không thì { }` không bị luật `IF` ngắn hơn nhận nhầm rồi bỏ lại phần `không thì`.

**Kết quả phân tích một khối được nhớ lại theo vị trí.** Nhiều luật mở đầu giống nhau (ba dạng của `nếu`) lần lượt được thử; không có bước này, mỗi luật phân tích lại toàn bộ thân, và `nếu` lồng nhau làm thời gian dịch nhân ba sau mỗi tầng (14 tầng mất 3,5 giây). Nay 30 tầng dịch trong dưới một mili giây.

### 6.2. Cú pháp mô tả luật

| Ký hiệu | Nghĩa |
|---|---|
| `(NHÓM)` | Bắt buộc một từ khóa hoặc một kiểu thuộc nhóm |
| `[NHÓM]?` | Có hoặc không; nhóm từ khóa, kiểu hoặc đệm |
| `[NHÓM]*` | Không hoặc nhiều |
| `[ ] ( ) { } ,` | Một ký hiệu viết nguyên văn, đứng riêng một mình |
| `<tên:loại>` | Một chỗ trống |

Các loại chỗ trống:

| Loại | Khớp với |
|---|---|
| `tên` | Một tên biến, hàm hoặc lớp |
| `biểu_thức` | Một biểu thức |
| `kiểu` | Một kiểu trong `[types]`, hoặc kiểu danh sách |
| `tham_số` | `(kiểu tên, kiểu tên, ...)`; bỏ cả ngoặc nếu không có tham số |
| `lời_gọi` | `tên(đối số, ...)` |
| `khối` | `{ các lệnh scope "thân" }` |
| `khối_lớp` | `{ các lệnh scope "lớp" }` |
| `lệnh` | Một lệnh scope "thân" |

### 6.3. Phạm vi của luật

Mỗi luật khai báo nơi nó được phép xuất hiện:

| `scope` | Vị trí |
|---|---|
| `tệp` | Ngoài cùng của tệp |
| `lớp` | Trong một `khối_lớp` |
| `thân` | Trong một `khối` |
| `biểu_thức` | Ở vị trí một giá trị; phải mở đầu bằng một `(TỪ_KHÓA)` |

### 6.4. Ví dụ

```toml
[[rules]]
name    = "WHILE"
scope   = "thân"
pattern = "(WHILE) <điều_kiện:biểu_thức> [THI]? <thân:khối>"

[java.rules]
WHILE = "while ({điều_kiện}) {thân}"
```

Luật này chấp nhận cả ba câu sau và cho ra cùng một kết quả:

```
Khi a lớn hơn b { ... }
Mỗi lúc giá trị a lớn hơn giá trị của b thì { ... }
trong khi a > b { ... }
```

Trong câu thứ hai: `Mỗi lúc` khớp `(WHILE)`; `giá trị` và `giá trị của` là đệm trước toán hạng, do bộ phân tích biểu thức bỏ qua; `thì` khớp `[THI]?`; phần `{ ... }` điền vào `<thân>`.

### 6.5. Biểu thức

Biểu thức được phân tích bằng phương pháp leo thang độ ưu tiên (precedence climbing), với độ ưu tiên lấy từ `[operators]`. Các dạng toán hạng do lõi hiểu sẵn: số, chuỗi (kể cả chuỗi nội suy), ký tự, hằng, tên, lời gọi hàm, ngoặc đơn, danh sách `[a, b]` và chỉ số `a[i]`. Ngoài ra, mọi luật scope `biểu_thức` đều dùng được như một toán hạng; lệnh `nhập số nguyên` được định nghĩa theo cách này.

**Lấy phần tử bằng chữ.** `ds thứ i` và `phần tử thứ i của ds` được đưa về đúng dạng `ds[i]` trong cây trung gian, nên mọi thứ gắn với `ds[i]` (kiểu của phần tử, lấy kí tự của chữ, so sánh theo giá trị) áp dụng y nguyên. Vị trí sau `thứ` là một toán hạng đơn: `ds thứ i cộng 1` là `(ds thứ i) cộng 1`; muốn tính vị trí thì viết `ds thứ (i cộng 1)`. Cách viết đầy đủ là một luật scope `biểu_thức` có khai `index = { of, at }`; loại chỗ trống `toán_hạng` được thêm để luật mô tả được "một toán hạng đơn".

Hai điều chỉnh để biểu thức đọc như tiếng Việt:

- **Từ gán giữa biểu thức là phép so sánh.** `[expression] keyword_operators = { ASSIGN = "EQ" }` cho phép `nếu a bằng b` và `nếu a là 3`. Không nhập nhằng, vì trong mọi luật, từ gán chỉ đứng sau một cái tên hoặc sau `]`, không bao giờ sau một biểu thức.
- **Phủ định ôm cả phép so sánh.** Toán tử đứng trước có thể khai `prefix_prec`: `không phải a nhỏ hơn b và c` đọc là `(không phải (a nhỏ hơn b)) và c`. Dấu trừ một ngôi không khai, nên vẫn chỉ ôm đúng một toán hạng.

### 6.6. Kiểm tra cấu hình khi nạp

Vì PEG che nhập nhằng bằng thứ tự chứ không báo, mọi mâu thuẫn phát hiện được đều bị chặn lúc nạp cấu hình:

- Một cụm thuộc hai nhóm; một tên nhóm khai báo ở hai bảng.
- Luật nhắc tới nhóm hoặc loại chỗ trống không tồn tại.
- Luật có thể gọi lại chính nó vô hạn (chỗ trống `lệnh` đứng trước mọi thành phần bắt buộc).
- Luật, toán tử, hằng hoặc kiểu chưa có ánh xạ sang Java; mẫu Java nhắc tới chỗ trống mà luật không có.

---

## 7. Kiểm tra ngữ nghĩa

Sau khi có cây trung gian, lõi kiểm tra tên và kiểu trước khi sinh mã. Lỗi ở tầng này được báo bằng tiếng Việt, có dòng và cột, và hiện ngay khi gõ trong trình soạn thảo; trước đây chúng chỉ lộ ra khi `javac` chạy.

### 7.1. Luật tự khai ý nghĩa của mình

Lõi không biết luật nào khai báo biến, luật nào định nghĩa hàm. Mỗi luật trong cấu hình gắn thêm phần ngữ nghĩa, với giá trị là tên các chỗ trống của chính luật đó:

| Khóa | Nghĩa | Ví dụ |
|---|---|---|
| `declare = { name, type?, value?, element_of? }` | Khai báo một biến; kiểu lấy từ `type`, từ phần tử của `element_of`, hoặc suy từ `value` | `đặt biến`, `lặp`, `với mỗi` |
| `function = { name?, params?, returns? }` | Định nghĩa hàm; thân là một phạm vi biến riêng | `Hàm ...`, `Hàm gốc` |
| `assign = { name, value }` | Gán cho biến đã có | `a là 5` |
| `defines = [..]` | Chỗ đặt tên mới, không phải nơi dùng biến | tên lớp |
| `gives = ".."` | Giá trị trả về cho hàm đang chứa nó | `trả về x` |
| `result = "KIỂU"` | Kiểu của một luật scope `biểu_thức` | `nhập số nguyên` |

Mọi chỗ trống loại `tên` không được nhắc tới trong `declare`, `function`, `defines` đều là nơi **dùng** một biến, nên biến đó phải đã được khai báo.

```toml
[[rules]]
name    = "FOR_EACH"
scope   = "thân"
pattern = "(EACH) <biến:tên> (IN) <danh_sách:biểu_thức> <thân:khối>"
declare = { name = "biến", element_of = "danh_sách" }
```

Mục `[semantics]` cho lõi biết tên của vài kiểu (kiểu của `42`, của `"chữ"`, của `đúng`), thứ tự rộng dần của các kiểu số, và toán tử nào cho kết quả đúng/sai. Bỏ cả mục này đi thì lõi không kiểm tra ngữ nghĩa nữa.

### 7.2. Những gì được kiểm tra

- **Biến chưa khai báo**, kể cả ở vế phải của chính câu khai báo nó (`đặt a là a cộng 1`) và bên trong `{}` của chuỗi nội suy.
- **Biến khai báo lại** trong cùng một hàm, kèm số dòng của lần khai báo trước.
- **Hàm chưa định nghĩa** và **sai số lượng đối số**. Hàm gọi được hàm định nghĩa phía dưới nó.
- **Sai kiểu** khi truyền đối số, khi gán, khi khai báo có kiểu, và khi trả về; kể cả trả về giá trị trong hàm không khai báo kiểu trả về.
- **Danh sách trộn kiểu**: `[9, 7.5]` bị từ chối; viết `[9.0, 7.5]`.
- **So sánh hai kiểu không liên quan**: `chữ bằng với kí tự`, `1 < a < 5`.
- **Lấy phần tử theo vị trí** trên thứ không phải danh sách hay chữ.
- **Vòng đếm lên có hai mốc là số mà mốc đầu lớn hơn** (`lặp k từ 3 đến 1`): sẽ không chạy lần nào, nên báo lỗi kèm lời khuyên viết `lùi về`. Mốc tính ra lúc chạy thì không đoán.
- **Chương trình không có `Hàm gốc`.**

Biến khai báo ở cấp lớp (`đặt số thực số dư là 0`, phải có kiểu) được mọi hàm nhìn thấy, kể cả hàm viết phía trên nó.

Phạm vi theo đúng Java: mỗi hàm có bộ biến riêng, biến của vòng lặp chỉ sống trong khối của nó. Số nguyên đặt được vào chỗ cần số nguyên lớn hoặc số thực, không có chiều ngược lại.

```
lỗi: đối số thứ 1 của «gấp đôi» phải là số nguyên, nhưng nhận chữ
 --> sai.agn:7:31
  |
7 |         đặt biến tổng điểm là gấp đôi("năm")
  |                               ^^^^^^^
```

Lỗi cú pháp dừng ở lỗi đầu tiên; lỗi ngữ nghĩa được gom lại và báo cả lượt.

### 7.3. Không biết thì không báo

Kiểu không suy ra được (danh sách rỗng, kết quả của một phép tính trên kiểu lạ) được coi là "chưa biết", và một giá trị chưa biết kiểu không bao giờ sinh lỗi. Thà bỏ sót để `javac` bắt còn hơn báo sai một chương trình đúng. `javac` vẫn là lưới cuối cùng (Mục 9.2).

### 7.4. Toán tử theo kiểu

Biết kiểu còn để sinh mã đúng. Trong Java, `==` trên chữ so sánh địa chỉ chứ không so nội dung, nên `tên bằng với "An"` từng cho kết quả sai mà không báo gì. Bảng `[java.operators]` nay có thể chứa biến thể theo kiểu của toán hạng, viết `TOÁN_TỬ.KIỂU`:

```toml
EQ           = "=="
"EQ.STRING"  = "Objects.equals({a}, {b})"
"NE.STRING"  = "!Objects.equals({a}, {b})"
LEN          = "{a}.size()"
"LEN.STRING" = "{a}.length()"
```

Tầng kiểm tra chọn biến thể khi một toán hạng có kiểu tương ứng; nhờ vậy `độ dài` dùng được cho cả danh sách lẫn chữ. Cùng cơ chế này cho `tên[0]` (`INDEX.STRING`), so sánh thứ tự chữ (`LT.STRING`), và so sánh hai danh sách (`EQ.LIST`).

Ba trường hợp mà Java cho kết quả sai một cách im lặng được xử lý bằng biến thể:

- **Hai số cùng lấy từ danh sách.** `ds[0] bằng với ds[1]` với hai số 1000 từng ra *khác*, vì Java so địa chỉ của hai "hộp" chứa số. Tầng kiểm tra theo dõi giá trị nào có thể đang nằm trong hộp (phần tử danh sách, biến của `với mỗi`, biến gán từ chúng); khi cả hai vế đều thế, nó chọn `EQ.BOXED = "(+{a}) == (+{b})"`. Khi một vế là số trần, Java tự mở hộp, nên mã vẫn là `==` cho dễ đọc.
- **Tràn số.** Phép cộng, trừ, nhân trên số nguyên dùng `ADD.INT = "Math.addExact({a}, {b})"`...: tràn thì chương trình dừng với "tràn số" thay vì ra số âm vô nghĩa. Biến thể của phép tính số học được chọn theo kiểu của *kết quả*, vì số nguyên cộng số thực phải dùng phép cộng thường. Phép gán kèm (`+=`) và bước đếm của vòng lặp chưa được bảo vệ.
- **Số nguyên quá lớn.** `8000000000` được hiểu là số nguyên lớn và sinh ra `8000000000L`.

---

## 8. Sinh mã Java

### 8.1. Mẫu

Mỗi luật có một mẫu trong `[java.rules]`; `{tên_chỗ_trống}` được thay bằng mã đã sinh của chỗ trống đó. Toán tử thường chỉ là một ký hiệu (`GT = ">"`), nhưng có thể là một mẫu với `{a}` và `{b}` khi Java cần hình dạng khác (`LEN = "{a}.size()"`).

Biểu thức con luôn được đóng ngoặc khi sinh mã. Thứ tự ưu tiên trong cấu hình có thể khác của Java, nên đây là cách duy nhất bảo đảm Java đọc biểu thức đúng như parser đã hiểu.

### 8.2. Cấu trúc chương trình

Chương trình viết lớp và hàm gốc tường minh: `Lớp A { Hàm gốc { ... } }`. Tên lớp quyết định tên file `.java`. Cấu hình có thể khai báo phần đầu file (`header`, hiện là `import java.util.*;`) và phần cuối file (`footer`, hiện là một lớp phụ `Anature` giữ bộ đọc bàn phím).

### 8.3. Bảng tên

Tên có dấu được bỏ dấu khi sinh Java (`tổng` → `tong`, `đếm` → `dem`). Hai tên nguồn khác nhau không bao giờ ra cùng một tên Java: tên đến sau mà trùng thì được gắn số (`Tổng` → `Tong`, `Tông` → `Tong_2`). Tên trùng từ khóa Java, hoặc trùng tên trong `[java] reserved`, cũng được gắn số (`class` → `class_2`).

File Java sinh ra có bảng đối chiếu ở đầu:

```java
// Tên đã đổi sang ASCII:
//   TínhToán -> TinhToan
//   Tông -> Tong_2
```

### 8.4. Kiểu

| Viết | Java | Trong danh sách |
|---|---|---|
| `số nguyên` | `int` | `Integer` |
| `số nguyên lớn` | `long` | `Long` |
| `số thực` | `double` | `Double` |
| `chữ`, `chuỗi` | `String` | `String` |
| `kí tự`, `ký tự` | `char` | `Character` |
| `luận lý`, `đúng sai` | `boolean` | `Boolean` |
| `danh sách <kiểu>`, `mảng <kiểu>` | `ArrayList<...>` | lồng nhau được |

Biến khai báo không kèm kiểu (`đặt biến a là 5`) dùng `var` để Java tự suy kiểu. Tham số và kiểu trả về của hàm phải khai kiểu.

### 8.5. Ghi chú số dòng

Mỗi câu lệnh Java kết thúc dòng đầu tiên của nó bằng `// dòng N`, với N là dòng tương ứng trong mã nguồn gốc. Ghi chú này vừa giúp người đọc file Java dò ngược, vừa là bảng tra mà tầng dịch thông báo dùng (Mục 9.2).

---

## 9. Xử lý lỗi

### 9.1. Lỗi cú pháp

Parser ghi nhớ vị trí xa nhất mà nó từng thất bại và tập những gì hợp lệ tại đó. Khi không luật nào khớp, thông báo trỏ vào đúng vị trí ấy và liệt kê các khả năng:

```
lỗi: ở đây cần một trong: «là», «bằng», «có giá trị», «=» — nhưng gặp «2»
 --> vi-du.agn:4:12
  |
4 | đặt biến b 2
  |            ^
```

Tập "những gì hợp lệ tại đây" là hoàn toàn xác định, và cũng chính là nguồn của bộ gợi ý khi gõ ở Mục 11.

Khi một cái tên bị một cụm của ngôn ngữ cắt ngang, hoặc có số đứng riêng trong tên, thông báo nói rõ:

```
lỗi: ở đây cần một trong: «là», «bằng», «có giá trị», «=», «:=» — nhưng gặp «đúng»
«đúng» là hằng (TRUE) của ngôn ngữ; nếu «đã đoán đúng» là một tên thì hãy đổi tên, vì tên không được chứa cụm này
```

Khi chỗ lỗi đang chờ một cái tên mà lại gặp một cụm của ngôn ngữ, thông báo cũng nói rõ lý do, vì danh sách từ dành riêng dài và người viết khó nhớ hết:

```
lỗi: ở đây cần một trong: kiểu dữ liệu, tên — nhưng gặp «chia»
«chia» là toán tử (DIV) của ngôn ngữ nên không dùng làm tên được
```

### 9.2. Lỗi do Java báo

Khi chạy bằng `run`, đầu ra lỗi của `java` được chặn lại và dịch từng dòng trước khi in:

- **Số dòng** được quy từ file Java về tệp nguồn; phần `javac` chép lại mã Java bị bỏ.
- **Tên** được đổi ngược từ dạng ASCII về tên người dùng đã viết.
- **Câu chữ** của các thông báo quen thuộc có câu tiếng Việt, khai báo trong `[java.messages]`; bản gốc vẫn in kèm trong ngoặc.
- **Vết gọi hàm** chỉ giữ các hàm của người dùng; các khung lặp lại khi đệ quy vô hạn được gộp.

```
lỗi lúc chạy: chia cho 0 (java.lang.ArithmeticException: / by zero)
 --> chay.agn:3, trong hàm «thương»
  |
3 |         trả về a chia b
    được gọi từ hàm «main» (chay.agn:16)
```

### 9.3. Đánh đổi giữa tự do và báo lỗi

Mức độ tự do trong cú pháp tỉ lệ nghịch với độ dễ của việc báo lỗi. Khi một chuỗi đầu vào không khớp luật nào, hệ thống không thể biết chắc người dùng định viết gì; nó chỉ nói được chỗ nào sai và ở đó cần gì. Đề xuất "có phải bạn muốn..." dựa trên độ gần là việc của pha soạn thảo.

---

## 10. Ngôn ngữ hiện có

Toàn bộ các cấu trúc dưới đây là nội dung của `language.toml`, không phải của lõi.

### 10.1. Các cấu trúc

| Viết | Java |
|---|---|
| `Lớp A { ... }` | `public class A { ... }` |
| `Hàm gốc { ... }` | `public static void main(String[] args) { ... }` |
| `Hàm f(số nguyên a) trả về số nguyên { ... }` | `static int f(int a) { ... }` |
| `Hàm g { ... }` | `static void g() { ... }` |
| `trả về x`, `trả về` | `return x;`, `return;` |
| `đặt biến a là 5` | `var a = 5;` |
| `đặt số thực a là 5` | `double a = 5;` |
| `a là a cộng 1` | `a = a + 1;` |
| `a += 1`, `a -= 1`, `a *= 2`, `a /= 2` | `a += 1;` ... (chỉ có dạng ký hiệu) |
| `đặt số thực số dư là 0` ở cấp lớp | `static double so_du = 0;` (biến dùng chung) |
| `khi <điều kiện> thì { }` | `while (...) { }` |
| `lặp i từ 1 đến n { }`, `lặp i = 1 .. n { }` | `for (var i = 1; i <= n; i++) { }` (tính cả `n`) |
| `lặp i từ n lùi về 1 { }` | `for (var i = n; i >= 1; i--) { }` |
| `với mỗi x trong ds { }` | `for (var x : ds) { }` |
| `nếu ... { } không thì nếu ... { } không thì { }` | `if / else if / else` |
| `dừng`, `thoát` | `break;` |
| `tiếp tục`, `bỏ qua` | `continue;` |
| `in x`, `in(x)`, `in ra màn hình x` | `System.out.println(x);` |
| `in()`, `in liền x` | xuống dòng trống; in mà không xuống dòng |
| `"Chào {tên}, {tuổi cộng 1} tuổi"` | `"Chào " + ten + ", " + (tuoi + 1) + " tuổi"` |
| `a bằng với "An"` (trên chữ) | `Objects.equals(a, "An")` |
| `[7, 8, 9]` | `new ArrayList<>(List.of(7, 8, 9))` |
| `ds[0]`, `ds[0] là 5` | `ds.get(0)`, `ds.set(0, 5);` (chỉ số từ 0) |
| `thêm 10 vào ds` | `ds.add(10);` |
| `ds thứ 0`, `phần tử thứ 0 của ds`, `thành phần thứ 0 của ds` | `ds.get(0)`: ba cách viết của `ds[0]`, dùng được cả trên chữ |
| `ds thứ 0 là 5`, `phần tử thứ 0 của ds là 5` | `ds.set(0, 5);` |
| `ds[0] += 1`, `bảng[i][j] là 5` | `ds.set(0, ds.get(0) + (1));`, `bang.get(i).set(j, 5);` |
| `xóa vị trí 1 khỏi ds`, `xóa phần tử thứ 1 khỏi ds` | `ds.remove((int) (1));` (theo vị trí) |
| `xóa 20 khỏi ds` | `ds.remove((Object) (20));` (theo giá trị) |
| `sắp xếp lại ds`, `ds chứa x` | `Collections.sort(ds);`, `ds.contains(x)` |
| `tên[0]`, `tên nhỏ hơn "Bình"`, `tên chứa "n"` | `ten.charAt(0)`, `ten.compareTo("Bình") < 0`, `ten.contains("n")` |
| `2 ^ 10`, `căn bậc hai của x`, `trị tuyệt đối của x` | `Math.pow(2, 10)`, `Math.sqrt(x)`, `Math.abs(x)` |
| `a chia hết cho b`, `nếu a bằng b`, `nếu a là 3` | `a % b == 0`, `a == b`, `a == 3` |
| `độ dài của ds`, `độ dài của chữ` | `ds.size()`, `chu.length()` |
| `nhập số nguyên cho biến tuổi`, `danh sách số nguyên tên ds` | Như viết không có `cho biến`, `tên`: từ đệm trước tên |
| `nhập số nguyên tuổi`, `nhập chữ tên`, `nhập tên` | Đọc một dòng từ bàn phím vào một biến mới; không ghi kiểu thì là chữ |
| `nhập số nguyên tuổi với lời nhắc "Tuổi: "` | In lời nhắc (không xuống dòng) rồi đọc |
| `a là nhập số nguyên` | Dạng giá trị, để đọc vào biến đã có hoặc dùng giữa biểu thức |
| `f(1 và 2)` | `f(1, 2);` |

### 10.2. Viết bằng ký hiệu

Hầu hết từ vựng có một cách viết bằng ký hiệu tương đương. Hai cách cho ra đúng cùng một mã Java; bộ kiểm thử xác nhận điều này trên một chương trình viết theo cả hai kiểu.

| Chữ | Ký hiệu | Token |
|---|---|---|
| `cộng`, `trừ` | `+`, `-` | `ADD`, `SUB` |
| `nhân` | `*`, `×` | `MUL` |
| `chia` | `/`, `÷` | `DIV` |
| `chia dư` | `%` | `MOD` |
| `lớn hơn`, `nhỏ hơn` | `>`, `<` | `GT`, `LT` |
| `lớn hơn hoặc bằng` | `>=`, `≥` | `GE` |
| `nhỏ hơn hoặc bằng` | `<=`, `≤` | `LE` |
| `bằng với`, `giống` | `==` | `EQ` |
| `khác`, `khác với` | `!=`, `≠` | `NE` |
| `và`, `hoặc` | `&&`, `\|\|` | `AND`, `OR` |
| `không phải` | `!` | `NOT` |
| `là`, `bằng`, `có giá trị` | `=`, `:=` | `ASSIGN` |
| `trả về`, `trả lại` | `->`, `→` | `RETURN` (cả kiểu trả về lẫn lệnh trả về) |
| `đến`, `tới` | `..` | `TO` |
| `trong`, `thuộc` | `:` | `IN` |
| `độ dài của`, `số phần tử` | `#` | `LEN` |

Cùng một hàm, viết theo hai kiểu:

```
Hàm tổng(danh sách số nguyên ds) trả về số nguyên {     Hàm tổng(danh sách số nguyên ds) -> số nguyên {
    đặt kết_quả là 0                                        đặt kết_quả = 0
    với mỗi x trong ds {                                    với mỗi x : ds {
        kết_quả là kết_quả cộng x                               kết_quả += x
    }                                                       }
    trả về kết_quả                                          -> kết_quả
}                                                       }
```

Ghi chú về phạm vi của ký hiệu:

- **Từ khóa cấu trúc chưa có ký hiệu**: `nếu`, `khi`, `lặp`, `với mỗi`, `in`, `thêm ... vào`, `xóa ... khỏi`, `Hàm`, `Lớp`, và tên kiểu. Muốn thêm thì bổ sung vào danh sách của nhóm trong `language.toml`; lõi không cần đổi.
- **Cố ý không có `<-` cho phép gán.** Theo quy tắc cụm dài nhất, `a <-1` sẽ bị đọc thành phép gán thay vì "a nhỏ hơn âm một". Một ký hiệu mới chỉ nên được thêm khi nó không phải là hai ký hiệu sẵn có đứng liền nhau một cách hợp lệ.
- **`và` là dấu phẩy trong danh sách, `&&` thì không** (Mục 3.6): `f(a và b)` là hai đối số, `f(a && b)` là một.
- **Bộ gợi ý chỉ liệt kê cụm chữ**, không liệt kê ký hiệu.

Ví dụ chạy được nằm trong thư mục `vi-du/`; `ky-hieu.agn` viết theo kiểu ký hiệu.

---

## 11. Hỗ trợ soạn thảo

Pha soạn thảo là lệnh `anature lsp`: một máy chủ theo giao thức LSP qua stdin/stdout. Thư mục `vscode/` chứa phần mở rộng VS Code, vốn chỉ khởi động máy chủ này; mọi trình soạn thảo hiểu LSP đều dùng được. Máy chủ đọc lại `language.toml` ở mỗi lần phân tích, nên sửa cấu hình là gợi ý và màu đổi theo ngay.

**File cấu hình của một tệp nguồn** là `language.toml` gần nhất, tính từ thư mục chứa tệp đi ngược lên; không có thì dùng bản mặc định đi kèm chương trình. Quy tắc này không phụ thuộc lệnh được chạy từ thư mục nào, nên `build`, `run` và trình soạn thảo luôn dùng cùng một file.

**Khi `language.toml` đang hỏng** (thường là đang sửa dở), máy chủ không báo lỗi lên các tệp nguồn. Nó tiếp tục phân tích bằng bản cấu hình hợp lệ gần nhất, đặt lỗi lên chính `language.toml` ở đúng dòng, và chỉ để lại một lời nhắc (không phải lỗi) ở dòng đầu của tệp nguồn. Sửa xong thì mọi thứ tự hết. Lệnh `build` và `run` thì dừng hẳn với thông báo lỗi cấu hình, vì chạy bằng một cấu hình khác với cái đang ghi trên đĩa là sai.

### 11.1. Gợi ý khi gõ

Gợi ý là tập "những gì hợp lệ tại đây" của parser (Mục 9.1), tính tại vị trí con trỏ. Cách làm: cắt văn bản tại con trỏ, đặt một token đánh dấu vào cuối, cho parser chạy, rồi đọc tập hợp lệ tại token đó. Thành phần này không dùng mô hình, không dùng xác suất; mục đích là **dẫn người dùng về từ vựng đã có**.

```
người dùng gõ:  "đặt biến a "
hiển thị:        bằng        từ khóa ASSIGN
                 có giá trị  từ khóa ASSIGN
                 là          từ khóa ASSIGN
```

- **Lọc theo phần đang gõ.** Vì một cụm có thể gồm nhiều từ, phần đang gõ dở được thử ở nhiều cách cắt: gõ `Mỗi l` cho ra `Mỗi lúc`, và cả đoạn `Mỗi l` được thay thế.
- **Giữ chữ hoa.** Gõ hoa đầu câu thì gợi ý cũng viết hoa.
- **Tên đã dùng.** Ở vị trí cần một tên hoặc một giá trị, các tên xuất hiện phía trên con trỏ được gợi ý, kể cả tên nhiều từ.
- **Khung câu lệnh.** Ở đầu một câu lệnh, ngoài từng từ khóa còn có khung của cả câu, dựng tự động từ luật: chọn `lặp ‹biến› từ ‹đầu› đến ‹cuối› { }` thì cả khung được điền, và `Tab` nhảy qua từng ô. Thêm một luật vào cấu hình là có khung cho nó. Khi chưa gõ gì, mỗi luật chỉ hiện một khung với cách viết đầu tiên; gõ tới một cách viết đồng nghĩa thì khung của nó mới hiện.
- **Dấu cách cũng gọi gợi ý**, vì sau `đặt biến a ` chính là lúc cần biết viết gì tiếp. Đổi lại, trong VS Code phím `Enter` không nhận gợi ý (dùng `Tab`).
- **Không gợi ý** khi phần trước con trỏ đã sai cú pháp, hoặc khi đang ở trong chuỗi.

### 11.2. Báo lỗi khi gõ và did-you-mean

Mỗi lần văn bản đổi, máy chủ phân tích lại. Nếu sai cú pháp, nó gạch chân lỗi đầu tiên với đúng thông báo của Mục 9.1. Nếu cú pháp đúng, nó gạch chân mọi lỗi về tên và kiểu của Mục 7, nên biến viết sai hay gọi hàm sai kiểu hiện ra ngay lúc gõ.

Khi chỗ lỗi có một tên lạ nằm gần một cụm hợp lệ tại vị trí đó, thông báo có thêm một dòng đề xuất:

```
ở đây cần một trong: «[», «(», «là», «bằng», «có giá trị», «=» — nhưng gặp «tổng»
có phải bạn muốn «nếu» thay cho «neu»?
```

Độ gần là khoảng cách sửa đổi (Levenshtein) sau khi bỏ dấu, nên quên dấu được coi là gần nhất. Đây là nơi duy nhất trong hệ thống dùng một kỹ thuật ước lượng. Nó chỉ thêm chữ vào thông báo lỗi; nó không bao giờ quyết định việc phân tích, và lệnh `build` / `run` không dùng tới nó.

### 11.3. Tô màu

Màu được tính từ lexer (semantic tokens), nên luôn khớp với từ vựng trong cấu hình: từ khóa, kiểu, toán tử. **Từ đệm được tô như chú thích**, vì trình dịch bỏ qua chúng; người viết nhìn màu là biết từ nào mang nghĩa. Chuỗi, số và chú thích được tô bằng một ngữ pháp tĩnh nhỏ trong phần mở rộng.

### 11.4. Giới hạn phạm vi

Hỗ trợ soạn thảo là một lớp phủ lên trên lõi. Hệ thống hoạt động đầy đủ mà không có nó.

---

## 12. Phạm vi loại trừ và giới hạn hiện tại

### 12.1. Loại trừ theo nguyên tắc

Những điều sau **không** thuộc phạm vi hệ thống, vì chúng phá vỡ các nguyên tắc ở Mục 1.2:

- **Hiểu ngôn ngữ tự nhiên không giới hạn.** Hệ thống không diễn giải các câu nằm ngoài từ vựng và luật đã định nghĩa.
- **Mở rộng từ vựng tự động ở thời điểm chạy.** Từ vựng chỉ thay đổi qua file cấu hình.
- **Thành phần theo xác suất ở thời điểm chạy.**

### 12.2. Giới hạn hiện tại

Những điều sau chưa có, nhưng không trái nguyên tắc:

- **Từ dành riêng trong tên** là giới hạn gặp nhiều nhất: 3 trong 10 chương trình thử vẫn vấp (`chữ cái`, `đã đoán đúng`, `có trong`). Thông báo nay nói rõ lý do (Mục 9.1), nhưng người viết vẫn phải đổi tên. Mỗi từ thêm vào ngôn ngữ lại lấy đi một từ của tên; vì vậy lũy thừa chỉ có ký hiệu `^`, và sắp xếp là `sắp xếp lại`. Riêng `đặt số nguyên tố là 5` vẫn âm thầm khai báo biến `tố`; lỗi chỉ hiện ở chỗ dùng.
- **`7 chia 2` ra `3`** (chia số nguyên, như Java); `0.1 cộng 0.2` ra `0.30000000000000004`.
- **Gán danh sách là gán tham chiếu**: sau `đặt b là a`, thêm vào `b` cũng đổi `a`.
- **Kiểm tra kiểu chưa phủ hết.** Điều kiện của `nếu` / `khi` không bị kiểm là đúng/sai; phép tính trên kiểu không hợp (chữ trừ số) không bị bắt. Những lỗi đó do `javac` báo: vị trí được quy về mã nguồn, nhưng chú giải vẫn là tiếng Anh và không có cột.
- **Kiểu còn chặt**: `đặt danh sách số thực ds là [1, 2]` bị từ chối; `thêm 5000 vào` danh sách số nguyên lớn bị `javac` từ chối; danh sách `[]` không khai kiểu thì không tính toán được với phần tử.
- **Chỉ báo một lỗi cú pháp mỗi lần**, và lỗi cú pháp che lỗi về tên và kiểu.
- **Cú pháp chưa có**: `nếu ... thì in(1)` không ngoặc nhọn; `in(1, 2)`; `nếu a lớn hơn 1 và nhỏ hơn 10`; dòng mới mở đầu bằng toán tử; gọi hàm không ngoặc; hai hàm cùng tên khác tham số; tham số mặc định; bước nhảy của vòng lặp.
- **Thư viện chưa có**: viết hoa, cắt chuỗi, duyệt từng kí tự của chữ, đổi chữ thành số, định dạng số, số ngẫu nhiên, chèn và nối danh sách, chỉ số âm.
- **Chuỗi nội suy không chứa được dấu `"`** bên trong `{}`; không có chuỗi nhiều dòng.
- **`build` không gọi `javac`.** Các lỗi mà tầng kiểm tra bỏ sót chỉ lộ ra khi `run`.
- **Trình soạn thảo**: mất gợi ý ở mọi dòng phía dưới một lỗi cú pháp và trong `{}` của chuỗi; gợi ý cả tên nằm ngoài phạm vi (biến của hàm khác).
- **Hàm gốc hiện là «main»** trong vết gọi hàm.
- **Một lớp cho mỗi chương trình**; chưa có trường, đối tượng, hay nhiều tệp.

---

## 13. Trình tự triển khai

| Bước | Trạng thái |
|---|---|
| 1. Lexer: tách token, giữ span, chuỗi và số | Xong |
| 2. Bảng tra cứu từ cấu hình | Xong |
| 3. Gộp đồng nghĩa, từ đệm theo vị trí | Xong |
| 4. Bộ thông dịch luật, kiểm chứng trên các câu biến thể | Xong |
| 5. Sinh mã Java, hai mức `build` / `run` | Xong |
| 6. Mở rộng: hàm, kiểu, vòng lặp, danh sách, nhập liệu | Xong |
| 7. Quy lỗi của Java về mã nguồn | Xong |
| 8. Hỗ trợ soạn thảo: gợi ý khi gõ, did-you-mean, tô màu, khung câu lệnh | Xong |
| 9. Tên nhiều từ, chuỗi nội suy, kiểm tra ngữ nghĩa | Xong |

---

## 14. Tóm tắt các quyết định kiến trúc

| Quyết định | Lý do |
|---|---|
| Hai pha tách biệt (soạn thảo / thực thi) | Giữ tính xác định ở thời điểm chạy trong khi vẫn cho phép trợ giúp lúc soạn |
| Mọi cụm tra bảng cứng; một cụm, một nhóm | Loại nhập nhằng ở tầng từ vựng mà không cần ngữ cảnh |
| Ký hiệu chỉ là thêm một cách viết đồng nghĩa trong nhóm | Người quen ký hiệu và người quen chữ dùng chung một ngôn ngữ, lõi không phân biệt |
| Từ đệm chia nhóm theo vị trí | Một từ có thể là đệm ở chỗ này, mang nghĩa ở chỗ khác |
| Từ vựng **và luật** đều nằm trong một file TOML | Đổi ngôn ngữ bề mặt mà lõi không đổi một dòng mã |
| PEG với lựa chọn có thứ tự, khớp trọn câu lệnh | Loại bỏ nhập nhằng, đảm bảo xác định |
| Kiểm tra cấu hình khi nạp | PEG che mâu thuẫn chứ không báo |
| Dạng trung gian tách khỏi mã đích | Đổi ngôn ngữ đích chỉ cần thay các bảng `[java.*]` |
| Giữ span xuyên suốt; ghi chú số dòng trong mã sinh ra | Báo lỗi trỏ đúng mã nguồn gốc, kể cả lỗi của Java |
| Không phân biệt hoa/thường; chuẩn hóa NFC | Viết hoa đầu câu và cách gõ dấu không được đổi nghĩa chương trình |
| Tên đổi sang ASCII, gắn số khi trùng | Mã Java an toàn với mọi môi trường mà vẫn đọc được khi debug |
| Luôn đóng ngoặc biểu thức con khi sinh mã | Độ ưu tiên trong cấu hình có thể khác của Java |
| Luật tự khai ý nghĩa (`declare`, `function`, ...) trong cấu hình | Lõi kiểm tra được tên và kiểu mà vẫn không biết luật nào làm gì |
| Kiểu chưa biết thì không báo lỗi; `javac` là lưới cuối | Không bao giờ từ chối một chương trình đúng |
| Toán tử có biến thể theo kiểu (`EQ.STRING`) | So sánh chữ đúng nội dung; Java dùng `==` cho địa chỉ |
| Tên nhiều từ, dừng ở cụm đầu tiên của ngôn ngữ | Đọc như tiếng Việt; đổi lại mọi cụm của ngôn ngữ bị cấm trong tên |
| Ước lượng chỉ trong pha soạn thảo | Thời điểm chạy phải xác định tuyệt đối |

### Thay đổi so với phiên bản 1.0

- **Từ đệm:** từ một danh sách dùng chung thành các nhóm theo vị trí. Việc loại từ đệm chuyển hẳn sang parser; không còn tầng chuẩn hóa riêng loại từ đệm trước parser.
- **Luật ngữ pháp:** từ ký hiệu minh họa thành dữ liệu thật trong file cấu hình, có phạm vi (`scope`) và mẫu Java đi kèm.
- **Một cụm, một nhóm:** quy tắc mới, kèm kiểm tra khi nạp.
- **Tên:** bổ sung quy tắc không phân biệt hoa/thường, chuẩn hóa NFC và bảng tên ASCII.
- **Lỗi của Java:** nguyên tắc 2 được mở rộng từ lỗi cú pháp sang lỗi biên dịch và lỗi lúc chạy.
- **Cụ thể hóa:** ngôn ngữ đích là Java, lõi viết bằng Rust, cấu hình là TOML, khối dùng `{ }`, câu lệnh kết thúc bằng xuống dòng.

### Thay đổi từ 2.0

- **2.1:** triển khai pha soạn thảo (`anature lsp` và phần mở rộng VS Code): gợi ý khi gõ, báo lỗi kèm did-you-mean, tô màu theo từ vựng (Mục 11).
- **2.2:** cách viết bằng ký hiệu cho phần lớn từ vựng, gán kèm phép tính, và vòng lặp dạng `lặp i = 1 .. n` (Mục 10.2). Chỉ `language.toml` thay đổi.
- **2.3:** tầng kiểm tra ngữ nghĩa bằng tiếng Việt và toán tử theo kiểu (Mục 7), sửa lỗi so sánh chữ; tên nhiều từ (Mục 3.7); chuỗi nội suy (Mục 4.2); `dừng` / `tiếp tục`; giải thích khi dùng từ dành riêng làm tên (Mục 9.1); khung câu lệnh trong gợi ý (Mục 11.1).
- **2.4:** sửa các lỗi tìm được khi thử bằng 10 chương trình thật (thư mục `thu-nghiem/`). Sai kết quả im lặng: so sánh hai số trong danh sách, so sánh hai danh sách, tràn số, vòng đếm ngược mốc (Mục 7.2, 7.4). Tốc độ: nhớ kết quả phân tích khối (Mục 6.1). Cách viết: xuống dòng tự nhiên hơn, `;`, `/* */`, `nếu a bằng b`, phủ định ôm phép so sánh (Mục 4.4, 6.5). Tính năng: biến cấp lớp, `lùi về`, gán phần tử, chuỗi (`tên[0]`, so sánh, `chứa`), `^`, `căn bậc hai`, số lớn. Thông báo: tên bị cắt ngang, tệp rỗng, thiếu hàm gốc, tệp có BOM; đề xuất tên gần đúng cho biến viết sai.
- **2.5:** tìm file cấu hình theo vị trí tệp nguồn thay vì thư mục chạy lệnh; máy chủ gợi ý dùng tiếp cấu hình hợp lệ gần nhất khi `language.toml` đang hỏng (Mục 11); cụm được phép chứa số; từ khóa trùng toán tử được gỡ bằng `keyword_operators` (Mục 3.3). Bộ kiểm thử dùng bản cấu hình cố định `tests/language.toml`.
- **2.6:** đuôi tệp đổi sang `.agn`; nhập thẳng vào biến (`nhập số nguyên tuổi`, kèm `với lời nhắc`); từ đệm trước tên (Mục 3.4).
- **2.7:** lấy và gán phần tử bằng chữ: `ds thứ i`, `phần tử thứ i của ds` (Mục 6.5).
- **2.8:** đệm giữa tên hàm và danh sách tham số: `từ`, `với tham gia`, `với tham số` (Mục 3.4).
