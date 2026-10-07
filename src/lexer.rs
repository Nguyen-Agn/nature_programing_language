//! Tách mã nguồn thành token nội bộ, giữ vị trí gốc của từng token.

use std::collections::HashMap;

use crate::config::Lang;
use crate::error::{Error, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Keyword(String),
    Literal(String),
    Op(String),
    Type(String),
    /// Từ đệm, kèm các nhóm đệm mà nó thuộc về.
    Filler(Vec<String>),
    Ident(String),
    Num(String),
    Str(String),
    /// Chuỗi nội suy: "Xin chào {tên}".
    Interp(Vec<Piece>),
    Sym(char),
    Newline,
    /// Vị trí con trỏ khi hỏi gợi ý: không khớp với gì, cũng không kết thúc câu lệnh.
    Cursor,
    Eof,
}

/// Một mảnh của chuỗi nội suy.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    /// Chữ nguyên văn, chưa có dấu nháy bao quanh.
    Text(String),
    /// Các token của một biểu thức trong {}, kết thúc bằng Eof.
    Code(Vec<Token>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

enum Atom {
    Word,
    Num,
    Str,
    Phrase(String),
    Sym(char),
    Newline,
}

/// Bước 1: cắt `text` thành các mảnh thô. Chuỗi ký tự và chú thích được nhận diện ở
/// đây, nên nội dung của chúng không bao giờ bị tra bảng từ khóa. `origin` là vị trí
/// của `text` trong tệp, để span luôn tính theo tệp gốc dù `text` chỉ là một đoạn.
fn atoms(lang: &Lang, text: &str, origin: Span) -> Result<(Vec<(Atom, Span)>, Span), Error> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let byte = |i: usize| origin.start + chars.get(i).map_or(text.len(), |c| c.0);
    let mut out = Vec::new();
    let (mut i, mut line, mut line_start) = (0, origin.line, 0);
    // Cột của ký tự thứ i: dòng đầu tiên của đoạn bắt đầu ở cột origin.col.
    let col = |i: usize, line: usize, line_start: usize| {
        i - line_start + if line == origin.line { origin.col } else { 1 }
    };

    while i < chars.len() {
        let c = chars[i].1;
        let start = i;
        let span = |end: usize| Span { start: byte(start), end: byte(end), line, col: col(start, line, line_start) };

        if c == '\n' {
            out.push((Atom::Newline, span(i + 1)));
            i += 1;
            line += 1;
            line_start = i;
        } else if c == ';' {
            // Dấu chấm phẩy kết thúc câu lệnh như một lần xuống dòng.
            i += 1;
            out.push((Atom::Newline, span(i)));
        } else if c.is_whitespace() {
            i += 1;
        } else if text[chars[i].0..].starts_with("//") {
            while i < chars.len() && chars[i].1 != '\n' {
                i += 1;
            }
        } else if text[chars[i].0..].starts_with("/*") {
            let opened = span(i + 2);
            i += 2;
            loop {
                if i >= chars.len() {
                    return Err(Error::at(opened, "chú thích /* chưa có */ đóng"));
                }
                if text[chars[i].0..].starts_with("*/") {
                    i += 2;
                    break;
                }
                if chars[i].1 == '\n' {
                    line += 1;
                    line_start = i + 1;
                }
                i += 1;
            }
        } else if c == '"' || c == '\'' {
            i += 1;
            loop {
                match chars.get(i).map(|c| c.1) {
                    None | Some('\n') => return Err(Error::at(span(start + 1), format!("thiếu dấu {c} đóng"))),
                    Some('\\') => i += 2,
                    Some(q) if q == c => break,
                    Some(_) => i += 1,
                }
            }
            i += 1;
            out.push((Atom::Str, span(i)));
        } else if c.is_ascii_digit() {
            while i < chars.len() && chars[i].1.is_ascii_digit() {
                i += 1;
            }
            if chars.get(i).map(|c| c.1) == Some('.') && chars.get(i + 1).is_some_and(|c| c.1.is_ascii_digit()) {
                i += 1;
                while i < chars.len() && chars[i].1.is_ascii_digit() {
                    i += 1;
                }
            }
            out.push((Atom::Num, span(i)));
        } else if c.is_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].1.is_alphanumeric() || chars[i].1 == '_') {
                i += 1;
            }
            out.push((Atom::Word, span(i)));
        } else if let Some(sym) = lang.symbols.iter().find(|s| text[chars[i].0..].starts_with(s.as_str())) {
            i += sym.chars().count();
            out.push((Atom::Phrase(sym.clone()), span(i)));
        } else if "{}()[],".contains(c) {
            i += 1;
            out.push((Atom::Sym(c), span(i)));
        } else {
            return Err(Error::at(span(i + 1), format!("ký tự «{c}» không thuộc ngôn ngữ")));
        }
    }
    let eof = Span { start: byte(chars.len()), end: byte(chars.len()), line, col: col(chars.len(), line, line_start) };
    Ok((out, eof))
}

/// Tên không phân biệt hoa/thường: mọi lần xuất hiện dùng cách viết của lần đầu.
type Names = HashMap<String, String>;

