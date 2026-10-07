//! Áp các luật trong cấu hình lên dãy token, theo kiểu PEG: thử lần lượt,
//! luật khớp trước thắng. Kết quả là cây trung gian chỉ chứa token nội bộ.

use crate::config::{Elem, Lang, Scope, SlotKind, norm};
use crate::error::{Error, Span};
use crate::lexer::{Piece, Tok, Token};

#[derive(Debug, Clone)]
pub enum Expr {
    Num(String),
    Str(String),
    /// Chuỗi nội suy: các mảnh chữ và biểu thức xen kẽ.
    Interp(Vec<Part>),
    Lit(String),
    Var(String, Span),
    Call(String, Span, Vec<Expr>),
    List(Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    /// Một luật scope biểu_thức trong cấu hình (ví dụ lệnh nhập).
    Rule(Box<Node>),
    Prefix(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Part {
    Text(String),
    Code(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Simple(String),
    List(Box<Type>),
}

#[derive(Debug, Clone)]
pub enum Value {
    Name(String, Span),
    /// Biểu thức và khoảng mã nguồn mà nó chiếm.
    Expr(Expr, Span),
    Type(Type),
    /// Các cặp (kiểu, tên).
    Params(Vec<(Type, String)>),
    Block(Vec<Node>),
    Stmt(Box<Node>),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub rule: usize,
    pub span: Span,
    pub slots: Vec<(String, Value)>,
}

/// Một thứ hợp lệ tại một vị trí. Dùng cho cả thông báo lỗi lẫn gợi ý khi gõ.
#[derive(Debug, Clone, PartialEq)]
pub enum Expect {
    /// Một cụm bất kỳ của nhóm từ khóa, kiểu hoặc đệm.
    Group(String),
    Sym(char),
    Name,
    Expr,
    Operator,
    Type,
    Newline,
    /// Một câu lệnh của phạm vi này có thể bắt đầu ở đây. Không hiện trong thông báo
    /// lỗi; bộ gợi ý dùng nó để đề xuất cả khung câu lệnh.
    Start(Scope),
}

struct Parser<'a> {
    lang: &'a Lang,
    src: &'a str,
    toks: &'a [Token],
    pos: usize,
    /// Vị trí xa nhất từng thất bại và những gì hợp lệ tại đó: nguồn của thông báo lỗi.
    far: usize,
    expected: Vec<Expect>,
    /// Đang ở ngay trong danh sách đối số: cụm ngăn cách ("và") đóng vai dấu phẩy.
    in_list: bool,
    /// Lỗi bên trong {} của một chuỗi nội suy: có vị trí riêng, được ưu tiên báo.
    fatal: Option<Error>,
    /// Tên gọi của token kết thúc trong thông báo lỗi.
    end_name: &'static str,
    /// Đang đọc vị trí sau từ "thứ": không đọc tiếp một "thứ" nữa vào trong nó, để
    /// "bảng thứ 0 thứ 1" là (bảng thứ 0) thứ 1.
    in_position: bool,
    /// Kết quả phân tích một khối tại một vị trí, kèm vị trí sau khối. Nhiều luật cùng
    /// mở đầu giống nhau (ba dạng của "nếu") sẽ không phân tích lại thân của mình.
    blocks: std::collections::HashMap<(usize, Scope), Option<(Vec<Node>, usize)>>,
}

fn parser<'a>(lang: &'a Lang, src: &'a str, toks: &'a [Token]) -> Parser<'a> {
    Parser { lang, src, toks, pos: 0, far: 0, expected: Vec::new(), in_list: false, fatal: None, end_name: "hết tệp", in_position: false, blocks: Default::default() }
}

pub fn parse(lang: &Lang, src: &str, toks: &[Token]) -> Result<Vec<Node>, Error> {
    let mut p = parser(lang, src, toks);
    p.file().ok_or_else(|| p.fatal.take().unwrap_or_else(|| p.error()))
}

/// Những gì hợp lệ tại token `Tok::Cursor`. Rỗng nếu phần trước con trỏ đã sai cú pháp.
pub fn expected_at_cursor(lang: &Lang, src: &str, toks: &[Token]) -> Vec<Expect> {
    let mut p = parser(lang, src, toks);
    let _ = p.file();
    if toks[p.far].tok == Tok::Cursor { p.expected } else { Vec::new() }
}

impl<'a> Parser<'a> {
    fn file(&mut self) -> Option<Vec<Node>> {
        let mut nodes = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::Eof {
            nodes.push(self.item(Scope::File)?);
            self.skip_newlines();
        }
        Some(nodes)
    }

