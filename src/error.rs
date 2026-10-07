/// Vị trí của một token trong mã nguồn gốc (sau khi chuẩn hóa NFC).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug)]
pub struct Error {
    pub msg: String,
    pub span: Option<Span>,
    /// Một tên không tìm thấy, kèm các tên đang có ở chỗ đó. Chỉ pha soạn thảo dùng,
    /// để đề xuất tên gần đúng; pha thực thi không đoán.
    pub unknown: Option<(String, Vec<String>)>,
}

impl Error {
    pub fn new(msg: impl Into<String>) -> Self {
        Error { msg: msg.into(), span: None, unknown: None }
    }

    pub fn at(span: Span, msg: impl Into<String>) -> Self {
        Error { msg: msg.into(), span: Some(span), unknown: None }
    }

    /// Dựng thông báo kèm dòng mã nguồn và dấu ^ chỉ đúng vị trí.
    pub fn render(&self, file: &str, src: &str) -> String {
        let Some(sp) = self.span else {
            return format!("lỗi: {}", self.msg);
        };
        let text = src.lines().nth(sp.line - 1).unwrap_or("");
        let num = sp.line.to_string();
        let pad = " ".repeat(num.len());
        // Giữ nguyên tab để dấu ^ thẳng cột với dòng phía trên.
        let lead: String = text
            .chars()
            .take(sp.col - 1)
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();
        let carets = "^".repeat(src[sp.start..sp.end].chars().count().max(1));
        format!(
            "lỗi: {}\n{pad}--> {file}:{}:{}\n{pad} |\n{num} | {text}\n{pad} | {lead}{carets}",
            self.msg, sp.line, sp.col
        )
    }
}
