//! Máy chủ gợi ý cho trình soạn thảo, nói giao thức LSP qua stdin/stdout.
//!
//! Đây là pha soạn thảo: gợi ý khi gõ lấy thẳng từ tập "cái gì hợp lệ ở đây" của
//! parser nên vẫn xác định; chỉ riêng "có phải bạn muốn..." là ước lượng, và nó
//! chỉ thêm chữ vào thông báo lỗi chứ không bao giờ quyết định việc phân tích.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde_json::{Value, json};
use unicode_normalization::{UnicodeNormalization, is_nfc};

use crate::codegen::ascii;
use crate::config::{Elem, Lang, Rule, Scope, SlotKind, norm};
use crate::lexer::{self, Piece, Tok, Token};
use crate::parser::{self, Expect};

/// Các loại token tô màu, theo đúng thứ tự gửi cho trình soạn thảo.
const TOKEN_TYPES: &[&str] = &["keyword", "type", "operator", "comment", "function", "variable", "number", "string"];

// Mã CompletionItemKind của LSP.
const KIND_TEXT: u8 = 1;
const KIND_VARIABLE: u8 = 6;
const KIND_CLASS: u8 = 7;
const KIND_KEYWORD: u8 = 14;
const KIND_SNIPPET: u8 = 15;
const KIND_CONSTANT: u8 = 21;
const KIND_OPERATOR: u8 = 24;

struct Doc {
    text: String,
    line_starts: Vec<usize>,
}

impl Doc {
    fn new(text: &str) -> Doc {
        // Lexer cần NFC. Văn bản gõ từ bàn phím gần như luôn đã là NFC; nếu không,
        // vị trí báo về có thể lệch vài cột trên dòng đó.
        let text: String = if is_nfc(text) { text.to_string() } else { text.nfc().collect() };
        // Giữ nguyên độ dài để vị trí không lệch: thay dấu thứ tự byte đầu tệp bằng dấu cách.
        let text = if text.starts_with('\u{feff}') { text.replacen('\u{feff}', " ", 1) } else { text };
        let line_starts = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
        Doc { text, line_starts }
    }

    /// Vị trí LSP (dòng, cột tính bằng đơn vị UTF-16) -> byte trong văn bản.
    fn offset(&self, line: usize, col16: usize) -> usize {
        let Some(&start) = self.line_starts.get(line) else {
            return self.text.len();
        };
        let mut units = 0;
        for (i, c) in self.text[start..].char_indices() {
            if units >= col16 || c == '\n' {
                return start + i;
            }
            units += c.len_utf16();
        }
        self.text.len()
    }

    fn position(&self, line: usize, byte: usize) -> Value {
        let start = self.line_starts[line];
        json!({ "line": line, "character": self.text[start..byte].encode_utf16().count() })
    }
}

struct Candidate {
    label: String,
    detail: String,
    kind: u8,
    /// Khung câu lệnh theo cú pháp snippet của LSP; None nếu chỉ chèn chính `label`.
    snippet: Option<String>,
    /// Khung viết bằng một cách viết đồng nghĩa: chỉ hiện khi người dùng đã gõ tới nó.
    variant: bool,
}

