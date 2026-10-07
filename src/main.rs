mod codegen;
mod config;
mod error;
mod lexer;
mod lsp;
mod parser;
mod report;
mod semantics;

use std::path::{Path, PathBuf};
use std::io::{BufRead, BufReader};
use std::process::{Command, ExitCode, Stdio};

use unicode_normalization::UnicodeNormalization;

use config::Lang;
use error::Error;

const DEFAULT_CONFIG: &str = include_str!("../language.toml");
/// Bản cấu hình cố định cho kiểm thử, tách khỏi file mà người dùng đang sửa.
#[cfg(test)]
const TEST_CONFIG: &str = include_str!("../tests/language.toml");
const CONFIG_NAME: &str = "language.toml";

const USAGE: &str = "\
Cách dùng:
  anature build <tệp.agn> [-o <thư mục>] [--config <tệp.toml>]   sinh file .java để xem và debug
  anature run   <tệp.agn> [--config <tệp.toml>]                  dịch rồi chạy luôn
  anature lsp                                                   máy chủ gợi ý cho trình soạn thảo (LSP qua stdio)

Cấu hình: --config, nếu không thì language.toml gần nhất tính từ thư mục của tệp nguồn
đi ngược lên, cuối cùng là bản mặc định đi kèm chương trình.";

/// Lỗi cú pháp dừng ngay ở lỗi đầu tiên; lỗi ngữ nghĩa được gom lại báo một lượt.
fn transpile(lang: &Lang, src: &str, source_name: &str) -> Result<codegen::Output, Vec<Error>> {
    let toks = lexer::lex(lang, src).map_err(|e| vec![e])?;
    let mut nodes = parser::parse(lang, src, &toks).map_err(|e| vec![e])?;
    if nodes.is_empty() {
        return Err(vec![Error::new("tệp chưa có chương trình nào")]);
    }
    let errors = semantics::check(lang, &mut nodes);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(codegen::generate(lang, &nodes, source_name))
}

struct Args {
    run: bool,
    source: PathBuf,
    out_dir: Option<PathBuf>,
    config: Option<PathBuf>,
}

fn parse_args() -> Option<Args> {
    let mut args = std::env::args().skip(1);
    let run = match args.next()?.as_str() {
        "build" => false,
        "run" => true,
        _ => return None,
    };
    let (mut source, mut out_dir, mut config) = (None, None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" => out_dir = Some(PathBuf::from(args.next()?)),
            "--config" => config = Some(PathBuf::from(args.next()?)),
            _ if source.is_none() => source = Some(PathBuf::from(a)),
            _ => return None,
        }
    }
    Some(Args { run, source: source?, out_dir, config })
}

/// File cấu hình của một tệp nguồn: language.toml gần nhất, tính từ thư mục chứa tệp
/// đi ngược lên. Không phụ thuộc lệnh được chạy từ thư mục nào, nên dòng lệnh và
/// trình soạn thảo luôn dùng cùng một file.
fn config_path(source: &Path) -> Option<PathBuf> {
    let source = std::path::absolute(source).ok()?;
    source.ancestors().skip(1).map(|dir| dir.join(CONFIG_NAME)).find(|p| p.is_file())
}

/// Cấu hình cho một tệp nguồn: `explicit`, nếu không thì theo `config_path`, cuối
/// cùng là bản mặc định đi kèm chương trình.
fn load_config(source: &Path, explicit: Option<&Path>) -> Result<Lang, String> {
    let path = match explicit {
        Some(p) => Some(p.to_path_buf()),
        None => config_path(source),
    };
    let (label, text) = match path {
        Some(p) => {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("lỗi: không đọc được {}: {e}", p.display()))?;
            (p.display().to_string(), text)
        }
        None => ("cấu hình mặc định".to_string(), DEFAULT_CONFIG.to_string()),
    };
    config::load(&text).map_err(|e| format!("lỗi cấu hình ({label}): {}", e.msg))
}

fn real_main() -> Result<ExitCode, String> {
    if std::env::args().nth(1).as_deref() == Some("lsp") {
        lsp::serve();
        return Ok(ExitCode::SUCCESS);
    }
    let Some(args) = parse_args() else {
        return Err(USAGE.to_string());
    };
    let lang = load_config(&args.source, args.config.as_deref())?;
    let file = args.source.display().to_string();
    let src: String = std::fs::read_to_string(&args.source)
        .map_err(|e| format!("lỗi: không đọc được {file}: {e}"))?
        // Tệp lưu từ Windows hay mở đầu bằng một dấu thứ tự byte vô hình.
        .trim_start_matches('\u{feff}')
        .nfc()
        .collect();

    let out = transpile(&lang, &src, &file)
        .map_err(|errors| errors.iter().map(|e| e.render(&file, &src)).collect::<Vec<_>>().join("\n\n"))?;
    let stem = args.source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let file_name = format!("{}.java", out.class.clone().unwrap_or(stem));
    let java = &out.java;

    if !args.run {
        let dir = args.out_dir.unwrap_or_else(|| args.source.parent().unwrap_or(Path::new("")).to_path_buf());
        let path = dir.join(&file_name);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(&path, java).map_err(|e| format!("lỗi: không ghi được {}: {e}", path.display()))?;
        println!("đã ghi {}", path.display());
        return Ok(ExitCode::SUCCESS);
    }

    let dir = std::env::temp_dir().join(format!("anature-{}", std::process::id()));
    let path = dir.join(&file_name);
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&path, java))
        .map_err(|e| format!("lỗi: không ghi được {}: {e}", path.display()))?;
    // Chỉ chặn stderr: mọi thông báo của Java được dịch về mã nguồn gốc trước khi in.
    let status = Command::new("java").arg(&path).stderr(Stdio::piped()).spawn().and_then(|mut child| {
        let mut report = report::Report::new(&lang, &file, &src, &file_name, &out);
        for line in BufReader::new(child.stderr.take().expect("stderr đã được nối ống")).lines() {
            match line.map(|l| report.line(&l)) {
                Ok(Some(text)) => eprintln!("{text}"),
                Ok(None) => {}
                Err(_) => break,
            }
        }
        child.wait()
    });
    let _ = std::fs::remove_dir_all(&dir);
    let status = status.map_err(|e| format!("lỗi: không chạy được `java` (cần Java 11 trở lên): {e}"))?;
    Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
}

