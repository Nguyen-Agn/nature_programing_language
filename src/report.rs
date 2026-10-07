//! Dịch thông báo của `java`/`javac` về mã nguồn gốc: số dòng, tên, và câu chữ
//! (với những thông báo có trong [java.messages]). Người dùng không viết mã Java,
//! nên không thông báo nào được phép trỏ vào nó.

use std::collections::HashMap;

use crate::codegen::Output;
use crate::config::Lang;

const LINE_MARK: &str = " // dòng ";

pub struct Report<'a> {
    lang: &'a Lang,
    file: &'a str,
    src: &'a str,
    java_file: String,
    /// Dòng Java (tính từ 1) -> dòng nguồn.
    lines: Vec<Option<usize>>,
    /// Tên Java -> tên trong mã nguồn.
    names: HashMap<&'a str, &'a str>,
    /// Đang bỏ qua phần javac chép lại dòng Java kèm dấu ^.
    in_echo: bool,
    /// Khung gọi hàm đầu tiên của một lỗi lúc chạy được in kèm dòng nguồn.
    first_frame: bool,
    last_frame: String,
}

impl<'a> Report<'a> {
    pub fn new(lang: &'a Lang, file: &'a str, src: &'a str, java_file: &str, out: &'a Output) -> Self {
        // Mỗi câu lệnh sinh ra đều mang ghi chú số dòng; các dòng còn lại (dấu } đóng,
        // nhánh else) thuộc về câu lệnh ngay phía trên.
        let mut last = None;
        let lines = out
            .java
            .lines()
            .map(|l| {
                if let Some(n) = l.rfind(LINE_MARK).and_then(|i| l[i + LINE_MARK.len()..].parse().ok()) {
                    last = Some(n);
                }
                last
            })
            .collect();
        Report {
            lang,
            file,
            src,
            java_file: java_file.to_string(),
            lines,
            names: out.renamed.iter().map(|(src, java)| (java.as_str(), src.as_str())).collect(),
            in_echo: false,
            first_frame: true,
            last_frame: String::new(),
        }
    }

    /// Nhận một dòng từ stderr của `java`; trả về dòng cần in, hoặc None nếu bỏ qua.
    pub fn line(&mut self, raw: &str) -> Option<String> {
        if self.in_echo {
            // javac chép lại dòng Java rồi một dòng chỉ có dấu ^; phần chú giải sau đó thì giữ.
            self.in_echo = raw.trim() != "^";
            return None;
        }
        if let Some((java_line, kind, msg)) = self.diagnostic(raw) {
            self.in_echo = true;
            let label = if kind == "warning" { "cảnh báo" } else { "lỗi" };
            return Some(self.located(&format!("{label}: {}", self.translate(msg)), java_line, ""));
        }
        if let Some(rest) = raw.strip_prefix("Exception in thread ") {
            let what = rest.split_once("\" ").map_or(rest, |(_, w)| w);
            self.first_frame = true;
            self.last_frame.clear();
            return Some(format!("lỗi lúc chạy: {}", self.translate(what)));
        }
        if let Some(frame) = raw.trim_start().strip_prefix("at ") {
            return self.frame(frame);
        }
        let t = raw.trim();
        if t == "error: compilation failed" {
            return Some("không biên dịch được chương trình.".to_string());
        }
        if t.strip_suffix(" error").or(t.strip_suffix(" errors")).is_some_and(|n| n.parse::<usize>().is_ok()) {
            return None;
        }
        Some(self.source_names(raw))
    }

    /// Tách "…/Lop.java:26: error: thông báo" thành (26, "error", "thông báo").
    fn diagnostic<'r>(&self, raw: &'r str) -> Option<(usize, &'r str, &'r str)> {
        let key = format!("{}:", self.java_file);
        let rest = &raw[raw.find(&key)? + key.len()..];
        let (num, rest) = rest.split_once(':')?;
        let (kind, msg) = rest.trim_start().split_once(": ")?;
        matches!(kind, "error" | "warning").then_some((num.parse().ok()?, kind, msg))
    }

    /// Một khung trong vết gọi hàm: "Lop.ham(Lop.java:20)". Khung của JDK bị bỏ qua.
    fn frame(&mut self, frame: &str) -> Option<String> {
        let (method, loc) = frame.strip_suffix(')')?.split_once('(')?;
        let java_line: usize = loc.strip_prefix(&format!("{}:", self.java_file))?.parse().ok()?;
        let method = method.rsplit('.').next().unwrap_or(method);
        let method = self.names.get(method).copied().unwrap_or(method);
        if self.first_frame {
            self.first_frame = false;
            return Some(self.located("", java_line, &format!(", trong hàm «{method}»")));
        }
        let text = match self.source_line(java_line) {
            Some(n) => format!("    được gọi từ hàm «{method}» ({}:{n})", self.file),
            None => format!("    được gọi từ hàm «{method}»"),
        };
        // Đệ quy vô hạn in cả nghìn khung giống nhau.
        if text == self.last_frame {
            return None;
        }
        self.last_frame = text.clone();
        Some(text)
    }

    fn source_line(&self, java_line: usize) -> Option<usize> {
        *self.lines.get(java_line.checked_sub(1)?)?
    }

    /// `head`, rồi vị trí và nội dung dòng nguồn ứng với `java_line`.
    fn located(&self, head: &str, java_line: usize, suffix: &str) -> String {
        let mut out = head.to_string();
        let Some(n) = self.source_line(java_line) else {
            return out;
        };
        let num = n.to_string();
        let pad = " ".repeat(num.len());
        if !out.is_empty() {
            out.push('\n');
        }
        let text = self.src.lines().nth(n - 1).unwrap_or("");
        out.push_str(&format!("{pad}--> {}:{n}{suffix}\n{pad} |\n{num} | {text}", self.file));
        out
    }

    /// Thêm câu tiếng Việt cho những thông báo quen thuộc, giữ nguyên bản gốc trong ngoặc.
    fn translate(&self, msg: &str) -> String {
        let msg = self.source_names(msg);
        match self.lang.java.messages.iter().find(|(key, _)| msg.contains(key.as_str())) {
            Some((_, vi)) => format!("{vi} ({msg})"),
            None => msg,
        }
    }

    /// Đổi các tên Java đã bỏ dấu trở lại tên người dùng đã viết.
    fn source_names(&self, text: &str) -> String {
        let mut out = String::new();
        let mut word = String::new();
        for c in text.chars().chain(std::iter::once(' ')) {
            if c.is_ascii_alphanumeric() || c == '_' {
                word.push(c);
                continue;
            }
            out.push_str(self.names.get(word.as_str()).copied().unwrap_or(&word));
            word.clear();
            out.push(c);
        }
        out.pop();
        out
    }
}