/// Khung của cả một câu lệnh, dựng từ luật: "lặp ‹biến› từ ‹đầu› đến ‹cuối› { }".
/// `first` là cách viết của từ khóa mở đầu. Trả về (nhãn hiển thị, thân snippet).
fn statement_frame(lang: &Lang, rule: &Rule, first: &str) -> Option<(String, String)> {
    let (mut label, mut body): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let blocks = rule.elems.iter().filter(|e| matches!(e, Elem::Slot { kind: SlotKind::Block(_), .. })).count();
    let (mut stop, mut block) = (0, 0);
    for (i, elem) in rule.elems.iter().enumerate() {
        match elem {
            Elem::Keyword(_) if i == 0 => {
                label.push(first.to_string());
                body.push(first.to_string());
            }
            Elem::Keyword(g) => {
                let words = &lang.words[g];
                let word = words.iter().find(|w| w.starts_with(char::is_alphabetic)).or(words.first())?;
                label.push(word.clone());
                body.push(word.clone());
            }
            Elem::Opt(_) | Elem::Many(_) => {}
            Elem::Sym(c) => {
                label.push(c.to_string());
                body.push(c.to_string());
            }
            Elem::Slot { name, kind } => {
                let shown = name.replace('_', " ");
                match kind {
                    SlotKind::Call => return None,
                    SlotKind::Block(_) => {
                        block += 1;
                        label.push("{ }".to_string());
                        // Con trỏ dừng cuối cùng ở khối sau chót.
                        let inside = if block == blocks { "$0".to_string() } else { stop += 1; format!("${stop}") };
                        body.push(format!("{{\n\t{inside}\n}}"));
                    }
                    SlotKind::Params => {
                        stop += 1;
                        label.push(format!("(‹{shown}›)"));
                        body.push(format!("(${{{stop}:{shown}}})"));
                    }
                    _ => {
                        stop += 1;
                        label.push(format!("‹{shown}›"));
                        body.push(format!("${{{stop}:{shown}}}"));
                    }
                }
            }
        }
    }
    (stop > 0 || blocks > 0).then(|| (label.join(" "), body.join(" ")))
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Mọi cụm chữ trong từ vựng có token thỏa `keep`, theo thứ tự bảng chữ cái.
fn vocabulary(lang: &Lang, out: &mut Vec<Candidate>, keep: impl Fn(&Tok) -> bool) {
    let mut found: Vec<(&String, &Tok)> = lang
        .phrases
        .iter()
        .filter(|(phrase, tok)| phrase.starts_with(char::is_alphabetic) && keep(tok))
        .collect();
    found.sort_by_key(|(phrase, _)| *phrase);
    for (phrase, tok) in found {
        let (kind, detail) = match tok {
            Tok::Keyword(g) => (KIND_KEYWORD, format!("từ khóa {g}")),
            Tok::Type(g) => (KIND_CLASS, format!("kiểu {g}")),
            Tok::Literal(g) => (KIND_CONSTANT, format!("hằng {g}")),
            Tok::Op(g) => (KIND_OPERATOR, format!("toán tử {g}")),
            _ => (KIND_TEXT, "từ đệm".to_string()),
        };
        out.push(Candidate { label: phrase.clone(), detail, kind, snippet: None, variant: false });
    }
}

/// Đổi tập "cái gì hợp lệ" của parser thành các cụm cụ thể để gợi ý.
fn expand(lang: &Lang, expected: &[Expect], names: &[String]) -> Vec<Candidate> {
    let mut out = Vec::new();
    let group = |out: &mut Vec<Candidate>, g: &str| {
        vocabulary(lang, out, |t| match t {
            Tok::Keyword(x) | Tok::Type(x) => x == g,
            Tok::Filler(groups) => groups.iter().any(|x| x == g),
            _ => false,
        })
    };
    let known_names = |out: &mut Vec<Candidate>| {
        for n in names {
            out.push(Candidate { label: n.clone(), detail: "tên đã dùng".to_string(), kind: KIND_VARIABLE, snippet: None, variant: false });
        }
    };
    for e in expected {
        match e {
            Expect::Group(g) => group(&mut out, g),
            Expect::Sym(c) => {
                out.push(Candidate { label: c.to_string(), detail: "ký hiệu".to_string(), kind: KIND_TEXT, snippet: None, variant: false })
            }
            // Đầu một câu lệnh: ngoài từng từ khóa, đề xuất luôn khung của cả câu.
            Expect::Start(scope) => {
                for rule in lang.rules.iter().filter(|r| r.scope == *scope) {
                    let Some(Elem::Keyword(g)) = rule.elems.first() else { continue };
                    let words = lang.words[g].iter().filter(|w| w.starts_with(char::is_alphabetic));
                    for (i, word) in words.enumerate() {
                        if let Some((label, body)) = statement_frame(lang, rule, &norm(word)) {
                            let detail = format!("khung câu {}", rule.name);
                            out.push(Candidate { label, detail, kind: KIND_SNIPPET, snippet: Some(body), variant: i > 0 });
                        }
                    }
                }
            }
            Expect::Name => known_names(&mut out),
            Expect::Operator => vocabulary(lang, &mut out, |t| matches!(t, Tok::Op(g) if lang.ops[g].infix)),
            Expect::Type => {
                vocabulary(lang, &mut out, |t| matches!(t, Tok::Type(_)));
                if let Some(g) = &lang.list_keyword {
                    group(&mut out, g);
                }
            }
            Expect::Expr => {
                known_names(&mut out);
                vocabulary(lang, &mut out, |t| matches!(t, Tok::Literal(_)));
                for rule in lang.rules.iter().filter(|r| r.scope == Scope::Expr) {
                    if let Some(Elem::Keyword(g)) = rule.elems.first() {
                        group(&mut out, g);
                    }
                }
                vocabulary(lang, &mut out, |t| matches!(t, Tok::Op(g) if lang.ops[g].prefix));
                if let Some(g) = &lang.operand_fillers {
                    group(&mut out, g);
                }
            }
            Expect::Newline => {}
        }
    }
    let mut seen = Vec::new();
    out.retain(|c| !seen.contains(&c.label) && { seen.push(c.label.clone()); true });
    // Từ đơn trước, khung câu sau, ký hiệu cuối cùng: ký hiệu hợp lệ nhưng hiếm khi
    // là thứ người dùng đang tìm.
    out.sort_by_key(|c| if !c.label.starts_with(is_word_char) { 2 } else { u8::from(c.snippet.is_some()) });
    out
}

/// Mọi token theo thứ tự xuất hiện, kể cả các token nằm trong {} của chuỗi nội suy.
fn flatten(toks: &[Token]) -> Vec<&Token> {
    let mut out = Vec::new();
    for t in toks {
        match &t.tok {
            Tok::Interp(pieces) => {
                for piece in pieces {
                    if let Piece::Code(code) = piece {
                        out.extend(flatten(code).into_iter().filter(|t| t.tok != Tok::Eof));
                    }
                }
            }
            _ => out.push(t),
        }
    }
    out
}

/// Những cụm hợp lệ nếu con trỏ đứng ở byte `cut`, coi như sau đó chưa có gì.
fn candidates_at(lang: &Lang, text: &str, cut: usize) -> Vec<Candidate> {
    let prefix = &text[..cut];
    let Ok(mut toks) = lexer::lex(lang, prefix) else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    for t in flatten(&toks) {
        if let Tok::Ident(n) = &t.tok {
            if !names.contains(n) {
                names.push(n.clone());
            }
        }
    }
    let eof = toks.pop().expect("lexer luôn kết thúc bằng Eof");
    toks.push(Token { tok: Tok::Cursor, span: eof.span });
    toks.push(eof);
    expand(lang, &parser::expected_at_cursor(lang, prefix, &toks), &names)
}

fn completion(lang: &Lang, doc: &Doc, line: usize, col16: usize) -> Vec<Value> {
    let Some(&line_start) = doc.line_starts.get(line) else {
        return Vec::new();
    };
    let cursor = doc.offset(line, col16);
    let before = &doc.text[line_start..cursor];

    // Một cụm có thể gồm nhiều từ ("mỗi lúc"), nên phần đang gõ dở có thể là một
    // từ hay vài từ cuối dòng. Thử lần lượt từng cách cắt.
    let mut head = before.trim_end_matches(is_word_char);
    let mut cuts = vec![head.len()];
    // Tên của người dùng cũng có thể gồm nhiều từ, không chỉ cụm của ngôn ngữ.
    for _ in 1..lang.max_words.max(4) {
        let spaces = head.trim_end_matches(' ');
        let word = spaces.trim_end_matches(is_word_char);
        if word.len() == spaces.len() {
            break;
        }
        head = word;
        cuts.push(head.len());
    }

    let mut items = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (depth, cut) in cuts.into_iter().enumerate() {
        let typed = &before[cut..];
        let typed_norm = norm(typed);
        if depth > 0 && typed_norm.is_empty() {
            continue;
        }
        let capital = typed.starts_with(char::is_uppercase);
        for c in candidates_at(lang, &doc.text, line_start + cut) {
            let key = norm(&c.label);
            let fits = key.starts_with(&typed_norm) && !(depth > 0 && key == typed_norm);
            // Chưa gõ gì thì mỗi khung câu chỉ hiện một lần, với cách viết đầu tiên.
            if !fits || seen.contains(&key) || (c.variant && typed_norm.is_empty()) {
                continue;
            }
            seen.push(key);
            // Giữ chữ hoa đầu câu nếu người dùng đã gõ hoa.
            let cased = |text: String| match (capital, text.chars().next()) {
                (true, Some(f)) => f.to_uppercase().chain(text.chars().skip(1)).collect(),
                _ => text,
            };
            let label = cased(c.label);
            let is_snippet = c.snippet.is_some();
            items.push(json!({
                "label": label,
                "kind": c.kind,
                "detail": c.detail,
                "sortText": format!("{:04}", items.len()),
                "insertTextFormat": if is_snippet { 2 } else { 1 },
                "textEdit": {
                    "range": { "start": doc.position(line, line_start + cut), "end": doc.position(line, cursor) },
                    "newText": c.snippet.map(cased).unwrap_or_else(|| label.clone()),
                },
            }));
        }
    }
    items
}

/// Khoảng cách sửa đổi giữa hai chuỗi (Levenshtein).
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut diag = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let above = row[j];
            row[j] = (diag + usize::from(a[i - 1] != b[j - 1])).min(above + 1).min(row[j - 1] + 1);
            diag = above;
        }
    }
    row[b.len()]
}

