//! Phát mã Java từ cây trung gian, dùng bảng ánh xạ cuối trong cấu hình.

use std::collections::{HashMap, HashSet};

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::config::{Lang, Part};
use crate::parser::{Expr, Node, Part as StrPart, Type, Value};

const INDENT: &str = "    ";

/// Những tên mà mã sinh ra không được đụng tới: từ khóa Java và vài tên mà các mẫu dùng.
const RESERVED: &[&str] = &[
    "_", "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const",
    "continue", "default", "do", "double", "else", "enum", "extends", "false", "final", "finally",
    "float", "for", "goto", "if", "implements", "import", "instanceof", "int", "interface", "long",
    "native", "new", "null", "package", "private", "protected", "public", "record", "return", "short",
    "static", "strictfp", "super", "switch", "synchronized", "this", "throw", "throws", "transient",
    "true", "try", "var", "void", "volatile", "while", "yield", "args", "main", "string", "system",
];

struct Gen<'a> {
    lang: &'a Lang,
    /// Tên trong mã nguồn -> tên Java. Mỗi tên nguồn có đúng một tên Java riêng.
    names: HashMap<String, String>,
    used: HashSet<String>,
    renamed: Vec<(String, String)>,
}

pub struct Output {
    /// Tên lớp chính, quyết định tên file .java.
    pub class: Option<String>,
    pub java: String,
    /// Các cặp (tên trong mã nguồn, tên Java) đã bị đổi.
    pub renamed: Vec<(String, String)>,
}

pub fn generate(lang: &Lang, nodes: &[Node], source: &str) -> Output {
    let mut g = Gen { lang, names: HashMap::new(), used: HashSet::new(), renamed: Vec::new() };
    let mut body = String::new();
    for n in nodes {
        body.push('\n');
        body.push_str(&g.node(n, 0, true));
        body.push('\n');
    }
    let mut out = format!("// Sinh tự động từ {source} — sửa tệp nguồn, đừng sửa tệp này.\n");
    if let Some(header) = &lang.java.header {
        out.push_str(header);
        out.push('\n');
    }
    if !g.renamed.is_empty() {
        out.push_str("// Tên đã đổi sang ASCII:\n");
        for (from, to) in &g.renamed {
            out.push_str(&format!("//   {from} -> {to}\n"));
        }
    }
    out.push_str(&body);
    if let Some(footer) = &lang.java.footer {
        out.push('\n');
        out.push_str(footer);
        out.push('\n');
    }
    Output { class: g.class_name(nodes), java: out, renamed: g.renamed }
}