    fn peek(&self) -> &'a Tok {
        &self.toks[self.pos].tok
    }

    fn skip_newlines(&mut self) {
        while *self.peek() == Tok::Newline {
            self.pos += 1;
        }
    }

    /// Bỏ qua các lần xuống dòng nếu ngay sau chúng là một token thỏa `wanted`.
    fn skip_newlines_before(&mut self, wanted: impl Fn(&Tok) -> bool) {
        let mut at = self.pos;
        while self.toks[at].tok == Tok::Newline {
            at += 1;
        }
        if at > self.pos && wanted(&self.toks[at].tok) {
            self.pos = at;
        }
    }

    /// Hai token có đứng liền nhau trên cùng một dòng không (giữa chúng chỉ có dấu cách).
    fn adjacent(&self, left: usize, right: usize) -> bool {
        let (a, b) = (&self.toks[left].span, &self.toks[right].span);
        a.line == b.line && a.end <= b.start && self.src[a.end..b.start].chars().all(|c| c == ' ' || c == '\t')
    }

    /// Giải thích thêm khi một cái tên bị một cụm của ngôn ngữ, hoặc một con số, cắt ngang.
    fn name_hint(&self) -> Option<String> {
        let at = self.far;
        let t = &self.toks[at];
        let text = |i: usize| &self.src[self.toks[i].span.start..self.toks[i].span.end];
        let is_name = |i: usize| matches!(self.toks.get(i).map(|t| &t.tok), Some(Tok::Ident(_)));
        let before = at > 0 && is_name(at - 1) && self.adjacent(at - 1, at);
        let after = is_name(at + 1) && self.adjacent(at, at + 1);
        if t.tok != Tok::Eof && matches!(t.tok, Tok::Num(_)) && before && !self.expected.contains(&Expect::Operator) {
            let joined = format!("{}_{}", text(at - 1).replace(' ', "_"), text(at));
            return Some(format!("tên không được chứa số đứng riêng; nếu «{} {}» là một tên thì hãy viết «{joined}»", text(at - 1), text(at)));
        }
        let kind = match &t.tok {
            Tok::Keyword(g) => format!("từ khóa ({g})"),
            Tok::Type(g) => format!("kiểu dữ liệu ({g})"),
            Tok::Op(g) => format!("toán tử ({g})"),
            Tok::Literal(g) => format!("hằng ({g})"),
            Tok::Filler(_) => "từ đệm".to_string(),
            _ => return None,
        };
        if !text(at).starts_with(char::is_alphabetic) {
            return None;
        }
        let wants_name = self.expected.contains(&Expect::Name) && !self.expected.iter().any(|e| matches!(e, Expect::Start(_)));
        // Tên bị cắt ngang ("đã đoán đúng"), hoặc một tên mở đầu bằng cụm của ngôn ngữ
        // ở chỗ đang chờ một giá trị ("số nguyên tố").
        let interrupted = before || (after && self.expected.contains(&Expect::Expr));
        if wants_name && !interrupted {
            return Some(format!("«{}» là {kind} của ngôn ngữ nên không dùng làm tên được", text(at)));
        }
        if !interrupted {
            return None;
        }
        let mut whole = String::new();
        if before {
            whole.push_str(text(at - 1));
            whole.push(' ');
        }
        whole.push_str(text(at));
        if after {
            whole.push(' ');
            whole.push_str(text(at + 1));
        }
        Some(format!("«{}» là {kind} của ngôn ngữ; nếu «{whole}» là một tên thì hãy đổi tên, vì tên không được chứa cụm này", text(at)))
    }

    fn note(&mut self, what: Expect) {
        if self.pos > self.far {
            self.far = self.pos;
            self.expected.clear();
        }
        if self.pos == self.far && !self.expected.contains(&what) {
            self.expected.push(what);
        }
    }