/// Mục gần `typed` nhất trong `choices`, nếu đủ gần. So sánh sau khi bỏ dấu, nên quên
/// dấu ("neu" thay cho "nếu") là gần nhất. `strict`: chỉ nhận khi khác nhau đúng ở dấu.
fn closest<'c>(typed: &str, choices: impl Iterator<Item = &'c String>, strict: bool) -> Option<&'c String> {
    let folded = ascii(&norm(typed));
    let size = folded.chars().count();
    // Từ càng ngắn càng dễ trùng ngẫu nhiên: từ ba chữ trở xuống chỉ nhận khi sai mỗi dấu.
    let limit = if strict || size <= 3 { 0 } else if size <= 6 { 1 } else { 2 };
    let words = typed.split_whitespace().count();
    choices
        .filter(|c| norm(c) != norm(typed) && c.split_whitespace().count() == words)
        .map(|c| (distance(&folded, &ascii(&norm(c))), c))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

/// "Có phải bạn muốn...": cụm hợp lệ gần nhất với một từ lạ ở chỗ lỗi cú pháp.
fn did_you_mean(lang: &Lang, text: &str, toks: &[Token], error_at: usize) -> Option<String> {
    let at = toks.iter().position(|t| t.span.start == error_at)?;
    let hint = |label: &String, typed: &str| format!("có phải bạn muốn «{label}» thay cho «{typed}»?");
    // Một từ khóa gõ sai sẽ bị đọc thành (một phần của) tên, và thường chỉ lộ ra ở
    // token ngay sau nó.
    for i in [Some(at), at.checked_sub(1)].into_iter().flatten() {
        let Tok::Ident(_) = toks[i].tok else { continue };
        let span = toks[i].span;
        let name = &text[span.start..span.end];
        let starts: Vec<usize> = name.split_whitespace().map(|w| w.as_ptr() as usize - name.as_ptr() as usize).collect();
        let word_end = |from: usize| from + name[from..].find(char::is_whitespace).unwrap_or(name.len() - from);

        // Cụm nhiều từ gõ sai ở từ sau ("mỗi luc"): từ đầu đã thành một từ khóa riêng,
        // nên ghép nó với từ đầu của tên rồi so với những gì hợp lệ trước từ khóa đó.
        if i > 0 && !matches!(toks[i - 1].tok, Tok::Ident(_) | Tok::Newline) && toks[i - 1].span.line == span.line {
            let before = toks[i - 1].span;
            if text[before.start..before.end].starts_with(char::is_alphabetic) {
                let typed = &text[before.start..span.start + word_end(0)];
                let candidates = candidates_at(lang, text, before.start);
                let labels = candidates.iter().filter(|c| c.snippet.is_none() && c.kind != KIND_VARIABLE).map(|c| &c.label);
                if let Some(label) = closest(typed, labels, false) {
                    return Some(hint(label, typed));
                }
            }
        }
        // Từng từ trong tên, so với từ vựng hợp lệ ngay tại vị trí của từ đó. Từ nằm sau
        // từ đầu thì chỉ nhận khi sai mỗi dấu, vì phần lớn chúng đúng là một phần của tên.
        for (k, &from) in starts.iter().enumerate() {
            let candidates = candidates_at(lang, text, span.start + from);
            let labels = || candidates.iter().filter(|c| c.snippet.is_none() && c.kind != KIND_VARIABLE).map(|c| &c.label);
            for &to in starts.iter().filter(|&&to| to >= from) {
                let typed = &name[from..word_end(to)];
                if let Some(label) = closest(typed, labels(), k > 0) {
                    return Some(hint(label, typed));
                }
            }
        }
    }
    None
}