/// Tách một chuỗi "..." thành chữ nguyên văn và các biểu thức trong {}.
/// Viết \{ và \} để có dấu ngoặc nhọn thật.
fn string(lang: &Lang, src: &str, span: Span, names: &mut Names) -> Result<Tok, Error> {
    let inner = Span { start: span.start + 1, end: span.end - 1, col: span.col + 1, ..span };
    let content = &src[inner.start..inner.end];
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut chars = content.char_indices();
    // Vị trí trong tệp của ký tự ở byte `at` trong nội dung chuỗi.
    let here = |at: usize| Span {
        start: inner.start + at,
        end: inner.start + at + 1,
        line: span.line,
        col: inner.col + content[..at].chars().count(),
    };
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some((_, brace @ ('{' | '}'))) => text.push(brace),
                Some((_, other)) => text.extend(['\\', other]),
                None => text.push('\\'),
            },
            '{' => {
                let Some(len) = content[at..].find('}') else {
                    return Err(Error::at(here(at), "thiếu dấu } đóng trong chuỗi; viết \\{ nếu muốn dấu { thật"));
                };
                let code = &content[at + 1..at + len];
                if code.trim().is_empty() {
                    return Err(Error::at(here(at), "trong {} của chuỗi cần một biểu thức"));
                }
                let (raw, eof) = atoms(lang, code, here(at + 1))?;
                let mut toks = tokens(lang, src, &raw, names)?;
                toks.push(Token { tok: Tok::Eof, span: eof });
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Code(toks));
                // Bỏ qua phần vừa xử lý, kể cả dấu } đóng.
                while chars.next().is_some_and(|(i, _)| i < at + len) {}
            }
            _ => text.push(c),
        }
    }
    if pieces.is_empty() {
        return Ok(Tok::Str(format!("\"{text}\"")));
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(Tok::Interp(pieces))
}

/// Bước 2: gộp các từ liền nhau thành cụm dài nhất có trong bảng; những từ còn lại
/// đứng liền nhau gộp thành một tên nhiều từ.
fn tokens(lang: &Lang, src: &str, raw: &[(Atom, Span)], names: &mut Names) -> Result<Vec<Token>, Error> {
    let text = |k: usize| &src[raw[k].1.start..raw[k].1.end];
    // Một cụm có thể xen số nguyên ở các từ phía sau ("in 1 dòng").
    let continues = |k: usize| match raw[k].0 {
        Atom::Word => true,
        Atom::Num => text(k).chars().all(|c| c.is_ascii_digit()),
        _ => false,
    };
    let phrase_at = |k: usize| {
        let mut run = 1;
        while run < lang.max_words && k + run < raw.len() && continues(k + run) {
            run += 1;
        }
        let lower: Vec<String> = (0..run).map(|d| text(k + d).to_lowercase()).collect();
        (1..=run).rev().find_map(|n| lang.phrases.get(&lower[..n].join(" ")).map(|t| (n, t)))
    };
    let mut out: Vec<Token> = Vec::new();

    let mut k = 0;
    while k < raw.len() {
        let span = raw[k].1;
        match &raw[k].0 {
            Atom::Word => {
                if let Some((n, tok)) = phrase_at(k) {
                    out.push(Token { tok: tok.clone(), span: Span { end: raw[k + n - 1].1.end, ..span } });
                    k += n;
                    continue;
                }
                // Tên kéo dài đến khi gặp một cụm của ngôn ngữ, hoặc thứ gì không phải từ.
                let mut end = k + 1;
                while end < raw.len() && matches!(raw[end].0, Atom::Word) && phrase_at(end).is_none() {
                    end += 1;
                }
                let words: Vec<&str> = (k..end).map(text).collect();
                let name = names.entry(words.join(" ").to_lowercase()).or_insert_with(|| words.join(" "));
                out.push(Token { tok: Tok::Ident(name.clone()), span: Span { end: raw[end - 1].1.end, ..span } });
                k = end;
                continue;
            }
            Atom::Phrase(key) => out.push(Token { tok: lang.phrases[key].clone(), span }),
            Atom::Num => out.push(Token { tok: Tok::Num(text(k).to_string()), span }),
            // 'ký tự' được chép nguyên sang mã đích; "chuỗi" có thể chứa {biểu thức}.
            Atom::Str if text(k).starts_with('\'') => {
                let inner: Vec<char> = text(k)[1..text(k).len() - 1].chars().collect();
                if !matches!(inner.as_slice(), [c] if *c != '\\') && !matches!(inner.as_slice(), ['\\', _]) {
                    return Err(Error::at(span, "trong dấu nháy đơn chỉ được đúng một kí tự; chữ thì viết trong dấu nháy kép"));
                }
                out.push(Token { tok: Tok::Str(text(k).to_string()), span })
            }
            Atom::Str => out.push(Token { tok: string(lang, src, span, names)?, span }),
            Atom::Sym(c) => out.push(Token { tok: Tok::Sym(*c), span }),
            Atom::Newline => {
                // Như Go: dòng kết thúc bằng dấu phẩy, toán tử, ( hoặc [ thì câu lệnh còn tiếp.
                let ends_stmt = !matches!(
                    out.last().map(|t| &t.tok),
                    None | Some(Tok::Newline | Tok::Op(_) | Tok::Sym(',' | '(' | '['))
                );
                if ends_stmt {
                    out.push(Token { tok: Tok::Newline, span });
                }
            }
        }
        k += 1;
    }
    Ok(out)
}

pub fn lex(lang: &Lang, src: &str) -> Result<Vec<Token>, Error> {
    let origin = Span { start: 0, end: 0, line: 1, col: 1 };
    let (raw, eof) = atoms(lang, src, origin)?;
    let mut out = tokens(lang, src, &raw, &mut Names::new())?;
    out.push(Token { tok: Tok::Eof, span: eof });
    Ok(out)
}