    fn error(&self) -> Error {
        let t = &self.toks[self.far];
        let got = match t.tok {
            Tok::Newline => "xuống dòng".to_string(),
            Tok::Eof => self.end_name.to_string(),
            _ => format!("«{}»", &self.src[t.span.start..t.span.end]),
        };
        let mut names: Vec<String> = Vec::new();
        for e in &self.expected {
            let texts = match e {
                Expect::Group(g) => self.lang.words[g].iter().map(|w| format!("«{w}»")).collect(),
                Expect::Sym(c) => vec![format!("«{c}»")],
                Expect::Name => vec!["tên".to_string()],
                Expect::Expr => vec!["biểu thức".to_string()],
                Expect::Operator => vec!["toán tử".to_string()],
                Expect::Type => vec!["kiểu dữ liệu".to_string()],
                Expect::Newline => vec!["xuống dòng".to_string()],
                Expect::Start(_) => Vec::new(),
            };
            for t in texts {
                if !names.contains(&t) {
                    names.push(t);
                }
            }
        }
        let mut msg = match names.as_slice() {
            [one] => format!("ở đây cần {one}, nhưng gặp {got}"),
            many => format!("ở đây cần một trong: {} — nhưng gặp {got}", many.join(", ")),
        };
        if let Some(hint) = self.name_hint() {
            msg = format!("{msg}\n{hint}");
        }
        Error::at(t.span, msg)
    }

    /// Khớp một token thuộc nhóm từ khóa hoặc nhóm đệm `group`.
    fn eat(&mut self, group: &str) -> bool {
        let hit = match self.peek() {
            Tok::Keyword(g) | Tok::Type(g) => g == group,
            Tok::Filler(groups) => groups.iter().any(|g| g == group),
            _ => false,
        };
        if hit {
            self.pos += 1;
        } else {
            self.note(Expect::Group(group.to_string()));
        }
        hit
    }

    fn eat_sym(&mut self, c: char) -> bool {
        let hit = *self.peek() == Tok::Sym(c);
        if hit {
            self.pos += 1;
        } else {
            self.note(Expect::Sym(c));
        }
        hit
    }

    /// Một câu lệnh thuộc `scope`: luật đầu tiên khớp trọn đến hết câu sẽ được chọn.
    fn item(&mut self, scope: Scope) -> Option<Node> {
        let start = self.pos;
        self.note(Expect::Start(scope));
        for idx in 0..self.lang.rules.len() {
            if self.lang.rules[idx].scope != scope {
                continue;
            }
            self.pos = start;
            if let Some(node) = self.rule(idx) {
                if matches!(self.peek(), Tok::Newline | Tok::Eof | Tok::Sym('}')) {
                    return Some(node);
                }
                self.note(Expect::Newline);
            }
        }
        self.pos = start;
        None
    }

    fn rule(&mut self, idx: usize) -> Option<Node> {
        let lang = self.lang;
        let span = self.toks[self.pos].span;
        let mut slots = Vec::new();
        for elem in &lang.rules[idx].elems {
            match elem {
                Elem::Keyword(g) => {
                    // Từ khóa giữa luật mà không mở đầu câu lệnh nào thì được nằm ở dòng mới:
                    //   }
                    //   không thì {
                    if !slots.is_empty() && !lang.starters.contains(g) {
                        self.skip_newlines_before(|t| matches!(t, Tok::Keyword(k) if k == g));
                    }
                    if !self.eat(g) {
                        return None;
                    }
                }
                Elem::Opt(g) => {
                    self.eat(g);
                }
                Elem::Many(g) => while self.eat(g) {},
                Elem::Sym(c) => {
                    if !self.eat_sym(*c) {
                        return None;
                    }
                }
                Elem::Slot { name, kind } => {
                    let first = self.toks[self.pos].span;
                    // Khoảng mã nguồn từ token đầu đến token cuối mà chỗ trống đã dùng.
                    let covered = |p: &Self| Span { end: p.toks[p.pos.max(1) - 1].span.end.max(first.end), ..first };
                    let value = match kind {
                        SlotKind::Name => Value::Name(self.name()?, first),
                        SlotKind::Expr => {
                            let e = self.expr(0)?;
                            Value::Expr(e, covered(self))
                        }
                        SlotKind::Operand => {
                            let e = self.operand()?;
                            Value::Expr(e, covered(self))
                        }
                        SlotKind::Type => Value::Type(self.ty()?),
                        SlotKind::Params => Value::Params(self.params()?),
                        SlotKind::Call => {
                            let e = self.call()?;
                            Value::Expr(e, covered(self))
                        }
                        SlotKind::Block(scope) => Value::Block(self.block(*scope)?),
                        SlotKind::Stmt => Value::Stmt(Box::new(self.item(Scope::Body)?)),
                    };
                    slots.push((name.clone(), value));
                }
            }
        }
        Some(Node { rule: idx, span, slots })
    }