fn diagnostics(lang: &Lang, doc: &Doc) -> Vec<Value> {
    let first_line = json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } });
    let report = |range: Value, message: &str| json!({ "range": range, "severity": 1, "source": "anature", "message": message });
    let range_of = |sp: crate::error::Span| {
        // Token cuối tệp có độ rộng 0; vẫn tô một ô để thấy được.
        let end = sp.end.max(sp.start + usize::from(sp.start < doc.text.len()));
        let end = (end..=doc.text.len()).find(|&i| doc.text.is_char_boundary(i)).unwrap_or(doc.text.len());
        json!({ "start": doc.position(sp.line - 1, sp.start), "end": doc.position(sp.line - 1, end) })
    };

    let toks = match lexer::lex(lang, &doc.text) {
        Ok(toks) => toks,
        Err(e) => return vec![report(e.span.map_or(first_line, range_of), &e.msg)],
    };
    match parser::parse(lang, &doc.text, &toks) {
        // Lỗi cú pháp: chỉ có một, kèm đề xuất nếu gõ gần đúng.
        Err(e) => {
            let hint = e.span.and_then(|sp| did_you_mean(lang, &doc.text, &toks, sp.start));
            let message = hint.map_or(e.msg.clone(), |hint| format!("{}\n{hint}", e.msg));
            vec![report(e.span.map_or(first_line, range_of), &message)]
        }
        // Cú pháp đúng: báo mọi lỗi về tên và kiểu.
        Ok(mut nodes) => crate::semantics::check(lang, &mut nodes)
            .into_iter()
            .map(|e| {
                // Tên chưa khai báo: đề xuất tên gần nhất trong số những tên đang có.
                let hint = e.unknown.as_ref().and_then(|(name, known)| {
                    closest(name, known.iter(), false).map(|k| format!("có phải bạn muốn «{k}»?"))
                });
                let message = hint.map_or(e.msg.clone(), |hint| format!("{}\n{hint}", e.msg));
                report(e.span.map_or(first_line.clone(), range_of), &message)
            })
            .collect(),
    }
}