/// Bỏ dấu tiếng Việt để tên chỉ còn chữ ASCII.
pub fn ascii(name: &str) -> String {
    let mut out = String::new();
    for c in name.nfd() {
        match c {
            'đ' => out.push('d'),
            'Đ' => out.push('D'),
            c if c.is_ascii_alphanumeric() || c == '_' => out.push(c),
            c if is_combining_mark(c) => {}
            _ => out.push('_'),
        }
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// Thay {khóa} trong mẫu bằng giá trị tương ứng; {…} lạ được giữ nguyên.
fn fill(tpl: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut rest = tpl;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open + 1..];
        let hit = tail.find('}').and_then(|end| vars.iter().find(|(k, _)| *k == &tail[..end]).map(|(_, v)| (end, v)));
        match hit {
            Some((end, v)) => {
                out.push_str(v);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('{');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

impl Gen<'_> {
    /// Hai tên nguồn khác nhau không bao giờ ra cùng một tên Java: tên đến sau
    /// mà trùng (kể cả chỉ khác hoa/thường) thì được gắn thêm số.
    fn name(&mut self, src: &str) -> String {
        if let Some(n) = self.names.get(src) {
            return n.clone();
        }
        let base = ascii(src);
        let mut out = base.clone();
        let mut n = 2;
        while self.used.contains(&out.to_lowercase())
            || RESERVED.contains(&out.to_lowercase().as_str())
            || self.lang.java.reserved.contains(&out.to_lowercase())
        {
            out = format!("{base}_{n}");
            n += 1;
        }
        self.used.insert(out.to_lowercase());
        self.names.insert(src.to_string(), out.clone());
        if out != src {
            self.renamed.push((src.to_string(), out.clone()));
        }
        out
    }

    fn class_name(&self, nodes: &[Node]) -> Option<String> {
        let (rule, slot) = (self.lang.java.class_rule.as_ref()?, self.lang.java.class_slot.as_ref()?);
        nodes.iter().filter(|n| &self.lang.rules[n.rule].name == rule).find_map(|n| {
            n.slots.iter().find_map(|(k, v)| match v {
                Value::Name(name, _) if k == slot => self.names.get(name).cloned(),
                _ => None,
            })
        })
    }

    /// `mark`: ghi chú số dòng nguồn ở cuối dòng đầu tiên của câu lệnh, để dò ngược khi debug.
    fn node(&mut self, n: &Node, depth: usize, mark: bool) -> String {
        let lang = self.lang;
        let mut s = String::new();
        for part in &lang.java.rules[&lang.rules[n.rule].name] {
            match part {
                Part::Text(t) => s.push_str(t),
                Part::Slot(name) => {
                    let (_, v) = n.slots.iter().find(|(k, _)| k == name).expect("đã kiểm tra khi nạp cấu hình");
                    s.push_str(&self.value(v, depth));
                }
            }
        }
        if mark {
            let note = format!(" // dòng {}", n.span.line);
            match s.find('\n') {
                Some(i) => s.insert_str(i, &note),
                None => s.push_str(&note),
            }
        }
        s
    }

    fn value(&mut self, v: &Value, depth: usize) -> String {
        match v {
            Value::Name(name, _) => self.name(name),
            Value::Expr(e, _) => self.expr(e),
            Value::Type(t) => self.ty(t, false),
            Value::Params(params) => {
                let parts: Vec<String> =
                    params.iter().map(|(t, n)| format!("{} {}", self.ty(t, false), self.name(n))).collect();
                parts.join(", ")
            }
            Value::Stmt(n) => self.node(n, depth, false),
            Value::Block(nodes) => {
                let mut s = String::from("{\n");
                for n in nodes {
                    s.push_str(&INDENT.repeat(depth + 1));
                    s.push_str(&self.node(n, depth + 1, true));
                    s.push('\n');
                }
                s.push_str(&INDENT.repeat(depth));
                s.push('}');
                s
            }
        }
    }

    /// `boxed`: kiểu nằm trong danh sách, nơi Java đòi Integer thay cho int.
    fn ty(&self, t: &Type, boxed: bool) -> String {
        let java = &self.lang.java;
        match t {
            Type::Simple(g) if boxed => java.boxed.get(g).unwrap_or(&java.types[g]).clone(),
            Type::Simple(g) => java.types[g].clone(),
            Type::List(inner) => fill(&java.lists.ty, &[("kiểu", &self.ty(inner, true))]),
        }
    }

    /// Luôn đóng ngoặc biểu thức con, vì thứ tự ưu tiên trong cấu hình có thể khác của Java.
    fn wrapped(&mut self, e: &Expr) -> String {
        match e {
            Expr::Binary(..) | Expr::Prefix(..) | Expr::Interp(_) => format!("({})", self.expr(e)),
            _ => self.expr(e),
        }
    }

    fn expr(&mut self, e: &Expr) -> String {
        let lang = self.lang;
        match e {
            // Số nguyên vượt quá kiểu số nguyên thường cần hậu tố riêng trong mã đích.
            Expr::Num(s) if !s.contains('.') && s.parse::<i32>().is_err() => {
                format!("{s}{}", lang.java.long_suffix.as_deref().unwrap_or(""))
            }
            Expr::Num(s) | Expr::Str(s) => s.clone(),
            Expr::Var(v, _) => self.name(v),
            Expr::Interp(parts) => {
                // Luôn mở đầu bằng một chuỗi, để phép nối không bị hiểu thành phép cộng số.
                let mut out = match parts.first() {
                    Some(StrPart::Text(_)) => String::new(),
                    _ => "\"\"".to_string(),
                };
                for part in parts {
                    let piece = match part {
                        StrPart::Text(text) => format!("\"{text}\""),
                        StrPart::Code(code) => self.wrapped(code),
                    };
                    out = if out.is_empty() { piece } else { fill(&lang.java.concat, &[("a", &out), ("b", &piece)]) };
                }
                out
            }
            Expr::Lit(l) => lang.java.literals[l].clone(),
            Expr::Call(f, _, args) => {
                let f = self.name(f);
                let args: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                format!("{f}({})", args.join(", "))
            }
            Expr::List(items) => {
                let items: Vec<String> = items.iter().map(|a| self.expr(a)).collect();
                fill(&lang.java.lists.literal, &[("phần_tử", &items.join(", "))])
            }
            Expr::Index(list, index) => {
                let (list, index) = (self.wrapped(list), self.expr(index));
                fill(&lang.java.lists.index, &[("danh_sách", &list), ("vị_trí", &index)])
            }
            Expr::Rule(n) => self.node(n, 0, false),
            // Toán tử có mẫu riêng ({a}, {b}) thay vì chỉ là một ký hiệu, ví dụ "{a}.size()".
            Expr::Prefix(op, x) if lang.java.operators[op].contains("{a}") => {
                let a = self.wrapped(x);
                fill(&lang.java.operators[op], &[("a", &a)])
            }
            Expr::Binary(op, l, r) if lang.java.operators[op].contains("{a}") => {
                let (a, b) = (self.wrapped(l), self.wrapped(r));
                fill(&lang.java.operators[op], &[("a", &a), ("b", &b)])
            }
            Expr::Prefix(op, x) => format!("{}{}", lang.java.operators[op], self.wrapped(x)),
            Expr::Binary(op, l, r) => {
                // a + b + c: cùng toán tử ở vế trái thì Java đọc giống hệt, không cần ngoặc.
                let left = match &**l {
                    Expr::Binary(lop, ..) if lop == op => self.expr(l),
                    _ => self.wrapped(l),
                };
                let right = self.wrapped(r);
                format!("{left} {} {right}", lang.java.operators[op])
            }
        }
    }
}
