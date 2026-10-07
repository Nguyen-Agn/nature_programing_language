//! Kiểm tra ngữ nghĩa trên cây trung gian: tên đã khai báo chưa, hàm gọi đúng chưa,
//! kiểu có khớp không. Lõi không biết luật nào làm gì; nó đọc phần ngữ nghĩa mà
//! cấu hình gắn cho từng luật (declare, function, assign, ...).
//!
//! Kiểu không suy ra được thì coi là "chưa biết" và không bao giờ sinh lỗi: thà bỏ
//! sót để javac bắt còn hơn báo sai.

use std::collections::HashMap;

use crate::config::{Elem, Lang, Scope, Semantics};
use crate::error::{Error, Span};
use crate::parser::{Expr, Node, Part, Type, Value};

struct Var {
    ty: Option<Type>,
    line: usize,
    /// Giá trị có thể đang nằm trong "hộp" của mã đích (phần tử lấy từ danh sách).
    /// Hai giá trị cùng nằm trong hộp cần phép so sánh riêng.
    boxed: bool,
}

struct Func {
    params: Vec<Type>,
    /// None: hàm không trả về giá trị.
    returns: Option<Type>,
}

/// Kiểu của một biểu thức, và nó có thể đang nằm trong hộp hay không.
type Typed = (Option<Type>, bool);

struct Checker<'a> {
    lang: &'a Lang,
    sem: &'a Semantics,
    funcs: HashMap<String, Func>,
    /// Biến khai báo ở cấp lớp: mọi hàm đều thấy.
    globals: HashMap<String, Var>,
    /// Các phạm vi biến lồng nhau của hàm đang xét, trong cùng nằm cuối.
    scopes: Vec<HashMap<String, Var>>,
    /// Kiểu trả về của hàm đang xét; None ngoài: không ở trong hàm nào.
    returns: Option<Option<Type>>,
    /// Khoảng mã nguồn của biểu thức đang xét, cho những lỗi không có vị trí riêng.
    here: Span,
    errors: Vec<Error>,
}

/// Trả về mọi lỗi tìm được. Đồng thời đổi tên toán tử trong cây sang biến thể theo
/// kiểu khi cấu hình có (EQ trên chữ thành "EQ.STRING"), để bộ sinh mã chọn đúng mẫu.
pub fn check(lang: &Lang, nodes: &mut [Node]) -> Vec<Error> {
    let Some(sem) = &lang.semantics else {
        return Vec::new();
    };
    let here = Span { start: 0, end: 0, line: 1, col: 1 };
    let mut c = Checker {
        lang,
        sem,
        funcs: HashMap::new(),
        globals: HashMap::new(),
        scopes: Vec::new(),
        returns: None,
        here,
        errors: Vec::new(),
    };
    c.block(nodes);
    if let (Some(entry), Some(first)) = (&sem.entry_rule, nodes.first()) {
        if !contains_rule(lang, nodes, entry) {
            let msg = format!("chương trình chưa có {}, nơi bắt đầu chạy", describe(lang, entry));
            c.errors.push(Error::at(first.span, msg));
        }
    }
    c.errors
}

fn contains_rule(lang: &Lang, nodes: &[Node], rule: &str) -> bool {
    nodes.iter().any(|n| {
        lang.rules[n.rule].name == rule
            || n.slots.iter().any(|(_, v)| matches!(v, Value::Block(inner) if contains_rule(lang, inner, rule)))
    })
}

/// Cách viết của một luật, ghép từ cách viết đầu tiên của từng từ khóa: «hàm gốc».
fn describe(lang: &Lang, rule: &str) -> String {
    let words: Vec<&str> = lang
        .rules
        .iter()
        .filter(|r| r.name == rule)
        .flat_map(|r| &r.elems)
        .filter_map(|e| match e {
            Elem::Keyword(g) => lang.words[g].first().map(String::as_str),
            _ => None,
        })
        .collect();
    format!("«{}»", words.join(" "))
}

fn slot<'n>(n: &'n Node, name: &str) -> Option<&'n Value> {
    n.slots.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