fn main() -> ExitCode {
    real_main().unwrap_or_else(|msg| {
        eprintln!("{msg}");
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chỉ lexer, parser và sinh mã: các kiểm thử cú pháp dùng tên chưa khai báo.
    fn syntax(src: &str) -> Result<String, Error> {
        let lang = config::load(TEST_CONFIG).unwrap();
        let toks = lexer::lex(&lang, src)?;
        let nodes = parser::parse(&lang, src, &toks)?;
        Ok(codegen::generate(&lang, &nodes, "t.agn").java)
    }

    /// Toàn bộ đường đi, kể cả kiểm tra ngữ nghĩa, trên thân của một hàm gốc.
    fn checked(body: &str) -> Result<String, Vec<String>> {
        let lang = config::load(TEST_CONFIG).unwrap();
        let src = format!("Lớp T {{\nHàm gốc {{\n{body}\n}}\n}}\n");
        transpile(&lang, &src, "t.agn").map(|o| o.java).map_err(|errors| errors.into_iter().map(|e| e.msg).collect())
    }

    fn run(body: &str) -> Result<String, Error> {
        let src = format!("Lớp T {{\nHàm gốc {{\n{body}\n}}\n}}\n");
        syntax(&src)
    }

    fn java(body: &str) -> String {
        run(body).unwrap_or_else(|e| panic!("{}", e.msg))
    }

    #[test]
    fn bien_the_cho_cung_dau_ra() {
        let a = java("Khi a lớn hơn b { in a }");
        let b = java("MỖI LÚC giá trị a lớn hơn giá trị của b thì { in ra màn hình a }");
        let c = java("trong khi a > b { xuất(a) }");
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert!(a.contains("while (a > b) {"));
    }

    #[test]
    fn chuoi_khong_bi_dien_giai() {
        assert!(java("in(\"Khi nào xong?\")").contains("System.out.println(\"Khi nào xong?\");"));
    }

    #[test]
    fn tu_khoa_khong_khop_giua_ten() {
        assert!(java("đặt biến Khiem là 1").contains("var Khiem = 1;"));
    }

    #[test]
    fn cum_dai_thang_cum_ngan() {
        assert!(java("nếu a lớn hơn hoặc bằng b { }").contains("if (a >= b) {"));
        assert!(java("đặt x có giá trị 7 chia dư 2").contains("var x = 7 % 2;"));
    }

    #[test]
    fn ten_khong_phan_biet_hoa_thuong() {
        assert!(java("đặt Tong là 1\ntong là TONG cộng 1").contains("Tong = Tong + 1;"));
    }

    #[test]
    fn uu_tien_toan_tu() {
        let out = java("đặt x là 1 cộng 2 nhân 3 trừ 4");
        assert!(out.contains("var x = (1 + (2 * 3)) - 4;"), "{out}");
        assert!(java("đặt y là không phải (a và b)").contains("var y = !(a && b);"));
    }

    #[test]
    fn dong_ket_thuc_bang_toan_tu_thi_con_tiep() {
        assert!(java("đặt x là 1 cộng\n2").contains("var x = 1 + 2;"));
    }

    #[test]
    fn neu_khong_thi_neu() {
        let out = java("nếu a { } không thì nếu b { } ngược lại { }");
        assert!(out.contains("} else if (b) {"), "{out}");
    }

    /// Bọc `items` trong một lớp, không tự thêm hàm gốc.
    fn class(items: &str) -> String {
        syntax(&format!("Lớp T {{\n{items}\n}}\n")).unwrap_or_else(|e| panic!("{}", e.msg))
    }

    #[test]
    fn ham_co_tham_so_va_kieu_tra_ve() {
        let out = class("Hàm cong(số nguyên a, số thực b) trả về số thực {\n trả về a cộng b\n}");
        assert!(out.contains("static double cong(int a, double b) {"), "{out}");
        assert!(out.contains("return a + b;"), "{out}");
        let out = class("Hàm chao(chữ ten) {\n in(ten)\n trả về\n}\nHàm rong { }");
        assert!(out.contains("static void chao(String ten) {"), "{out}");
        assert!(out.contains("return;") && out.contains("static void rong() {"), "{out}");
    }

    #[test]
    fn kieu_du_lieu() {
        let out = java("đặt số nguyên lớn a là 1\nđặt kí tự c là 'x'\nđặt luận lý d là đúng\nđặt đúng sai e là sai");
        assert!(out.contains("long a = 1;") && out.contains("char c = 'x';"), "{out}");
        assert!(out.contains("boolean d = true;") && out.contains("boolean e = false;"), "{out}");
    }

    #[test]
    fn va_tuong_duong_dau_phay_trong_danh_sach() {
        let a = class("Hàm f(số nguyên a và số nguyên b) { }\nHàm gốc {\n f(1 và 2)\n}");
        let b = class("Hàm f(số nguyên a, số nguyên b) { }\nHàm gốc {\n f(1, 2)\n}");
        assert_eq!(a, b);
        assert!(a.contains("f(1, 2);"), "{a}");
        // Ngoài danh sách, trong ngoặc con, hoặc viết &&: vẫn là phép "và".
        assert!(java("đặt x là f((a và b), c && d)").contains("var x = f(a && b, c && d);"));
        assert!(java("nếu f(a) và g(b) { }").contains("if (f(a) && g(b)) {"));
    }

    #[test]
    fn ten_co_dau_thanh_ten_java_rieng_biet() {
        let out = java("đặt Tổng là 1\nđặt Tông là 2\nđặt tong là 3\nđặt đếm là Tổng cộng tông cộng TONG");
        assert!(out.contains("var Tong = 1;") && out.contains("var Tong_2 = 2;"), "{out}");
        assert!(out.contains("var tong_3 = 3;"), "{out}");
        assert!(out.contains("var dem = Tong + Tong_2 + tong_3;"), "{out}");
        assert!(out.contains("//   Tông -> Tong_2"), "{out}");
        // Tên trùng từ khóa Java cũng được né.
        assert!(java("đặt class là 1").contains("var class_2 = 1;"));
    }

    #[test]
    fn vong_lap_dem_tinh_ca_can_tren() {
        let out = java("lặp i từ 1 đến n cộng 1 { in(i) }");
        assert!(out.contains("for (var i = 1; i <= n + 1; i++) {"), "{out}");
        assert!(java("lặp i từ n lùi về 1 { }").contains("for (var i = n; i >= 1; i--) {"));
    }

    #[test]
    fn danh_sach() {
        let out = java("đặt ds là [7, 8 và 9]\nin(ds[0] cộng độ dài của ds)\nvới mỗi x trong ds { in(x) }");
        assert!(out.contains("var ds = new ArrayList<>(List.of(7, 8, 9));"), "{out}");
        assert!(out.contains("System.out.println(ds.get(0) + (ds.size()));"), "{out}");
        assert!(out.contains("for (var x : ds) {"), "{out}");
        let out = java("thêm 1 vào ds\nds[0] là 5\nxóa phần tử thứ 0 khỏi ds\nđặt x là m[1][2]");
        assert!(out.contains("ds.add(1);") && out.contains("ds.set(0, 5);"), "{out}");
        assert!(out.contains("ds.remove((int) (0));") && out.contains("var x = m.get(1).get(2);"), "{out}");
    }

    #[test]
    fn kieu_danh_sach() {
        let out = class("Hàm f(danh sách số nguyên a, mảng mảng chữ b) trả về danh sách số thực { }");
        assert!(out.contains("static ArrayList<Double> f(ArrayList<Integer> a, ArrayList<ArrayList<String>> b) {"), "{out}");
        assert!(java("đặt danh sách số nguyên a là []").contains("ArrayList<Integer> a = new ArrayList<>(List.of());"));
    }

    #[test]
    fn nhap_lieu() {
        let out = java("đặt a là nhập số nguyên\nđặt b là nhập chữ\nđặt c là nhập\nđặt d là 1 cộng nhập số thực");
        assert!(out.contains("var a = Integer.parseInt(Anature.NHAP.nextLine().trim());"), "{out}");
        assert!(out.contains("var b = Anature.NHAP.nextLine();") && out.contains("var c = Anature.NHAP.nextLine();"), "{out}");
        assert!(out.contains("var d = 1 + Double.parseDouble("), "{out}");
        // Tên của người dùng không được chiếm tên mà mã sinh ra cần.
        assert!(java("đặt list là 1").contains("var list_2 = 1;"));
    }

    #[test]
    fn loi_cua_java_duoc_quy_ve_ma_nguon() {
        let lang = config::load(TEST_CONFIG).unwrap();
        let src = "Lớp T {\n  Hàm thương(số nguyên a) trả về số nguyên {\n    trả về 1 chia a\n  }\n  Hàm gốc {\n    đặt Tổng là thương(0)\n  }\n}\n";
        let out = transpile(&lang, src, "t.agn").unwrap();
        // Tìm dòng Java của một câu lệnh để dựng thông báo giả như của java.
        let at = |needle: &str| out.java.lines().position(|l| l.contains(needle)).unwrap() + 1;
        let mut r = report::Report::new(&lang, "t.agn", src, "T.java", &out);

        let got = r.line(&format!("/tmp/x/T.java:{}: error: cannot find symbol", at("var Tong"))).unwrap();
        assert!(got.starts_with("lỗi: tên chưa được khai báo") && got.contains("--> t.agn:6"), "{got}");
        assert!(got.contains("6 |     đặt Tổng là thương(0)"), "{got}");
        assert_eq!(r.line("        var Tong = thuong(0);"), None);
        assert_eq!(r.line("            ^"), None);
        assert_eq!(r.line("  symbol:   variable Tong").unwrap(), "  symbol:   variable Tổng");
        assert_eq!(r.line("1 error"), None);

        let got = r.line("Exception in thread \"main\" java.lang.ArithmeticException: / by zero").unwrap();
        assert!(got.starts_with("lỗi lúc chạy: chia cho 0"), "{got}");
        let got = r.line(&format!("\tat T.thuong(T.java:{})", at("return"))).unwrap();
        assert!(got.contains("--> t.agn:3, trong hàm «thương»") && got.contains("trả về 1 chia a"), "{got}");
        let got = r.line(&format!("\tat T.main(T.java:{})", at("var Tong"))).unwrap();
        assert!(got.contains("được gọi từ hàm «main» (t.agn:6)"), "{got}");
        assert_eq!(r.line("\tat java.base/java.lang.Thread.run(Thread.java:1)"), None);
    }

    #[test]
    fn ky_hieu_tuong_duong_voi_chu() {
        // Cùng một chương trình, viết bằng chữ và bằng ký hiệu, phải ra cùng một mã Java.
        let words = class(
            "Hàm f(số nguyên a và số nguyên b) trả về số nguyên {\n trả về a cộng b nhân 2\n}\n\
             Hàm gốc {\n đặt x là 0\n lặp i từ 1 đến 3 { x là x cộng i }\n\
             với mỗi e trong ds { in(e) }\n\
             nếu x lớn hơn hoặc bằng 2 và x khác 9 hoặc không phải (x nhỏ hơn hoặc bằng 1) { in(độ dài của ds chia 2) }\n}",
        );
        let symbols = class(
            "Hàm f(số nguyên a, số nguyên b) -> số nguyên {\n -> a + b × 2\n}\n\
             Hàm gốc {\n đặt x := 0\n lặp i = 1 .. 3 { x = x + i }\n\
             với mỗi e : ds { in(e) }\n\
             nếu x ≥ 2 && x ≠ 9 || !(x ≤ 1) { in(#ds ÷ 2) }\n}",
        );
        assert_eq!(words, symbols);
        assert!(words.contains("static int f(int a, int b) {") && words.contains("return a + (b * 2);"), "{words}");

        let out = java("a += 1\na -= 2\na *= 3\na /= 4");
        assert!(out.contains("a += 1;") && out.contains("a -= 2;"), "{out}");
        assert!(out.contains("a *= 3;") && out.contains("a /= 4;"), "{out}");
        // Số thập phân và ký hiệu .. không lẫn nhau; dấu trừ sau phép so sánh vẫn là số âm.
        assert!(java("lặp i = 1.5..n { }").contains("for (var i = 1.5; i <= n; i++) {"));
        assert!(java("nếu a<=-1 { }").contains("if (a <= (-1)) {"));
    }

    #[test]
    fn dung_va_tiep_tuc_trong_vong_lap() {
        let out = java("khi đúng {\n nếu a { dừng }\n nếu b { tiếp tục }\n thoát\n bỏ qua\n}");
        assert_eq!(out.matches("break;").count(), 2, "{out}");
        assert_eq!(out.matches("continue;").count(), 2, "{out}");
    }

    #[test]
    fn chuoi_noi_suy() {
        let out = java("in(\"Chào {tên}, {1 cộng 2} tuổi!\")");
        assert!(out.contains("System.out.println(\"Chào \" + ten + \", \" + (1 + 2) + \" tuổi!\");"), "{out}");
        // Mở đầu bằng biểu thức: thêm chuỗi rỗng để Java không gộp hai số với nhau.
        assert!(java("đặt s là \"{a}{b}\"").contains("var s = \"\" + a + b;"));
        // \{ là dấu ngoặc thật; chuỗi không có {} thì giữ nguyên.
        assert!(java("in(\"tập \\{1, 2\\} và \\\"x\\\"\")").contains("println(\"tập {1, 2} và \\\"x\\\"\");"));
        // Lỗi bên trong {} trỏ đúng vào trong chuỗi.
        let e = run("in(\"a {1 cộng} b\")").unwrap_err();
        assert_eq!((e.span.unwrap().line, e.span.unwrap().col), (3, 14));
        assert!(e.msg.contains("biểu thức") && e.msg.contains("dấu } của chuỗi"), "{}", e.msg);
        assert!(run("in(\"a {} b\")").is_err() && run("in(\"a {b\")").is_err());
    }

    #[test]
    fn ten_nhieu_tu() {
        let out = class(
            "Hàm tính tổng(số nguyên số đầu và số nguyên số sau) trả về số nguyên {\n trả về số đầu cộng số sau\n}\n\
             Hàm gốc {\n đặt biến Tổng Điểm là tính tổng(1, 2)\n tổng điểm là TỔNG ĐIỂM nhân 2\n}",
        );
        assert!(out.contains("static int tinh_tong(int so_dau, int so_sau) {"), "{out}");
        assert!(out.contains("return so_dau + so_sau;"), "{out}");
        assert!(out.contains("var Tong_Diem = tinh_tong(1, 2);") && out.contains("Tong_Diem = Tong_Diem * 2;"), "{out}");
        // Tên dừng lại ở cụm đầu tiên của ngôn ngữ.
        assert!(java("nếu điểm trung bình lớn hơn điểm chuẩn thì { }").contains("if (diem_trung_binh > diem_chuan) {"));
    }

    #[test]
    fn giai_thich_khi_dung_tu_danh_rieng_lam_ten() {
        let e = run("đặt biến chia là 5").unwrap_err();
        assert!(e.msg.contains("«chia» là toán tử (DIV) của ngôn ngữ nên không dùng làm tên được"), "{}", e.msg);
        let e = syntax("Lớp T {\nHàm in(số nguyên a) { }\n}").unwrap_err();
        assert!(e.msg.contains("«in» là từ khóa (PRINT)"), "{}", e.msg);
        // Ở đầu câu lệnh thì một từ khóa lạc chỗ không phải là "tên sai".
        assert!(!run("thì").unwrap_err().msg.contains("không dùng làm tên"));
    }

    #[test]
    fn kiem_tra_ten() {
        let errors = checked("in(x)\nđặt a là 1\nđặt a là 2\nb là a\nf(1)").unwrap_err();
        assert_eq!(
            errors,
            [
                "biến «x» chưa được khai báo",
                "biến «a» đã được khai báo ở dòng 4",
                "biến «b» chưa được khai báo",
                "hàm «f» chưa được định nghĩa",
            ]
        );
        // Biến chưa tồn tại ở vế phải của chính câu khai báo nó.
        assert_eq!(checked("đặt a là a cộng 1").unwrap_err(), ["biến «a» chưa được khai báo"]);
        // Biến của vòng lặp chỉ sống trong khối; dùng lại tên sau đó là hợp lệ.
        assert!(checked("lặp i từ 1 đến 3 { in(i) }\nlặp i từ 1 đến 3 { in(i) }").is_ok());
        assert_eq!(checked("lặp i từ 1 đến 3 { }\nin(i)").unwrap_err(), ["biến «i» chưa được khai báo"]);
        assert_eq!(checked("với mỗi x trong [1, 2] { in(x) }\nin(x)").unwrap_err(), ["biến «x» chưa được khai báo"]);
        assert!(checked("in(\"{y}\")").unwrap_err()[0].contains("«y» chưa được khai báo"));
    }

    fn checked_class(items: &str) -> Result<String, Vec<String>> {
        let lang = config::load(TEST_CONFIG).unwrap();
        // Thêm sẵn một hàm gốc nếu đoạn thử chưa có, vì chương trình bắt buộc phải có.
        let main = if items.contains("Hàm gốc") { "" } else { "\nHàm gốc { }" };
        let src = format!("Lớp T {{\n{items}{main}\n}}\n");
        transpile(&lang, &src, "t.agn").map(|o| o.java).map_err(|errors| errors.into_iter().map(|e| e.msg).collect())
    }

    #[test]
    fn kiem_tra_ham_va_kieu() {
        let f = "Hàm gộp hai(số nguyên a, số thực b) trả về số thực {\n trả về a cộng b\n}\n";
        // Hàm gọi được hàm định nghĩa phía dưới; số nguyên đặt được vào chỗ cần số thực.
        assert!(checked_class(&format!("Hàm gốc {{\n in(gộp hai(1, 2))\n}}\n{f}")).is_ok());
        let errors = checked_class(&format!(
            "{f}Hàm gốc {{\n in(gộp hai(1))\n in(gộp hai(1.5, 2))\n in(gộp hai(\"a\", 2))\n}}"
        ))
        .unwrap_err();
        assert_eq!(
            errors,
            [
                "hàm «gộp hai» cần 2 đối số, nhưng được gọi với 1",
                "đối số thứ 1 của «gộp hai» phải là số nguyên, nhưng nhận số thực",
                "đối số thứ 1 của «gộp hai» phải là số nguyên, nhưng nhận chữ",
            ]
        );
        assert_eq!(
            checked_class("Hàm f trả về số nguyên {\n trả về \"a\"\n}").unwrap_err(),
            ["hàm phải trả về số nguyên, nhưng giá trị này có kiểu chữ"]
        );
        assert_eq!(
            checked_class("Hàm f {\n trả về 1\n}").unwrap_err(),
            ["hàm này không khai báo kiểu trả về nên không trả về giá trị được"]
        );
        assert_eq!(
            checked("đặt số nguyên a là 2.5\nđặt b là 1\nb là \"x\"\nđặt danh sách chữ c là [1]").unwrap_err(),
            [
                "biến có kiểu số nguyên, không nhận được giá trị kiểu số thực",
                "biến có kiểu số nguyên, không gán được giá trị kiểu chữ",
                "biến có kiểu danh sách chữ, không nhận được giá trị kiểu danh sách số nguyên",
            ]
        );
        assert_eq!(
            checked("đặt a là [9, 7.5]").unwrap_err(),
            ["các phần tử của danh sách phải cùng kiểu, nhưng ở đây có cả số nguyên và số thực"]
        );
        // Không biết kiểu thì không báo lỗi: [] rỗng, và phần tử của danh sách chưa rõ kiểu.
        assert!(checked("đặt danh sách số nguyên a là []\nđặt số nguyên lớn b là 1\nđặt số thực c là b chia 2").is_ok());
    }

    #[test]
    fn so_sanh_va_do_dai_theo_kieu() {
        // Trên chữ, == của Java so địa chỉ chứ không so nội dung.
        let out = checked("đặt tên là nhập chữ\nnếu tên bằng với \"An\" hoặc \"Bình\" khác tên { in(độ dài của tên) }").unwrap();
        assert!(out.contains("if ((Objects.equals(ten, \"An\")) || (!Objects.equals(\"Bình\", ten))) {"), "{out}");
        assert!(out.contains("System.out.println(ten.length());"), "{out}");
        // Trên số và danh sách thì giữ nguyên.
        let out = checked("đặt n là 1\nđặt ds là [1]\nnếu n bằng với 1 { in(độ dài của ds) }").unwrap();
        assert!(out.contains("if (n == 1) {") && out.contains("System.out.println(ds.size());"), "{out}");
        // Nối chuỗi cho ra chữ, nên so sánh kết quả của nó cũng theo kiểu chữ.
        assert!(checked("đặt a là \"x\" cộng 1\nin(a bằng với \"x1\")").unwrap().contains("Objects.equals(a, \"x1\")"));
    }

    // ---- Hồi quy cho các lỗi tìm được khi thử bằng chương trình thật ----

    #[test]
    fn so_sanh_bang_theo_gia_tri_chu_khong_theo_dia_chi() {
        // Hai số cùng lấy từ danh sách: Java so địa chỉ của hai "hộp".
        let out = checked("đặt ds là [1000, 1000]\nđặt a là ds[0]\nvới mỗi x trong ds { in(x bằng với a) }\nin(ds[0] khác ds[1])").unwrap();
        assert!(out.contains("System.out.println((+x) == (+a));"), "{out}");
        assert!(out.contains("System.out.println((+ds.get(0)) != (+ds.get(1)));"), "{out}");
        // Một vế là số trần thì Java tự mở hộp, giữ nguyên cho dễ đọc.
        assert!(checked("đặt ds là [1]\nđặt n là 1\nin(ds[0] bằng với n)").unwrap().contains("ds.get(0) == n"));
        // Hai danh sách bằng nhau khi phần tử bằng nhau.
        assert!(checked("đặt a là [1]\nđặt b là [1]\nin(a bằng với b)").unwrap().contains("Objects.equals(a, b)"));
        assert_eq!(
            checked("đặt s là \"a\"\nin(s bằng với 'a')").unwrap_err(),
            ["phép toán này không dùng được giữa chữ và kí tự"]
        );
    }

    #[test]
    fn tran_so_duoc_bao_thay_vi_ra_so_sai() {
        let out = checked("đặt a là 1000000 nhân 1000000 cộng 1\nđặt b là 1.5 nhân 2\nđặt số nguyên lớn c là 8000000000 trừ a").unwrap();
        assert!(out.contains("var a = Math.addExact((Math.multiplyExact(1000000, 1000000)), 1);"), "{out}");
        assert!(out.contains("var b = 1.5 * 2;") && out.contains("long c = Math.subtractExact(8000000000L, a);"), "{out}");
    }

    #[test]
    fn vong_lap_dem_lui_va_canh_bao_khi_moc_nguoc() {
        assert!(checked("lặp k từ 3 lùi về 1 { in(k) }").unwrap().contains("for (var k = 3; k >= 1; k--) {"));
        let errors = checked("lặp k từ 3 đến 1 { in(k) }").unwrap_err();
        assert!(errors[0].contains("sẽ không chạy lần nào") && errors[0].contains("«lùi về»"), "{errors:?}");
        // Mốc tính ra lúc chạy thì không đoán: "từ 2 đến n chia 2" phải rỗng khi n nhỏ.
        assert!(checked("đặt n là 2\nlặp i từ 2 đến n chia 2 { in(i) }").is_ok());
    }

    #[test]
    fn cach_viet_tu_nhien_hon() {
        // "bằng" và "là" giữa một biểu thức là phép so sánh; phủ định ôm cả phép so sánh.
        assert!(java("nếu a bằng b thì { }").contains("if (a == b) {"));
        assert!(java("nếu a là 3 thì { }").contains("if (a == 3) {"));
        assert!(java("nếu không phải a nhỏ hơn b và c thì { }").contains("if ((!(a < b)) && c) {"));
        assert!(java("đặt x là -a nhân 2").contains("var x = (-a) * 2;"));
        // Xuống dòng trước "không thì", trước {, trước ) và ]; dấu phẩy cuối; dấu chấm phẩy.
        let out = java("nếu a\n{\n in(1)\n}\nkhông thì\n{\n in(2)\n}");
        assert!(out.contains("if (a) {") && out.contains("} else {"), "{out}");
        assert!(java("đặt ds là [\n 1,\n 2,\n]\nin(\n f(1,\n 2\n )\n)").contains("System.out.println(f(1, 2));"));
        assert!(java("đặt a là 1; đặt b là 2").contains("var a = 1;") && java("đặt a là 1; đặt b là 2").contains("var b = 2;"));
        assert!(java("/* bỏ qua\n nhiều dòng */ in(1) // và cuối dòng").contains("System.out.println(1);"));
        assert!(java("in()\nin liền \"a\"").contains("System.out.println();") && java("in liền \"a\"").contains("System.out.print(\"a\");"));
        // Từ khóa mở đầu một câu lệnh thì không được nuốt qua dòng: hai lệnh vẫn là hai lệnh.
        assert_eq!(java("nếu a { }\nnếu b { }").matches("if (").count(), 2);
    }

    #[test]
    fn bien_dung_chung_va_gan_phan_tu() {
        let out = checked_class(
            "đặt số thực số dư là 0\nHàm góp(số thực x) { số dư += x }\nHàm gốc {\n góp(5)\n in(số dư)\n đặt số dư là 1\n}",
        )
        .unwrap();
        assert!(out.contains("static double so_du = 0;") && out.contains("so_du += x;"), "{out}");
        assert_eq!(checked_class("Hàm f { in(vật lạ) }").unwrap_err(), ["biến «vật lạ» chưa được khai báo"]);
        let out = java("ds[0] += 1\nds[i] *= 2\nbảng[0][1] là 5\nxóa 20 khỏi ds\nxóa vị trí 0 khỏi ds\nsắp xếp lại ds");
        assert!(out.contains("ds.set(0, ds.get(0) + (1));") && out.contains("ds.set(i, ds.get(i) * (2));"), "{out}");
        assert!(out.contains("bang.get(0).set(1, 5);") && out.contains("ds.remove((Object) (20));"), "{out}");
        assert!(out.contains("ds.remove((int) (0));") && out.contains("Collections.sort(ds);"), "{out}");
    }

    #[test]
    fn chu_va_phep_toan_moi() {
        let out = checked("đặt tên là \"An\"\nin(tên[0])\nin(tên nhỏ hơn \"Bình\")\nin(tên chứa \"n\")\nvới mỗi x trong [1] { in(tên[x]) }").unwrap();
        assert!(out.contains("println(ten.charAt(0));") && out.contains("println(ten.compareTo(\"Bình\") < 0);"), "{out}");
        assert!(out.contains("println(ten.contains(\"n\"));") && out.contains("println(ten.charAt(x));"), "{out}");
        let out = checked("đặt a là 2 ^ 10\nđặt b là căn bậc hai của 16\nđặt c là trị tuyệt đối của -5\nin(10 chia hết cho 5)").unwrap();
        assert!(out.contains("var a = Math.pow(2, 10);") && out.contains("var b = Math.sqrt(16);"), "{out}");
        assert!(out.contains("var c = Math.abs((-5));") && out.contains("println(10 % 5 == 0);"), "{out}");
        assert!(checked("đặt số nguyên n là 5\nin(n[0])").unwrap_err()[0].contains("chỉ lấy phần tử theo vị trí được trên danh sách hoặc chữ"));
        assert!(run("in('abc')").unwrap_err().msg.contains("chỉ được đúng một kí tự"));
        assert!(java("đặt c là 'a'\nđặt d là '\\n'").contains("var c = 'a';"));
    }

    #[test]
    fn giai_thich_khi_ten_bi_cat_ngang() {
        let e = run("đặt đã đoán đúng là sai").unwrap_err();
        assert!(e.msg.contains("«đúng» là hằng (TRUE) của ngôn ngữ; nếu «đã đoán đúng» là một tên thì hãy đổi tên"), "{}", e.msg);
        let e = run("đặt số nguyên tố là 5\nin(số nguyên tố)").unwrap_err();
        assert!(e.msg.contains("nếu «số nguyên tố» là một tên"), "{}", e.msg);
        let e = run("đặt điểm 1 là 5").unwrap_err();
        assert!(e.msg.contains("hãy viết «điểm_1»"), "{}", e.msg);
        // Hai lệnh dính nhau trên một dòng không phải là "tên bị cắt".
        assert!(!run("in(a) đặt b là 1").unwrap_err().msg.contains("là một tên"));
    }

    #[test]
    fn chuong_trinh_phai_co_ham_goc() {
        assert_eq!(checked_class("Hàm phụ { }\n// không có Hàm gốc ở đây").unwrap_err().len(), 1);
        let lang = config::load(TEST_CONFIG).unwrap();
        let errors = transpile(&lang, "Lớp T {\n Hàm phụ { }\n}\n", "t.agn").err().unwrap();
        assert_eq!(errors[0].msg, "chương trình chưa có «hàm gốc», nơi bắt đầu chạy");
        assert_eq!(transpile(&lang, "// trống\n", "t.agn").err().unwrap()[0].msg, "tệp chưa có chương trình nào");
    }

    #[test]
    fn neu_long_sau_van_dich_nhanh() {
        // Trước đây mỗi tầng "nếu" làm thời gian dịch nhân ba; 30 tầng sẽ không bao giờ xong.
        let mut body = "in(1)".to_string();
        for _ in 0..30 {
            body = format!("nếu đúng thì {{\n{body}\n}}");
        }
        let started = std::time::Instant::now();
        assert!(java(&body).contains("System.out.println(1);"));
        assert!(started.elapsed().as_secs() < 2);
    }

    #[test]
    fn nhap_thang_vao_bien() {
        let out = checked("nhập số nguyên tuổi\nnhập chữ tên với lời nhắc \"Tên: \"\nnhập biệt danh\nin(tuổi cộng 1)\nin(tên bằng với biệt danh)").unwrap();
        assert!(out.contains("int tuoi = Integer.parseInt(Anature.NHAP.nextLine().trim());"), "{out}");
        assert!(out.contains("System.out.print(\"Tên: \"); String ten = Anature.NHAP.nextLine();"), "{out}");
        assert!(out.contains("String biet_danh = Anature.NHAP.nextLine();"), "{out}");
        // Biến có kiểu ngay từ lúc nhập: phép tính và phép so sánh theo đúng kiểu đó.
        assert!(out.contains("Math.addExact(tuoi, 1)") && out.contains("Objects.equals(ten, biet_danh)"), "{out}");
        assert_eq!(checked("nhập số thực a\nnhập chữ a").unwrap_err(), ["biến «a» đã được khai báo ở dòng 3"]);
        // Cách viết cũ, dùng nhập như một giá trị, vẫn còn.
        assert!(checked("đặt số nguyên a là 0\na là nhập số nguyên").is_ok());
    }

    #[test]
    fn tu_dem_truoc_ten() {
        // Có tên theo sau thì là đệm; hai cách viết cho ra cùng một mã.
        let a = checked("nhập số nguyên cho biến tuổi\nđặt danh sách số nguyên tên ds là [1]\nin(biến tuổi cộng giá trị của biến ds[0])\nbiến tuổi là 5").unwrap();
        let b = checked("nhập số nguyên tuổi\nđặt danh sách số nguyên ds là [1]\nin(tuổi cộng ds[0])\ntuổi là 5").unwrap();
        assert_eq!(a, b);
        let out = checked_class("Hàm chào(chữ tên người, số nguyên biến tuổi) { in(người cộng tuổi) }\nHàm gốc { chào(\"An\", 1) }").unwrap();
        assert!(out.contains("static void chao(String nguoi, int tuoi) {"), "{out}");
        // Đứng một mình thì chính nó là tên: biến «tên» vẫn dùng được như trước.
        let out = checked("nhập chữ tên\nđặt số nguyên biến là 2\ntên là tên cộng \"!\"\nin(\"{tên} {biến}\")\nin(tên[0])\nin(biến - 1)").unwrap();
        assert!(out.contains("String ten = Anature.NHAP.nextLine();") && out.contains("int bien = 2;"), "{out}");
        assert!(out.contains("ten = ten + \"!\";") && out.contains("println(\"\" + ten + \" \" + bien);"), "{out}");
        assert!(out.contains("println(ten.charAt(0));") && out.contains("println(Math.subtractExact(bien, 1));"), "{out}");
        assert_eq!(checked("in(biến chưa có)").unwrap_err(), ["biến «chưa có» chưa được khai báo"]);
    }

    #[test]
    fn lay_phan_tu_bang_chu() {
        // Ba cách viết của cùng một việc cho ra cùng một mã.
        let brackets = checked("đặt ds là [1, 2]\nđặt i là 0\nin(ds[i] cộng ds[1])\nds[i] là 5\nđặt bảng là [[1]]\nin(bảng[0][i])").unwrap();
        let nth = checked("đặt ds là [1, 2]\nđặt i là 0\nin(ds thứ i cộng ds thứ 1)\nds thứ i là 5\nđặt bảng là [[1]]\nin(bảng thứ 0 thứ i)").unwrap();
        let element = checked(
            "đặt ds là [1, 2]\nđặt i là 0\nin(phần tử thứ i của ds cộng thành phần thứ 1 của ds)\nphần tử thứ i của ds là 5\nđặt bảng là [[1]]\nin(phần tử thứ i của bảng[0])",
        )
        .unwrap();
        assert_eq!(brackets, nth);
        assert_eq!(brackets, element);
        assert!(brackets.contains("Math.addExact(ds.get(i), ds.get(1))") && brackets.contains("ds.set(i, 5);"), "{brackets}");
        // Kiểu vẫn theo: trên chữ là lấy kí tự, và so sánh hai phần tử vẫn theo giá trị.
        let out = checked("đặt tên là \"An\"\nin(tên thứ 0)\nin(kí tự thứ 1 của tên)\nđặt ds là [1000, 1000]\nin(ds thứ 0 bằng với phần tử thứ 1 của ds)");
        assert!(out.is_err(), "«kí tự» là tên kiểu, không phải cách lấy phần tử");
        let out = checked("đặt tên là \"An\"\nin(tên thứ 0)\nin(phần tử thứ 1 của tên)\nđặt ds là [1000, 1000]\nin(ds thứ 0 bằng với phần tử thứ 1 của ds)").unwrap();
        assert!(out.contains("println(ten.charAt(0));") && out.contains("println(ten.charAt(1));"), "{out}");
        assert!(out.contains("println((+ds.get(0)) == (+ds.get(1)));"), "{out}");
        // Vị trí là một toán hạng đơn; phép tính thì đặt trong ngoặc.
        let out = checked("đặt ds là [1, 2]\nđặt i là 0\nin(ds thứ (i cộng 1))\nin(ds thứ i cộng 1)").unwrap();
        assert!(out.contains("println(ds.get(Math.addExact(i, 1)));") && out.contains("println(Math.addExact(ds.get(i), 1));"), "{out}");
        // Lệnh xóa theo vị trí vẫn như cũ.
        assert!(java("xóa phần tử thứ 1 khỏi ds").contains("ds.remove((int) (1));"));
    }

    #[test]
    fn tu_dem_giua_ten_ham_va_tham_so() {
        let plain = checked_class("Hàm tổng(số nguyên a, số nguyên b) trả về số nguyên { trả về a cộng b }\nHàm gốc { in(tổng(1, 2)) }").unwrap();
        for (def, call) in [("từ ", "từ "), ("với tham gia ", "với tham số "), ("từ", "")] {
            let src = format!(
                "Hàm tổng {def}(số nguyên a, số nguyên b) trả về số nguyên {{ trả về a cộng b }}\nHàm gốc {{ in(tổng {call}(1, 2)) }}"
            );
            assert_eq!(checked_class(&src).unwrap(), plain, "{src}");
        }
        // Vòng lặp vẫn dùng "từ" theo nghĩa của nó, kể cả khi mốc đầu nằm trong ngoặc.
        assert!(checked("lặp i từ (1) đến 3 { in(i) }").unwrap().contains("for (var i = 1; i <= 3; i++) {"));
    }

    #[test]
    fn loi_tro_ve_ma_nguon_goc() {
        let e = run("đặt biến a là 5\nđặt biến b 2").unwrap_err();
        let span = e.span.unwrap();
        // 2 dòng bọc ngoài + dòng thứ hai của thân; con trỏ nằm ở số 2.
        assert_eq!((span.line, span.col), (4, 12));
        assert!(e.msg.contains("«là»") && e.msg.contains("«2»"), "{}", e.msg);
    }

    #[test]
    fn tu_dem_sai_vi_tri_bi_tu_choi() {
        assert!(run("đặt biến a thì là 5").is_err());
    }

    #[test]
    fn cau_hinh_trung_cum_bi_tu_choi() {
        let bad = TEST_CONFIG.replace("EQ  = { words = [\"bằng với\"", "EQ  = { words = [\"khi\", \"bằng với\"");
        assert_ne!(bad, TEST_CONFIG);
        let Err(e) = config::load(&bad) else { panic!("phải lỗi") };
        // Thông báo chỉ luôn cách gỡ khi đó là một từ khóa trùng với một toán tử.
        assert!(e.msg.contains("cụm «khi» được khai báo ở cả WHILE và EQ"), "{}", e.msg);
        assert!(e.msg.contains("thêm WHILE = \"EQ\" vào keyword_operators"), "{}", e.msg);
        // Đã có ánh xạ (ASSIGN đóng vai EQ) thì ghi cụm ở cả hai nơi là hợp lệ.
        let twice = TEST_CONFIG.replace("EQ  = { words = [\"bằng với\"", "EQ  = { words = [\"bằng\", \"bằng với\"");
        assert!(config::load(&twice).is_ok());
    }

    #[test]
    fn cau_hinh_di_kem_chuong_trinh_hop_le() {
        // language.toml ở thư mục gốc được nhúng vào chương trình làm cấu hình mặc định.
        if let Err(e) = config::load(DEFAULT_CONFIG) {
            panic!("language.toml đang hỏng: {}", e.msg);
        }
    }

    #[test]
    fn cum_co_so_va_tu_khoa_dong_vai_toan_tu() {
        let lang = config::load(TEST_CONFIG).unwrap();
        let src = "Lớp T {\nHàm gốc {\nin 1 dòng \"a\"\nin 1\nđặt a là 1 thêm 2\nthêm a vào ds\n}\n}\n";
        let toks = lexer::lex(&lang, src).unwrap();
        let out = codegen::generate(&lang, &parser::parse(&lang, src, &toks).unwrap(), "t.agn").java;
        assert!(out.contains("System.out.print(\"a\");") && out.contains("System.out.println(1);"), "{out}");
        // "thêm" mở đầu câu là lệnh thêm vào danh sách; giữa biểu thức là phép cộng.
        assert!(out.contains("var a = 1 + 2;") && out.contains("ds.add(a);"), "{out}");
    }
}