    /// Bỏ qua các từ đệm đứng trước tên ("biến", "tên", "cho biến"). Trả về từ đệm cuối
    /// cùng đã bỏ qua, dưới dạng một cái tên, kèm vị trí của nó.
    fn skip_name_fillers(&mut self) -> Option<(String, Span)> {
        let lang = self.lang;
        let group = lang.name_fillers.as_ref()?;
        let filler = |t: &Tok| matches!(t, Tok::Filler(groups) if groups.contains(group));
        let mut last = None;
        loop {
            let span = self.toks[self.pos].span;
            let text = norm(&self.src[span.start..span.end]);
            if filler(self.peek()) {
                last = Some((text, span));
            } else if matches!(self.peek(), Tok::Keyword(_)) && lang.keyword_name_fillers.contains(&text) {
                // Một từ khóa chỉ là đệm khi ngay sau nó là một cái tên (hoặc đệm khác);
                // nó không bao giờ tự mình thành tên.
                let next = &self.toks[self.pos + 1].tok;
                if !matches!(next, Tok::Ident(_)) && !filler(next) {
                    break;
                }
                last = None;
            } else {
                break;
            }
            self.pos += 1;
        }
        last
    }

    /// Một cái tên, có thể có từ đệm đứng trước. Từ đệm chỉ là đệm khi theo sau nó là
    /// một tên; đứng một mình thì chính nó là tên: "nhập chữ tên" tạo biến «tên», còn
    /// "nhập chữ tên học sinh" tạo biến «học sinh».
    fn name(&mut self) -> Option<String> {
        let filler = self.skip_name_fillers();
        if let Tok::Ident(name) = self.peek() {
            self.pos += 1;
            Some(name.clone())
        } else if let Some((name, _)) = filler {
            Some(name)
        } else {
            self.note(Expect::Name);
            None
        }
    }

    fn ty(&mut self) -> Option<Type> {
        match self.peek() {
            Tok::Type(t) => {
                self.pos += 1;
                Some(Type::Simple(t.clone()))
            }
            Tok::Keyword(k) if self.lang.list_keyword.as_ref() == Some(k) => {
                self.pos += 1;
                Some(Type::List(Box::new(self.ty()?)))
            }
            _ => {
                self.note(Expect::Type);
                None
            }
        }
    }

    fn is_sep(&self) -> bool {
        let t = &self.toks[self.pos];
        match t.tok {
            Tok::Sym(',') => true,
            Tok::Op(_) | Tok::Filler(_) => self.lang.separators.contains(&norm(&self.src[t.span.start..t.span.end])),
            _ => false,
        }
    }

    fn eat_sep(&mut self) -> bool {
        let hit = self.is_sep();
        if hit {
            self.pos += 1;
        } else {
            self.note(Expect::Sym(','));
        }
        hit
    }

    /// Vị trí của dấu ( mở danh sách tham số hay đối số, tính từ `at`, sau khi bỏ qua
    /// các từ được phép chen giữa tên hàm và nó ("từ", "với tham số").
    fn arguments_open(&self, mut at: usize) -> Option<usize> {
        let allowed = &self.lang.before_arguments;
        loop {
            match &self.toks[at].tok {
                Tok::Sym('(') => return Some(at),
                Tok::Keyword(g) if allowed.contains(g) => at += 1,
                Tok::Filler(groups) if groups.iter().any(|g| allowed.contains(g)) => at += 1,
                _ => return None,
            }
        }
    }