impl Checker<'_> {
    fn simple(&self, group: &str) -> Option<Type> {
        Some(Type::Simple(group.to_string()))
    }

    /// Tên kiểu như người dùng viết: "số nguyên", "danh sách chữ".
    fn show(&self, t: &Type) -> String {
        let word = |g: &String| self.lang.words.get(g).and_then(|w| w.first()).unwrap_or(g).clone();
        match t {
            Type::Simple(g) => word(g),
            Type::List(inner) => match &self.lang.list_keyword {
                Some(g) => format!("{} {}", word(g), self.show(inner)),
                None => format!("[{}]", self.show(inner)),
            },
        }
    }

    fn rank(&self, t: &Type) -> Option<usize> {
        match t {
            Type::Simple(g) => self.sem.numeric.iter().position(|n| n == g),
            Type::List(_) => None,
        }
    }

    /// Giá trị kiểu `got` có đặt được vào chỗ cần kiểu `want` không.
    fn fits(&self, want: &Type, got: &Type) -> bool {
        want == got || matches!((self.rank(want), self.rank(got)), (Some(w), Some(g)) if g <= w)
    }

    /// Số, hoặc kí tự: những kiểu mà mã đích so sánh được với nhau bằng giá trị.
    fn countable(&self, t: &Type) -> bool {
        self.rank(t).is_some() || matches!(t, Type::Simple(g) if *g == self.sem.character)
    }

    fn lookup(&mut self, name: &str, span: Span) -> Typed {
        match self.scopes.iter().rev().find_map(|s| s.get(name)).or_else(|| self.globals.get(name)) {
            Some(var) => (var.ty.clone(), var.boxed),
            None => {
                let mut e = Error::at(span, format!("biến «{name}» chưa được khai báo"));
                let known = self.scopes.iter().flat_map(|s| s.keys()).chain(self.globals.keys()).cloned().collect();
                e.unknown = Some((name.to_string(), known));
                self.errors.push(e);
                (None, false)
            }
        }
    }

    fn declare(&mut self, name: &str, ty: Option<Type>, span: Span, boxed: bool) {
        // Java không cho khai báo lại một biến cục bộ, kể cả ở khối lồng bên trong.
        if let Some(old) = self.scopes.iter().find_map(|s| s.get(name)) {
            let msg = format!("biến «{name}» đã được khai báo ở dòng {}", old.line);
            self.errors.push(Error::at(span, msg));
            return;
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), Var { ty, line: span.line, boxed });
        }
    }

    /// Ghi nhận trước mọi hàm và mọi biến cấp lớp trong khối, để hàm gọi được hàm
    /// và dùng được biến chung khai báo phía dưới nó.
    fn collect(&mut self, nodes: &[Node]) {
        for n in nodes {
            let rule = &self.lang.rules[n.rule];
            if let (Scope::Class, Some(d)) = (rule.scope, &rule.sem.declare) {
                let ty = match d.ty.as_ref().and_then(|s| slot(n, s)) {
                    Some(Value::Type(t)) => Some(t.clone()),
                    _ => None,
                };
                if let Some(Value::Name(name, span)) = slot(n, &d.name) {
                    let var = Var { ty, line: span.line, boxed: false };
                    if let Some(old) = self.globals.insert(name.clone(), var) {
                        let msg = format!("biến «{name}» đã được khai báo ở dòng {}", old.line);
                        self.errors.push(Error::at(*span, msg));
                    }
                }
            }
            let Some(f) = &rule.sem.function else { continue };
            let Some(Value::Name(name, span)) = f.name.as_ref().and_then(|s| slot(n, s)) else { continue };
            let params = match f.params.as_ref().and_then(|s| slot(n, s)) {
                Some(Value::Params(params)) => params.iter().map(|(t, _)| t.clone()).collect(),
                _ => Vec::new(),
            };
            let returns = match f.returns.as_ref().and_then(|s| slot(n, s)) {
                Some(Value::Type(t)) => Some(t.clone()),
                _ => None,
            };
            if self.funcs.insert(name.clone(), Func { params, returns }).is_some() {
                self.errors.push(Error::at(*span, format!("hàm «{name}» đã được định nghĩa trước đó")));
            }
        }
    }

    fn block(&mut self, nodes: &mut [Node]) {
        self.collect(nodes);
        self.scopes.push(HashMap::new());
        for n in nodes {
            self.node(n);
        }
        self.scopes.pop();
    }

    fn node(&mut self, n: &mut Node) {
        let lang = self.lang;
        let rule = &lang.rules[n.rule];
        let sem = &rule.sem;

        // 1. Kiểu của mọi biểu thức, tính trước khi luật khai báo thêm tên nào:
        //    trong "đặt a là a cộng 1", chữ a bên phải chưa tồn tại.
        let mut types: HashMap<String, (Typed, Span)> = HashMap::new();
        for (name, value) in n.slots.iter_mut() {
            match value {
                Value::Expr(e, span) => {
                    self.here = *span;
                    let t = self.expr(e);
                    types.insert(name.clone(), (t, *span));
                }
                Value::Type(t) => {
                    types.insert(name.clone(), ((Some(t.clone()), false), n.span));
                }
                _ => {}
            }
        }
        let typed = |slot: &Option<String>| slot.as_ref().and_then(|s| types.get(s)).cloned();
        let type_of = |slot: &Option<String>| typed(slot).map(|((t, _), span)| (t, span));

        // 2. Mọi chỗ trống tên không phải nơi đặt tên mới đều là nơi dùng một biến.
        let new_names: Vec<&String> = sem
            .defines
            .iter()
            .chain(sem.declare.as_ref().map(|d| &d.name))
            .chain(sem.function.as_ref().and_then(|f| f.name.as_ref()))
            .collect();
        let mut used: HashMap<String, Option<Type>> = HashMap::new();
        for (name, value) in &n.slots {
            if let Value::Name(var, span) = value {
                if !new_names.contains(&name) {
                    used.insert(name.clone(), self.lookup(var, *span).0);
                }
            }
        }

        // 3. Gán và trả về: kiểu của giá trị phải đặt được vào chỗ nhận.
        if let Some(a) = &sem.assign {
            let target = used.get(&a.name).cloned().flatten();
            if let (Some(want), Some((Some(got), span))) = (target, type_of(&Some(a.value.clone()))) {
                if !self.fits(&want, &got) {
                    let msg = format!("biến có kiểu {}, không gán được giá trị kiểu {}", self.show(&want), self.show(&got));
                    self.errors.push(Error::at(span, msg));
                }
            }
        }
        if let Some((got, span)) = type_of(&sem.gives) {
            match (&self.returns, got) {
                (Some(None), _) => {
                    let msg = "hàm này không khai báo kiểu trả về nên không trả về giá trị được";
                    self.errors.push(Error::at(span, msg));
                }
                (Some(Some(want)), Some(got)) if !self.fits(want, &got) => {
                    let msg = format!("hàm phải trả về {}, nhưng giá trị này có kiểu {}", self.show(want), self.show(&got));
                    self.errors.push(Error::at(span, msg));
                }
                _ => {}
            }
        }

        // Vòng đếm lên với hai mốc là số mà mốc đầu lớn hơn: sẽ không chạy lần nào.
        if let Some(a) = &sem.ascending {
            let number = |name: &String| match slot(n, name) {
                Some(Value::Expr(Expr::Num(text), span)) => text.parse::<f64>().ok().map(|v| (v, text.clone(), *span)),
                _ => None,
            };
            if let (Some((from, a_text, span)), Some((to, b_text, _))) = (number(&a.from), number(&a.to)) {
                if from > to {
                    let msg = format!("vòng lặp đếm lên từ {a_text} đến {b_text} sẽ không chạy lần nào; {}", a.hint);
                    self.errors.push(Error::at(span, msg));
                }
            }
        }

        // 4. Phạm vi: hàm mở một bộ biến riêng; biến của vòng lặp chỉ sống trong khối của nó.
        let has_blocks = n.slots.iter().any(|(_, v)| matches!(v, Value::Block(_)));
        let mut outer = None;
        let mut pushed = false;
        if let Some(f) = &sem.function {
            outer = Some((std::mem::take(&mut self.scopes), self.returns.take()));
            self.scopes.push(HashMap::new());
            if let Some(Value::Params(params)) = f.params.as_ref().and_then(|s| slot(n, s)) {
                for (ty, name) in params {
                    self.declare(name, Some(ty.clone()), n.span, false);
                }
            }
            self.returns = Some(type_of(&f.returns).and_then(|(t, _)| t));
        } else if let Some(d) = &sem.declare {
            let value = typed(&d.value);
            let declared = type_of(&d.ty).and_then(|(t, _)| t).or_else(|| d.is.as_deref().and_then(|g| self.simple(g)));
            if let (Some(want), Some(((Some(got), _), span))) = (&declared, &value) {
                if !self.fits(want, got) {
                    let msg = format!("biến có kiểu {}, không nhận được giá trị kiểu {}", self.show(want), self.show(got));
                    self.errors.push(Error::at(*span, msg));
                }
            }
            let element = match type_of(&d.element_of) {
                Some((Some(Type::List(inner)), _)) => Some(*inner),
                _ => None,
            };
            // Có khai kiểu thì mã đích giữ giá trị trần; lấy từ danh sách thì còn trong hộp.
            let boxed = declared.is_none() && (d.element_of.is_some() || value.as_ref().is_some_and(|((_, b), _)| *b));
            let ty = declared.or(element).or(value.and_then(|((t, _), _)| t));
            if has_blocks {
                self.scopes.push(HashMap::new());
                pushed = true;
            }
            // Biến cấp lớp đã được ghi nhận từ trước, trong collect().
            if rule.scope != Scope::Class {
                if let Some(Value::Name(name, span)) = slot(n, &d.name) {
                    self.declare(name, ty, *span, boxed);
                }
            }
        }

        for (_, value) in n.slots.iter_mut() {
            match value {
                Value::Block(nodes) => self.block(nodes),
                Value::Stmt(node) => self.node(node),
                _ => {}
            }
        }

        if pushed {
            self.scopes.pop();
        }
        if let Some((scopes, returns)) = outer {
            self.scopes = scopes;
            self.returns = returns;
        }
    }

    /// Kiểu kết quả của một toán tử trên các toán hạng đã biết kiểu.
    fn op_type(&self, op: &str, operands: &[&Option<Type>]) -> Option<Type> {
        let ops = &self.sem.operators;
        let has = |list: &[String]| list.iter().any(|o| o == op);
        let is = |t: &Option<Type>, group: &str| matches!(t, Some(Type::Simple(g)) if g == group);
        if has(&ops.boolean) {
            return self.simple(&self.sem.boolean);
        }
        if has(&ops.integer) {
            return self.simple(&self.sem.integer);
        }
        if has(&ops.decimal) {
            return self.simple(&self.sem.decimal);
        }
        if has(&ops.joins_text) && operands.iter().any(|t| is(t, &self.sem.text)) {
            return self.simple(&self.sem.text);
        }
        // Phép tính trên số: kết quả mang kiểu rộng nhất trong các toán hạng.
        let mut widest = 0;
        for t in operands {
            widest = widest.max(self.rank(t.as_ref()?)?);
        }
        self.simple(&self.sem.numeric[widest])
    }

    /// Mẫu riêng của toán tử cho các toán hạng này, nếu cấu hình có:
    ///   OP.BOXED  cả hai vế là số (hoặc kí tự) có thể đang nằm trong hộp
    ///   OP.KIỂU   theo kiểu của một toán hạng (EQ.STRING), hoặc LIST cho danh sách
    /// Riêng phép tính số học chọn theo kiểu của kết quả (ADD.INT), vì số nguyên cộng
    /// số thực phải dùng mẫu của số thực.
    fn variant(&self, op: &str, operands: &[&Typed], result: &Option<Type>) -> Option<String> {
        let exists = |key: String| self.lang.java.operators.contains_key(&key).then_some(key);
        let suffix = |t: &Option<Type>| match t {
            Some(Type::Simple(g)) => Some(g.clone()),
            Some(Type::List(_)) => Some("LIST".to_string()),
            None => None,
        };
        let both_boxed = operands.len() == 2
            && operands.iter().all(|(t, boxed)| *boxed && t.as_ref().is_some_and(|t| self.countable(t)));
        if both_boxed {
            if let Some(key) = exists(format!("{op}.BOXED")) {
                return Some(key);
            }
        }
        let ops = &self.sem.operators;
        let fixed_result = [&ops.boolean, &ops.integer, &ops.decimal].iter().any(|list| list.iter().any(|o| o == op));
        if fixed_result {
            operands.iter().find_map(|(t, _)| exists(format!("{op}.{}", suffix(t)?)))
        } else if operands.len() == 2 {
            exists(format!("{op}.{}", suffix(result)?))
        } else {
            None
        }
    }

    fn expr(&mut self, e: &mut Expr) -> Typed {
        let lang = self.lang;
        match e {
            Expr::Num(n) => {
                let group = if n.contains('.') {
                    &self.sem.decimal
                } else if n.parse::<i32>().is_err() {
                    self.sem.big_integer.as_ref().unwrap_or(&self.sem.integer)
                } else {
                    &self.sem.integer
                };
                (self.simple(group), false)
            }
            Expr::Str(s) => (self.simple(if s.starts_with('\'') { &self.sem.character } else { &self.sem.text }), false),
            Expr::Lit(_) => (self.simple(&self.sem.boolean), false),
            Expr::Interp(parts) => {
                for part in parts {
                    if let Part::Code(code) = part {
                        self.expr(code);
                    }
                }
                (self.simple(&self.sem.text), false)
            }
            Expr::Var(name, span) => self.lookup(name, *span),
            Expr::Call(name, span, args) => {
                let got: Vec<Option<Type>> = args.iter_mut().map(|a| self.expr(a).0).collect();
                let Some(f) = self.funcs.get(name.as_str()) else {
                    let mut e = Error::at(*span, format!("hàm «{name}» chưa được định nghĩa"));
                    e.unknown = Some((name.clone(), self.funcs.keys().cloned().collect()));
                    self.errors.push(e);
                    return (None, false);
                };
                let returns = f.returns.clone();
                let mut problem = None;
                if f.params.len() != got.len() {
                    problem = Some(format!("hàm «{name}» cần {} đối số, nhưng được gọi với {}", f.params.len(), got.len()));
                } else {
                    for (i, (want, got)) in f.params.iter().zip(&got).enumerate() {
                        if let Some(got) = got.as_ref().filter(|got| !self.fits(want, got)) {
                            let (want, got) = (self.show(want), self.show(got));
                            problem = Some(format!("đối số thứ {} của «{name}» phải là {want}, nhưng nhận {got}", i + 1));
                            break;
                        }
                    }
                }
                self.errors.extend(problem.map(|msg| Error::at(*span, msg)));
                (returns, false)
            }
            Expr::List(items) => {
                let types: Vec<Option<Type>> = items.iter_mut().map(|i| self.expr(i).0).collect();
                // Java suy kiểu phần tử từ cả danh sách; trộn kiểu cho ra một kiểu không dùng được.
                let first = types.first().cloned().flatten();
                if let Some(other) = types.iter().flatten().find(|t| Some(*t) != first.as_ref()) {
                    if let Some(first) = &first {
                        let (a, b) = (self.show(first), self.show(other));
                        let msg = format!("các phần tử của danh sách phải cùng kiểu, nhưng ở đây có cả {a} và {b}");
                        self.errors.push(Error::at(self.here, msg));
                    }
                }
                (first.map(|t| Type::List(Box::new(t))), false)
            }
            Expr::Index(list, index) => {
                self.expr(index);
                match self.expr(list).0 {
                    Some(Type::List(inner)) => (Some(*inner), true),
                    // Lấy một kí tự của chữ: mã đích dùng cách khác với danh sách.
                    Some(Type::Simple(g)) if g == self.sem.text && lang.java.operators.contains_key("INDEX.STRING") => {
                        let hole = || Box::new(Expr::Lit(String::new()));
                        let (list, index) = (std::mem::replace(list, hole()), std::mem::replace(index, hole()));
                        *e = Expr::Binary("INDEX.STRING".to_string(), list, index);
                        (self.simple(&self.sem.character), false)
                    }
                    Some(other) => {
                        let shown = self.show(&other);
                        let msg = format!("chỉ lấy phần tử theo vị trí được trên danh sách hoặc chữ, không phải trên {shown}");
                        self.errors.push(Error::at(self.here, msg));
                        (None, false)
                    }
                    None => (None, true),
                }
            }
            // Một cách viết khác của ds[i]: đổi về đúng dạng đó để kiểu và mẫu mã đích theo.
            Expr::Rule(node) if lang.rules[node.rule].sem.index.is_some() => {
                let index = lang.rules[node.rule].sem.index.as_ref().expect("vừa kiểm tra");
                let mut take = |name: &String| {
                    node.slots.iter_mut().find(|(k, _)| k == name).and_then(|(_, v)| match v {
                        Value::Expr(inner, _) => Some(Box::new(std::mem::replace(inner, Expr::Lit(String::new())))),
                        _ => None,
                    })
                };
                match (take(&index.of), take(&index.at)) {
                    (Some(list), Some(at)) => {
                        *e = Expr::Index(list, at);
                        self.expr(e)
                    }
                    _ => (None, false),
                }
            }
            Expr::Rule(node) => {
                self.node(node);
                (lang.rules[node.rule].sem.result.as_deref().and_then(|g| self.simple(g)), false)
            }
            Expr::Prefix(op, operand) => {
                let t = self.expr(operand);
                let result = self.op_type(op, &[&t.0]);
                if let Some(key) = self.variant(op, &[&t], &result) {
                    *op = key;
                }
                (result, false)
            }
            Expr::Binary(op, left, right) => {
                let (l, r) = (self.expr(left), self.expr(right));
                let result = self.op_type(op, &[&l.0, &r.0]);
                // Phép so sánh giữa hai kiểu không liên quan luôn sai; báo thay vì để nó chạy.
                if let (true, Some(a), Some(b)) = (self.sem.operators.compares.iter().any(|o| o == op), &l.0, &r.0) {
                    if a != b && !(self.countable(a) && self.countable(b)) {
                        let msg = format!("phép toán này không dùng được giữa {} và {}", self.show(a), self.show(b));
                        self.errors.push(Error::at(self.here, msg));
                    }
                }
                if let Some(key) = self.variant(op, &[&l, &r], &result) {
                    *op = key;
                }
                (result, false)
            }
        }
    }
}