/// Tô màu theo đúng từ vựng trong cấu hình. Từ đệm được tô như chú thích, vì
/// trình dịch bỏ qua chúng.
fn semantic_tokens(lang: &Lang, doc: &Doc) -> Vec<u32> {
    let toks = match lexer::lex(lang, &doc.text) {
        Ok(toks) => toks,
        // Đang gõ dở một chuỗi: vẫn giữ màu cho mọi dòng phía trên dòng đó.
        Err(e) => {
            let cut = e.span.map_or(0, |sp| doc.line_starts[sp.line - 1]);
            lexer::lex(lang, &doc.text[..cut]).unwrap_or_default()
        }
    };
    // Phần chữ của chuỗi nội suy do ngữ pháp tĩnh tô; ở đây chỉ tô các biểu thức trong {}.
    let toks = flatten(&toks);
    let mut data = Vec::new();
    let (mut last_line, mut last_col) = (0, 0);
    for (i, t) in toks.iter().enumerate() {
        let kind = match t.tok {
            Tok::Keyword(_) | Tok::Literal(_) => 0,
            Tok::Type(_) => 1,
            Tok::Op(_) => 2,
            // Từ đệm trước tên mà không có tên theo sau thì chính nó là một biến.
            Tok::Filler(ref groups)
                if lang.name_fillers.as_ref().is_some_and(|g| groups.contains(g))
                    && !toks.get(i + 1).is_some_and(|n| matches!(n.tok, Tok::Ident(_) | Tok::Filler(_))) =>
            {
                5
            }
            Tok::Filler(_) => 3,
            // Tên đi liền với ( là tên hàm.
            Tok::Ident(_) if toks.get(i + 1).is_some_and(|n| n.tok == Tok::Sym('(')) => 4,
            Tok::Ident(_) => 5,
            Tok::Num(_) => 6,
            Tok::Str(_) => 7,
            _ => continue,
        };
        let line = t.span.line - 1;
        let col = doc.text[doc.line_starts[line]..t.span.start].encode_utf16().count();
        let len = doc.text[t.span.start..t.span.end].encode_utf16().count();
        let delta_col = if line == last_line { col - last_col } else { col };
        data.extend([(line - last_line) as u32, delta_col as u32, len as u32, kind, 0]);
        (last_line, last_col) = (line, col);
    }
    data
}