    /// Danh sách tham số khi khai báo hàm; không có ngoặc nghĩa là không tham số.
    fn params(&mut self) -> Option<Vec<(Type, String)>> {
        let mut params = Vec::new();
        let Some(open) = self.arguments_open(self.pos) else {
            self.note(Expect::Sym('('));
            return Some(params);
        };
        self.pos = open;
        self.pos += 1;
        if *self.peek() == Tok::Sym(')') {
            self.pos += 1;
            return Some(params);
        }
        loop {
            let ty = self.ty()?;
            params.push((ty, self.name()?));
            self.skip_newlines();
            if !self.eat_sep() {
                return self.eat_sym(')').then_some(params);
            }
        }
    }

    fn call(&mut self) -> Option<Expr> {
        let span = self.toks[self.pos].span;
        let name = self.name()?;
        if let Some(open) = self.arguments_open(self.pos) {
            self.pos = open;
        }
        if !self.eat_sym('(') {
            return None;
        }
        Some(Expr::Call(name, span, self.scoped(true, |p| p.args(')'))?))
    }

    /// Các biểu thức cách nhau bằng dấu phẩy (hoặc cụm ngăn cách), đến ký hiệu đóng `close`.
    fn args(&mut self, close: char) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        if *self.peek() == Tok::Sym(close) {
            self.pos += 1;
            return Some(args);
        }
        loop {
            args.push(self.expr(0)?);
            // Trong ngoặc, xuống dòng không kết thúc gì cả; dấu phẩy cuối cũng được.
            self.skip_newlines();
            if !self.eat_sep() {
                return self.eat_sym(close).then_some(args);
            }
            self.skip_newlines();
            if *self.peek() == Tok::Sym(close) {
                self.pos += 1;
                return Some(args);
            }
        }
    }

    /// Chạy `f` với `in_list` tạm đổi, và luôn khôi phục dù khớp hay không.
    fn scoped<T>(&mut self, in_list: bool, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let outer = std::mem::replace(&mut self.in_list, in_list);
        let out = f(self);
        self.in_list = outer;
        out
    }

    fn block(&mut self, scope: Scope) -> Option<Vec<Node>> {
        // Dấu { được phép nằm ở dòng mới; không câu lệnh nào mở đầu bằng nó.
        self.skip_newlines_before(|t| *t == Tok::Sym('{'));
        let key = (self.pos, scope);
        if let Some(known) = self.blocks.get(&key) {
            let (nodes, end) = known.clone()?;
            self.pos = end;
            return Some(nodes);
        }
        let found = self.block_body(scope);
        self.blocks.insert(key, found.clone().map(|nodes| (nodes, self.pos)));
        found
    }

    fn block_body(&mut self, scope: Scope) -> Option<Vec<Node>> {
        if !self.eat_sym('{') {
            return None;
        }
        let mut nodes = Vec::new();
        loop {
            self.skip_newlines();
            if *self.peek() == Tok::Sym('}') {
                self.pos += 1;
                return Some(nodes);
            }
            self.note(Expect::Sym('}'));
            nodes.push(self.item(scope)?);
        }
    }

    fn expr(&mut self, min_prec: u8) -> Option<Expr> {
        let lang = self.lang;
        let mut left = self.operand()?;
        loop {
            if self.in_list && self.is_sep() {
                return Some(left);
            }
            let name = match self.peek() {
                Tok::Op(name) => name,
                // "nếu a bằng b": một từ khóa được cấu hình cho đọc như toán tử khi ở giữa biểu thức.
                Tok::Keyword(g) if lang.keyword_ops.contains_key(g) => &lang.keyword_ops[g],
                _ => {
                    self.note(Expect::Operator);
                    return Some(left);
                }
            };
            let op = &lang.ops[name];
            if !op.infix || op.prec < min_prec {
                return Some(left);
            }
            self.pos += 1;
            let right = self.expr(op.prec + 1)?;
            left = Expr::Binary(name.clone(), Box::new(left), Box::new(right));
        }
    }

    fn operand(&mut self) -> Option<Expr> {
        let lang = self.lang;
        // Đệm trước toán hạng ("giá trị của") và đệm trước tên ("biến") có thể đi liền nhau.
        let mut name_filler = None;
        loop {
            if let Some(found) = self.skip_name_fillers() {
                name_filler = Some(found);
                continue;
            }
            let Some(group) = &lang.operand_fillers else { break };
            if !matches!(self.peek(), Tok::Filler(groups) if groups.contains(group)) {
                break;
            }
            name_filler = None;
            self.pos += 1;
        }
        // Từ đệm trước tên mà không có tên nào theo sau: chính nó là biến đang được dùng.
        if let (Some((name, span)), false) = (name_filler, matches!(self.peek(), Tok::Ident(_))) {
            return self.indexed(Expr::Var(name, span));
        }
        if let Tok::Op(name) = self.peek() {
            if lang.ops[name].prefix {
                self.pos += 1;
                let inner = match lang.ops[name].prefix_prec {
                    Some(prec) => self.expr(prec)?,
                    None => self.operand()?,
                };
                return Some(Expr::Prefix(name.clone(), Box::new(inner)));
            }
        }
        let expr = self.primary()?;
        self.indexed(expr)
    }

    /// Các lần lấy phần tử theo vị trí nối sau một toán hạng: a[0][1].
    fn indexed(&mut self, mut expr: Expr) -> Option<Expr> {
        loop {
            let by_word = matches!(self.peek(), Tok::Keyword(g) if self.lang.index_keyword.as_ref() == Some(g));
            if *self.peek() == Tok::Sym('[') {
                self.pos += 1;
                let index = self.scoped(false, |p| p.expr(0))?;
                self.skip_newlines();
                if !self.eat_sym(']') {
                    return None;
                }
                expr = Expr::Index(Box::new(expr), Box::new(index));
            } else if by_word && !self.in_position {
                // "ds thứ i": vị trí là một toán hạng đơn; phép tính thì viết trong ngoặc.
                self.pos += 1;
                let outer = std::mem::replace(&mut self.in_position, true);
                let index = self.operand();
                self.in_position = outer;
                expr = Expr::Index(Box::new(expr), Box::new(index?));
            } else {
                return Some(expr);
            }
        }
    }

    fn primary(&mut self) -> Option<Expr> {
        let expr = match self.peek() {
            Tok::Num(n) => Expr::Num(n.clone()),
            Tok::Str(s) => Expr::Str(s.clone()),
            Tok::Interp(pieces) => {
                let mut parts = Vec::new();
                for piece in pieces {
                    parts.push(match piece {
                        Piece::Text(text) => Part::Text(text.clone()),
                        Piece::Code(toks) => Part::Code(self.embedded(toks)?),
                    });
                }
                Expr::Interp(parts)
            }
            Tok::Literal(l) => Expr::Lit(l.clone()),
            Tok::Ident(_) if self.arguments_open(self.pos + 1).is_some() => return self.call(),
            Tok::Ident(v) => Expr::Var(v.clone(), self.toks[self.pos].span),
            Tok::Sym('(') => {
                self.pos += 1;
                // Trong ngoặc đơn, "và" trở lại là toán tử.
                let inner = self.scoped(false, |p| p.expr(0))?;
                self.skip_newlines();
                return self.eat_sym(')').then_some(inner);
            }
            Tok::Sym('[') => {
                self.pos += 1;
                return Some(Expr::List(self.scoped(true, |p| p.args(']'))?));
            }
            Tok::Keyword(_) => return self.expr_rule(),
            _ => {
                self.note(Expect::Expr);
                return None;
            }
        };
        self.pos += 1;
        Some(expr)
    }

    /// Biểu thức trong {} của một chuỗi nội suy, phân tích bằng một parser con trên
    /// các token riêng của nó.
    fn embedded(&mut self, toks: &'a [Token]) -> Option<Expr> {
        let mut sub = parser(self.lang, self.src, toks);
        sub.end_name = "dấu } của chuỗi";
        let found = sub.expr(0).filter(|_| *sub.peek() == Tok::Eof);
        if found.is_none() {
            self.fatal = Some(sub.fatal.take().unwrap_or_else(|| sub.error()));
        }
        found
    }

    fn expr_rule(&mut self) -> Option<Expr> {
        let start = self.pos;
        for idx in 0..self.lang.rules.len() {
            if self.lang.rules[idx].scope != Scope::Expr {
                continue;
            }
            self.pos = start;
            if let Some(node) = self.rule(idx) {
                return Some(Expr::Rule(Box::new(node)));
            }
        }
        self.pos = start;
        self.note(Expect::Expr);
        None
    }
}