/// "file:///a/b%20c.agn" -> /a/b c.agn
fn uri_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://")?.as_bytes();
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let hex = (raw[i] == b'%').then(|| raw.get(i + 1..i + 3)).flatten();
        match hex.and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()) {
            Some(b) => {
                bytes.push(b);
                i += 3;
            }
            None => {
                bytes.push(raw[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(bytes).ok().map(PathBuf::from)
}

fn path_uri(path: &std::path::Path) -> String {
    let mut out = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Một file cấu hình đang hỏng: đường dẫn, nội dung, và lỗi.
struct Broken {
    path: PathBuf,
    text: String,
    error: String,
}

/// Cấu hình cho từng tệp đang mở. File language.toml được đọc lại ở mỗi lần phân
/// tích, để sửa là thấy ngay; nhưng khi nó đang hỏng (thường là đang sửa dở), máy chủ
/// dùng tiếp bản hợp lệ gần nhất thay vì báo lỗi lên mọi tệp nguồn.
struct Configs {
    default: Lang,
    /// Đường dẫn -> (nội dung đã nạp, cấu hình dựng từ nội dung đó).
    good: HashMap<PathBuf, (String, Lang)>,
}

impl Configs {
    fn new() -> Configs {
        let default = crate::config::load(crate::DEFAULT_CONFIG).expect("cấu hình mặc định phải hợp lệ");
        Configs { default, good: HashMap::new() }
    }

    fn get(&mut self, uri: &str) -> (&Lang, Option<Broken>) {
        let Some(path) = uri_path(uri).and_then(|p| crate::config_path(&p)) else {
            return (&self.default, None);
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (&self.default, None);
        };
        let mut broken = None;
        if self.good.get(&path).is_none_or(|(loaded, _)| *loaded != text) {
            match crate::config::load(&text) {
                Ok(lang) => {
                    self.good.insert(path.clone(), (text, lang));
                }
                Err(e) => broken = Some(Broken { path: path.clone(), text, error: e.msg }),
            }
        }
        (self.good.get(&path).map_or(&self.default, |(_, lang)| lang), broken)
    }
}

/// Chẩn đoán đặt lên chính file cấu hình, ở dòng chứa cụm bị nhắc tới nếu tìm được.
fn config_diagnostic(broken: &Broken) -> Value {
    // Dòng mà trình đọc TOML chỉ ra ("at line 13"); nếu không có thì dòng khai báo cụm
    // bị nhắc tới, tìm theo dạng có dấu nháy để không dính vào chú thích.
    let stated = broken.error.split("at line ").nth(1).and_then(|rest| {
        rest.split(|c: char| !c.is_ascii_digit()).next()?.parse::<usize>().ok()?.checked_sub(1)
    });
    let quoted = broken.error.split('«').nth(1).and_then(|rest| rest.split('»').next());
    let declared = quoted.and_then(|q| {
        let exact = format!("\"{q}\"");
        broken.text.lines().enumerate().filter(|(_, l)| l.contains(&exact)).last().map(|(i, _)| i)
    });
    let line = stated.or(declared).unwrap_or(0);
    let width = broken.text.lines().nth(line).map_or(1, |l| l.encode_utf16().count().max(1));
    json!({
        "range": { "start": { "line": line, "character": 0 }, "end": { "line": line, "character": width } },
        "severity": 1,
        "source": "anature",
        "message": broken.error,
    })
}

fn read_message(input: &mut impl BufRead) -> Option<Value> {
    let mut length = 0;
    loop {
        let mut header = String::new();
        if input.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let header = header.trim();
        if header.is_empty() {
            break;
        }
        if let Some(n) = header.strip_prefix("Content-Length:") {
            length = n.trim().parse().ok()?;
        }
    }
    let mut body = vec![0; length];
    input.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn send(message: Value) {
    let body = message.to_string();
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "Content-Length: {}\r\n\r\n{body}", body.len());
    let _ = out.flush();
}

pub fn serve() {
    let mut input = std::io::stdin().lock();
    let mut docs: HashMap<String, Doc> = HashMap::new();
    let mut configs = Configs::new();
    // Các file cấu hình đang mang chẩn đoán lỗi, để xóa đi khi chúng được sửa xong.
    let mut flagged: Vec<PathBuf> = Vec::new();

    while let Some(msg) = read_message(&mut input) {
        let method = msg["method"].as_str().unwrap_or("");
        let params = &msg["params"];
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("").to_string();
        let publish = |uri: &str, diagnostics: Vec<Value>| {
            send(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": { "uri": uri, "diagnostics": diagnostics },
            }));
        };

        let result = match method {
            "initialize" => json!({
                "capabilities": {
                    "textDocumentSync": 1,
                    // Dấu cách cũng gọi gợi ý: sau "đặt biến a " là lúc cần biết viết gì tiếp.
                    "completionProvider": { "triggerCharacters": [" "] },
                    "semanticTokensProvider": {
                        "legend": { "tokenTypes": TOKEN_TYPES, "tokenModifiers": [] },
                        "full": true,
                    },
                },
                "serverInfo": { "name": "anature", "version": env!("CARGO_PKG_VERSION") },
            }),
            "textDocument/didOpen" | "textDocument/didChange" => {
                let text = match method {
                    "textDocument/didOpen" => params["textDocument"]["text"].as_str(),
                    _ => params["contentChanges"][0]["text"].as_str(),
                };
                let doc = Doc::new(text.unwrap_or(""));
                let (lang, broken) = configs.get(&uri);
                let mut found = diagnostics(lang, &doc);
                match &broken {
                    Some(b) => {
                        // Lỗi nằm ở file cấu hình, nên báo ở đó. Tệp nguồn chỉ nhận một lời
                        // nhắc (không phải lỗi) rằng nó đang được phân tích bằng cấu hình cũ.
                        publish(&path_uri(&b.path), vec![config_diagnostic(b)]);
                        if !flagged.contains(&b.path) {
                            flagged.push(b.path.clone());
                        }
                        found.push(json!({
                            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
                            "severity": 2,
                            "source": "anature",
                            "message": format!(
                                "{} đang có lỗi, nên tệp này được phân tích bằng bản cấu hình hợp lệ gần nhất.\n{}",
                                b.path.display(),
                                b.error
                            ),
                        }));
                    }
                    None => {
                        if let Some(path) = uri_path(&uri).and_then(|p| crate::config_path(&p)) {
                            if let Some(i) = flagged.iter().position(|p| *p == path) {
                                flagged.remove(i);
                                publish(&path_uri(&path), Vec::new());
                            }
                        }
                    }
                }
                publish(&uri, found);
                docs.insert(uri, doc);
                continue;
            }
            "textDocument/didClose" => {
                docs.remove(&uri);
                publish(&uri, Vec::new());
                continue;
            }
            "textDocument/completion" => match docs.get(&uri) {
                Some(doc) => {
                    let pos = &params["position"];
                    let (line, col) = (pos["line"].as_u64().unwrap_or(0), pos["character"].as_u64().unwrap_or(0));
                    json!(completion(configs.get(&uri).0, doc, line as usize, col as usize))
                }
                None => json!([]),
            },
            "textDocument/semanticTokens/full" => match docs.get(&uri) {
                Some(doc) => json!({ "data": semantic_tokens(configs.get(&uri).0, doc) }),
                None => json!({ "data": [] }),
            },
            "shutdown" => Value::Null,
            "exit" => return,
            _ if msg.get("id").is_some() => {
                send(json!({
                    "jsonrpc": "2.0",
                    "id": msg["id"],
                    "error": { "code": -32601, "message": format!("chưa hỗ trợ: {method}") },
                }));
                continue;
            }
            _ => continue,
        };
        if msg.get("id").is_some() {
            send(json!({ "jsonrpc": "2.0", "id": msg["id"], "result": result }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "Lớp A {\n    Hàm gốc {\n        đặt biến tổng là 5\n        ";

    fn lang() -> Lang {
        crate::config::load(crate::TEST_CONFIG).unwrap()
    }

    /// Gợi ý khi con trỏ đứng cuối `line`, là dòng thứ tư của một hàm gốc.
    fn labels(line: &str) -> Vec<String> {
        let doc = Doc::new(&format!("{HEAD}{line}"));
        let col = line.encode_utf16().count() + 8;
        completion(&lang(), &doc, 3, col).iter().map(|i| i["label"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn goi_y_dung_tap_hop_le_tai_con_tro() {
        assert_eq!(labels("đặt biến a "), ["bằng", "có giá trị", "là", "tương đương"]);
        assert_eq!(labels("đặt biến a l"), ["là"]);
        let start = labels("");
        assert!(start.contains(&"nếu".to_string()) && start.contains(&"tổng".to_string()), "{start:?}");
        assert!(!start.contains(&"là".to_string()) && start.last().unwrap() == "}", "{start:?}");
        let value = labels("đặt biến a là ");
        assert!(value.contains(&"tổng".to_string()) && value.contains(&"nhập".to_string()), "{value:?}");
        assert!(!value.contains(&"cộng".to_string()), "{value:?}");
        assert!(labels("đặt biến a là tổng ").contains(&"cộng".to_string()));
    }

    #[test]
    fn goi_y_cum_nhieu_tu_va_giu_chu_hoa() {
        assert_eq!(labels("Mỗi l"), ["Mỗi lúc", "Mỗi lúc ‹điều kiện› { }"]);
        let doc = Doc::new(&format!("{HEAD}Mỗi l"));
        let item = &completion(&lang(), &doc, 3, 13)[0];
        // Cả "Mỗi l" được thay, không chỉ chữ "l".
        assert_eq!(item["textEdit"]["range"]["start"]["character"], 8);
        assert_eq!(item["textEdit"]["range"]["end"]["character"], 13);
    }

    #[test]
    fn khong_goi_y_khi_phan_truoc_da_sai_hoac_trong_chuoi() {
        assert!(labels("đặt đặt đặt\n ").is_empty());
        assert!(labels("in(\"xin ").is_empty());
    }

    #[test]
    fn goi_y_khung_cau_lenh_tu_luat() {
        let doc = Doc::new(&format!("{HEAD}lặp"));
        let items = completion(&lang(), &doc, 3, 11);
        let frame = items.iter().find(|i| i["label"] == "lặp ‹biến› từ ‹đầu› đến ‹cuối› { }").expect("phải có khung");
        assert_eq!(frame["insertTextFormat"], 2);
        assert_eq!(frame["textEdit"]["newText"], "lặp ${1:biến} từ ${2:đầu} đến ${3:cuối} {\n\t$0\n}");
        // Từ khóa đơn đứng trước các khung câu.
        assert_eq!(items[0]["label"], "lặp");
        // Hai khối: con trỏ dừng cuối cùng ở khối sau chót.
        let doc = Doc::new(&format!("{HEAD}nếu"));
        let items = completion(&lang(), &doc, 3, 11);
        let both = items.iter().find(|i| i["label"] == "nếu ‹điều kiện› { } không thì { }").expect("phải có khung");
        assert_eq!(both["textEdit"]["newText"], "nếu ${1:điều kiện} {\n\t$2\n} không thì {\n\t$0\n}");
        // Khung câu chỉ xuất hiện ở đầu câu lệnh.
        assert!(labels("đặt biến a là ").iter().all(|l| !l.contains('‹')));
        // Trong lớp thì gợi ý khung của hàm.
        let doc = Doc::new("Lớp A {\n    hàm");
        let items = completion(&lang(), &doc, 1, 7);
        assert!(items.iter().any(|i| i["label"] == "hàm ‹tên› (‹tham số›) trả về ‹kiểu› { }"), "{items:?}");
    }

    #[test]
    fn goi_y_ten_nhieu_tu() {
        let doc = Doc::new(&format!("{HEAD}đặt biến tổng điểm là 1\n        in(tổng đ"));
        let items = completion(&lang(), &doc, 4, 17);
        let item = items.iter().find(|i| i["label"] == "tổng điểm").expect("phải gợi ý tên nhiều từ");
        assert_eq!(item["textEdit"]["range"]["start"]["character"], 11);
    }

    #[test]
    fn bao_loi_ngu_nghia_khi_go() {
        let doc = Doc::new(&format!("{HEAD}in(tonng)\n        in(tổng(1))\n    }}\n}}\n"));
        let found = diagnostics(&lang(), &doc);
        let messages: Vec<&str> = found.iter().map(|d| d["message"].as_str().unwrap()).collect();
        assert_eq!(messages, ["biến «tonng» chưa được khai báo\ncó phải bạn muốn «tổng»?", "hàm «tổng» chưa được định nghĩa"]);
        assert_eq!(found[0]["range"]["start"], json!({ "line": 3, "character": 11 }));
        assert_eq!(found[0]["range"]["end"], json!({ "line": 3, "character": 16 }));
    }

    fn message(line: &str) -> String {
        let doc = Doc::new(&format!("{HEAD}{line}\n    }}\n}}\n"));
        let found = diagnostics(&lang(), &doc);
        found.first().map(|d| d["message"].as_str().unwrap().to_string()).unwrap_or_default()
    }

    #[test]
    fn co_phai_ban_muon() {
        assert!(message("neu tổng lớn hơn 3 { }").contains("có phải bạn muốn «nếu» thay cho «neu»?"));
        assert!(message("lăp i từ 1 đến 3 { }").contains("«lặp» thay cho «lăp»"));
        assert!(message("đặt biến a la 5").contains("«là» thay cho «la»"));
        assert!(message("mỗi luc tổng lớn hơn 1 { }").contains("«mỗi lúc» thay cho «mỗi luc»"));
        assert!(message("nếu tổng lơn hơn 1 { }").contains("«lớn hơn» thay cho «lơn hơn»"));
        // Tên có chứa một cụm của ngôn ngữ: giải thích, và không đoán bừa một từ gần giống.
        let cut = message("đặt đã đoán đúng là sai");
        assert!(cut.contains("nếu «đã đoán đúng» là một tên thì hãy đổi tên") && !cut.contains("có phải"), "{cut}");
        // Không đủ gần thì không đoán.
        assert!(!message("x y").contains("có phải"));
        assert_eq!(message("in(tổng)"), "");
    }

    #[test]
    fn to_mau_theo_tu_vung() {
        /// Các cặp (chữ được tô, loại) trên dòng đầu.
        fn colors(text: &str) -> Vec<(String, &'static str)> {
            let line: Vec<u16> = text.lines().next().unwrap_or("").encode_utf16().collect();
            let (mut col, mut out) = (0, Vec::new());
            for t in semantic_tokens(&lang(), &Doc::new(text)).chunks(5).take_while(|t| t[0] == 0) {
                col += t[1] as usize;
                out.push((String::from_utf16_lossy(&line[col..col + t[2] as usize]), TOKEN_TYPES[t[3] as usize]));
            }
            out
        }
        let got = colors("Mỗi lúc giá trị a lớn hơn f(2) thì { in(\"x\") }");
        let want = [
            ("Mỗi lúc", "keyword"), ("giá trị", "comment"), ("a", "variable"), ("lớn hơn", "operator"),
            ("f", "function"), ("2", "number"), ("thì", "comment"), ("in", "keyword"), ("\"x\"", "string"),
        ];
        assert_eq!(got, want.map(|(text, kind)| (text.to_string(), kind)));
        // Chuỗi chưa đóng ở dòng dưới không làm mất màu dòng trên.
        assert_eq!(colors("đặt số nguyên a là 1\nin(\"dở").len(), 5);
    }
}
