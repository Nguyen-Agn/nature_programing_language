//! Đọc file cấu hình và dựng các bảng tra cứu. Mọi mâu thuẫn trong cấu hình
//! bị từ chối ngay tại đây, để các tầng sau không phải đoán.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

use crate::error::Error;
use crate::lexer::Tok;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    keywords: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    literals: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    operators: BTreeMap<String, RawOp>,
    #[serde(default)]
    types: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    fillers: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    expression: RawExpr,
    #[serde(default)]
    lists: RawLists,
    semantics: Option<Semantics>,
    rules: Vec<RawRule>,
    java: RawJava,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOp {
    words: Vec<String>,
    prec: u8,
    prefix_prec: Option<u8>,
    #[serde(default)]
    prefix: bool,
    #[serde(default = "yes")]
    infix: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawExpr {
    operand_fillers: Option<String>,
    name_fillers: Option<String>,
    #[serde(default)]
    keyword_operators: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawLists {
    #[serde(default)]
    separators: Vec<String>,
    type_keyword: Option<String>,
    index_keyword: Option<String>,
    #[serde(default)]
    before_arguments: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JavaLists {
    #[serde(rename = "type")]
    pub ty: String,
    pub literal: String,
    pub index: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    name: String,
    scope: String,
    pattern: String,
    #[serde(flatten)]
    sem: Sem,
}

/// Các kiểu và toán tử mà tầng kiểm tra ngữ nghĩa cần biết tên.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Semantics {
    pub integer: String,
    pub decimal: String,
    pub text: String,
    pub character: String,
    pub boolean: String,
    /// Các kiểu số, từ hẹp đến rộng: kiểu đứng trước gán được vào kiểu đứng sau.
    pub numeric: Vec<String>,
    /// Kiểu của số nguyên quá lớn so với `integer` (8000000000).
    pub big_integer: Option<String>,
    /// Luật mà chương trình bắt buộc phải có: nơi bắt đầu chạy.
    pub entry_rule: Option<String>,
    #[serde(default)]
    pub operators: SemanticOps,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SemanticOps {
    /// Toán tử cho kết quả đúng/sai.
    #[serde(default)]
    pub boolean: Vec<String>,
    /// Toán tử so sánh: hai vế phải cùng loại kiểu.
    #[serde(default)]
    pub compares: Vec<String>,
    /// Toán tử cho kết quả số nguyên.
    #[serde(default)]
    pub integer: Vec<String>,
    /// Toán tử cho kết quả số thực.
    #[serde(default)]
    pub decimal: Vec<String>,
    /// Toán tử nối chuỗi khi một vế là chữ.
    #[serde(default)]
    pub joins_text: Vec<String>,
}

/// Ý nghĩa của một luật đối với tên và kiểu. Mọi trường đều là tên chỗ trống của luật.
#[derive(Deserialize, Default)]
pub struct Sem {
    /// Luật khai báo một biến.
    pub declare: Option<Declare>,
    /// Luật định nghĩa một hàm: thân của nó là một phạm vi biến riêng.
    pub function: Option<Function>,
    /// Chỗ trống tên là nơi đặt tên mới (tên lớp), không phải nơi dùng biến.
    #[serde(default)]
    pub defines: Vec<String>,
    /// Luật gán giá trị cho một biến đã có.
    pub assign: Option<Assign>,
    /// Luật trả giá trị về cho hàm đang chứa nó.
    pub gives: Option<String>,
    /// Kiểu của giá trị mà một luật scope biểu_thức tạo ra.
    pub result: Option<String>,
    /// Hai mốc của một vòng đếm lên.
    pub ascending: Option<Ascending>,
    /// Luật scope biểu_thức này là một cách viết khác của việc lấy phần tử theo vị
    /// trí ("phần tử thứ i của ds"): nó được hiểu đúng như ds[i].
    pub index: Option<IndexOf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexOf {
    pub of: String,
    pub at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ascending {
    pub from: String,
    pub to: String,
    /// Lời khuyên in kèm khi mốc đầu lớn hơn mốc cuối.
    pub hint: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declare {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Option<String>,
    pub value: Option<String>,
    pub element_of: Option<String>,
    /// Kiểu cố định của biến, khi luật không có chỗ trống kiểu ("nhập số nguyên a").
    pub is: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    pub name: Option<String>,
    pub params: Option<String>,
    pub returns: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assign {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJava {
    class_rule: Option<String>,
    class_slot: Option<String>,
    rules: BTreeMap<String, String>,
    #[serde(default)]
    operators: BTreeMap<String, String>,
    #[serde(default)]
    literals: BTreeMap<String, String>,
    #[serde(default)]
    types: BTreeMap<String, String>,
    #[serde(default)]
    boxed: BTreeMap<String, String>,
    lists: JavaLists,
    concat: String,
    long_suffix: Option<String>,
    header: Option<String>,
    footer: Option<String>,
    #[serde(default)]
    reserved: Vec<String>,
    #[serde(default)]
    messages: BTreeMap<String, String>,
}

pub struct Op {
    pub prec: u8,
    /// Toán tử đứng trước ôm cả biểu thức có độ ưu tiên từ mức này trở lên
    /// ("không phải a nhỏ hơn b"); None: chỉ ôm đúng một toán hạng ("-a").
    pub prefix_prec: Option<u8>,
    pub prefix: bool,
    pub infix: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    File,
    Class,
    Body,
    /// Luật dùng ở vị trí toán hạng trong biểu thức.
    Expr,
}

pub enum SlotKind {
    Name,
    Expr,
    /// Một toán hạng đơn, không kèm phép toán nào phía sau.
    Operand,
    Type,
    Params,
    Call,
    Block(Scope),
    Stmt,
}

pub enum Elem {
    Keyword(String),
    Opt(String),
    Many(String),
    /// Một ký hiệu viết nguyên văn trong luật: [ ] ( ) { } ,
    Sym(char),
    Slot { name: String, kind: SlotKind },
}

pub struct Rule {
    pub name: String,
    pub scope: Scope,
    pub elems: Vec<Elem>,
    pub sem: Sem,
}

pub enum Part {
    Text(String),
    Slot(String),
}

pub struct Java {
    pub class_rule: Option<String>,
    pub class_slot: Option<String>,
    pub rules: HashMap<String, Vec<Part>>,
    pub operators: HashMap<String, String>,
    pub literals: HashMap<String, String>,
    pub types: HashMap<String, String>,
    /// Kiểu dùng bên trong danh sách (Integer thay cho int); thiếu thì dùng `types`.
    pub boxed: HashMap<String, String>,
    pub lists: JavaLists,
    /// Mẫu nối hai mảnh của một chuỗi nội suy.
    pub concat: String,
    /// Hậu tố của số nguyên vượt quá kiểu số nguyên thường (Java: L).
    pub long_suffix: Option<String>,
    pub header: Option<String>,
    pub footer: Option<String>,
    /// Tên (chữ thường) mà mã sinh ra không được dùng, ngoài từ khóa Java.
    pub reserved: HashSet<String>,
    /// (đoạn trong thông báo của Java, câu tiếng Việt), đoạn dài xét trước.
    pub messages: Vec<(String, String)>,
}

pub struct Lang {
    /// Cụm đã chuẩn hóa -> token nội bộ.
    pub phrases: HashMap<String, Tok>,
    /// Số từ của cụm dài nhất, để lexer biết phải nhìn trước bao xa.
    pub max_words: usize,
    /// Các cụm ký hiệu (">=", "==", ...), dài trước ngắn sau.
    pub symbols: Vec<String>,
    /// Nhóm -> các cách viết, dùng khi báo "ở đây cần ...".
    pub words: HashMap<String, Vec<String>>,
    pub ops: HashMap<String, Op>,
    pub operand_fillers: Option<String>,
    /// Nhóm đệm được bỏ qua ngay trước một cái tên: "nhập số nguyên cho biến tuổi".
    pub name_fillers: Option<String>,
    /// Các cụm từ khóa cũng đóng vai đệm trước tên, khi theo sau chúng là một cái tên.
    pub keyword_name_fillers: HashSet<String>,
    /// Nhóm từ khóa được đọc như một toán tử khi nằm giữa biểu thức:
    /// "nếu a bằng b" dùng từ gán "bằng" với nghĩa so sánh.
    pub keyword_ops: HashMap<String, String>,
    /// Các nhóm từ khóa mở đầu một luật. Từ khóa giữa luật mà không thuộc tập này
    /// (như "không thì") được phép nằm ở dòng mới.
    pub starters: HashSet<String>,
    /// Các cụm (đã chuẩn hóa) có vai trò như dấu phẩy trong danh sách tham số.
    pub separators: HashSet<String>,
    /// Nhóm từ khóa mở đầu kiểu danh sách ("danh sách số nguyên").
    pub list_keyword: Option<String>,
    /// Nhóm từ khóa lấy phần tử theo vị trí khi đứng sau một giá trị: "ds thứ 0" là ds[0].
    pub index_keyword: Option<String>,
    /// Các nhóm (từ khóa hoặc đệm) được phép chen giữa tên hàm và dấu ( của danh sách
    /// tham số hay đối số: "Hàm tổng từ (số nguyên a)", "tổng với tham số (1, 2)".
    pub before_arguments: Vec<String>,
    pub rules: Vec<Rule>,
    /// Không có mục [semantics] thì bỏ qua tầng kiểm tra ngữ nghĩa.
    pub semantics: Option<Semantics>,
    pub java: Java,
}

/// Dạng so khớp của một cụm: NFC, chữ thường, các từ cách nhau đúng một dấu cách.
pub fn norm(s: &str) -> String {
    let s: String = s.nfc().collect::<String>().to_lowercase();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Default)]
struct Vocab {
    phrases: HashMap<String, Tok>,
    owner: HashMap<String, String>,
    words: HashMap<String, Vec<String>>,
    max_words: usize,
    symbols: Vec<String>,
    /// [expression] keyword_operators: nhóm từ khóa -> toán tử mà nó đóng vai giữa biểu thức.
    keyword_ops: BTreeMap<String, String>,
    /// [expression] name_fillers: nhóm đệm đứng trước tên.
    name_fillers: Option<String>,
    /// Các cụm vừa là từ khóa vừa được khai là đệm trước tên ("cho biến").
    keyword_name_fillers: HashSet<String>,
}

impl Vocab {
    /// Chuẩn hóa một cụm và ghi nhận nó là cụm chữ hay cụm ký hiệu.
    fn classify(&mut self, group: &str, w: &str) -> Result<String, Error> {
        let key = norm(w);
        // Cụm chữ: các từ, có thể xen số ở giữa ("in 1 dòng"). Từ đầu phải là chữ, vì
        // lexer chỉ bắt đầu dò cụm từ một từ.
        let is_word = !key.is_empty()
            && !key.starts_with(|c: char| c.is_numeric())
            && key.split(' ').all(|p| {
                p.chars().all(|c| c.is_ascii_digit())
                    || (p.chars().all(|c| c.is_alphanumeric() || c == '_') && !p.starts_with(|c: char| c.is_numeric()))
            });
        let is_symbol = !key.is_empty()
            && !key.starts_with("//")
            && key.chars().all(|c| !c.is_alphanumeric() && !c.is_whitespace() && !"{}()[],\"'_".contains(c));
        if is_word {
            self.max_words = self.max_words.max(key.split(' ').count());
        } else if is_symbol {
            if !self.symbols.contains(&key) {
                self.symbols.push(key.clone());
            }
        } else {
            return Err(Error::new(format!(
                "cụm «{w}» trong {group} không hợp lệ: một cụm hoặc gồm các từ (mở đầu bằng chữ), hoặc gồm toàn ký hiệu"
            )));
        }
        Ok(key)
    }

    /// Cụm ngăn cách được phép trùng với một toán tử ("và"): trong danh sách nó là dấu phẩy.
    fn separator(&mut self, w: &str) -> Result<String, Error> {
        let key = self.classify("[lists] separators", w)?;
        match self.phrases.get(&key) {
            Some(Tok::Op(_)) => {}
            Some(_) => {
                return Err(Error::new(format!(
                    "cụm ngăn cách «{w}» đã thuộc nhóm {}; chỉ được trùng với toán tử",
                    self.owner[&key]
                )));
            }
            None => {
                self.phrases.insert(key.clone(), Tok::Filler(Vec::new()));
                self.owner.insert(key.clone(), "[lists] separators".to_string());
            }
        }
        Ok(key)
    }

    fn group(&mut self, group: &str, words: &[String], tok: Tok) -> Result<(), Error> {
        if self.words.insert(group.to_string(), words.to_vec()).is_some() {
            return Err(Error::new(format!("nhóm {group} được khai báo ở hai bảng khác nhau")));
        }
        for w in words {
            let key = self.classify(group, w)?;
            if let Some(old) = self.phrases.get_mut(&key) {
                match (old, &tok) {
                    // Ghi một cụm hai lần trong cùng một nhóm thì chỉ là thừa.
                    _ if self.owner[&key] == group => {}
                    (Tok::Filler(groups), Tok::Filler(new)) => groups.extend(new.iter().cloned()),
                    // Từ khóa đã được cấu hình cho đóng vai toán tử này giữa biểu thức: ghi
                    // cụm ở cả hai nơi là thừa nhưng không mâu thuẫn. Giữ nó là từ khóa.
                    (Tok::Keyword(k), Tok::Op(o)) if self.keyword_ops.get(k) == Some(o) => {}
                    // Một từ khóa cũng được dùng làm đệm trước tên: "cho biến a là 5" mở đầu
                    // một câu lệnh, còn trong "nhập số nguyên cho biến tuổi" nó chỉ là đệm.
                    (Tok::Keyword(_), Tok::Filler(_)) if self.name_fillers.as_deref() == Some(group) => {
                        self.keyword_name_fillers.insert(key.clone());
                    }
                    (Tok::Keyword(k), Tok::Op(o)) => {
                        return Err(Error::new(format!(
                            "cụm «{w}» được khai báo ở cả {k} và {o}. Nếu muốn nó mở đầu câu lệnh là {k} còn \
                             giữa biểu thức là {o}, thêm {k} = \"{o}\" vào keyword_operators của [expression]"
                        )));
                    }
                    _ => {
                        return Err(Error::new(format!(
                            "cụm «{w}» được khai báo ở cả {} và {group}",
                            self.owner[&key]
                        )));
                    }
                }
            } else {
                self.phrases.insert(key.clone(), tok.clone());
                self.owner.insert(key, group.to_string());
            }
        }
        Ok(())
    }
}

fn parse_scope(s: &str) -> Option<Scope> {
    match norm(s).as_str() {
        "tệp" => Some(Scope::File),
        "lớp" => Some(Scope::Class),
        "thân" => Some(Scope::Body),
        "biểu_thức" => Some(Scope::Expr),
        _ => None,
    }
}

fn parse_pattern(rule: &str, pattern: &str, raw: &Raw) -> Result<Vec<Elem>, Error> {
    let bad = |piece: &str, why: &str| Error::new(format!("luật {rule}: «{piece}» {why}"));
    let mut elems = Vec::new();
    // Một chỗ trống `lệnh` đứng trước mọi thành phần bắt buộc sẽ gọi lại chính luật đó mãi.
    let mut anchored = false;
    for piece in pattern.split_whitespace() {
        let mut one = piece.chars();
        if let (Some(c), None) = (one.next(), one.next()) {
            if !"[](){},".contains(c) {
                return Err(bad(piece, "không phải ký hiệu được phép; dùng: [ ] ( ) { } ,"));
            }
            elems.push(Elem::Sym(c));
            anchored = true;
        } else if let Some(g) = piece.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            if !raw.keywords.contains_key(g) && !raw.types.contains_key(g) {
                return Err(bad(piece, "không phải nhóm nào trong [keywords] hay [types]"));
            }
            elems.push(Elem::Keyword(g.to_string()));
            anchored = true;
        } else if let Some(rest) = piece.strip_prefix('[') {
            let (g, many) = if let Some(g) = rest.strip_suffix("]?") {
                (g, false)
            } else if let Some(g) = rest.strip_suffix("]*") {
                (g, true)
            } else {
                return Err(bad(piece, "phải kết thúc bằng ]? hoặc ]*"));
            };
            if !raw.keywords.contains_key(g) && !raw.types.contains_key(g) && !raw.fillers.contains_key(g) {
                return Err(bad(piece, "không có trong [keywords], [types] hay [fillers]"));
            }
            elems.push(if many { Elem::Many(g.to_string()) } else { Elem::Opt(g.to_string()) });
        } else if let Some(inner) = piece.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
            let Some((name, kind)) = inner.split_once(':') else {
                return Err(bad(piece, "phải có dạng <tên:loại>"));
            };
            let kind = match norm(kind).as_str() {
                "tên" => SlotKind::Name,
                "biểu_thức" => SlotKind::Expr,
                "kiểu" => SlotKind::Type,
                "tham_số" => SlotKind::Params,
                "lời_gọi" => SlotKind::Call,
                "khối" => SlotKind::Block(Scope::Body),
                "khối_lớp" => SlotKind::Block(Scope::Class),
                "lệnh" => SlotKind::Stmt,
                "toán_hạng" => SlotKind::Operand,
                _ => return Err(bad(piece, "có loại lạ; dùng: tên, biểu_thức, toán_hạng, kiểu, tham_số, lời_gọi, khối, khối_lớp, lệnh")),
            };
            if matches!(kind, SlotKind::Stmt) && !anchored {
                return Err(bad(piece, "không được đứng trước thành phần bắt buộc đầu tiên"));
            }
            if elems.iter().any(|e| matches!(e, Elem::Slot { name: n, .. } if n == name)) {
                return Err(bad(piece, "trùng tên với một chỗ trống khác"));
            }
            elems.push(Elem::Slot { name: name.to_string(), kind });
            anchored = true;
        } else {
            return Err(bad(piece, "không đúng cú pháp; dùng (NHÓM), [NHÓM]?, [NHÓM]* hoặc <tên:loại>"));
        }
    }
    if !anchored {
        return Err(Error::new(format!("luật {rule}: cần ít nhất một thành phần bắt buộc")));
    }
    Ok(elems)
}

fn parse_template(rule: &str, tpl: &str) -> Result<Vec<Part>, Error> {
    let mut parts = Vec::new();
    let mut rest = tpl;
    while let Some(open) = rest.find('{') {
        let Some(len) = rest[open..].find('}') else {
            return Err(Error::new(format!("[java.rules] {rule}: thiếu dấu }} đóng")));
        };
        if open > 0 {
            parts.push(Part::Text(rest[..open].to_string()));
        }
        parts.push(Part::Slot(rest[open + 1..open + len].to_string()));
        rest = &rest[open + len + 1..];
    }
    if !rest.is_empty() {
        parts.push(Part::Text(rest.to_string()));
    }
    Ok(parts)
}

pub fn load(text: &str) -> Result<Lang, Error> {
    let mut raw: Raw =
        toml::from_str(text).map_err(|e| Error::new(format!("sai định dạng TOML: {e}")))?;

    let mut vocab = Vocab {
        keyword_ops: raw.expression.keyword_operators.clone(),
        name_fillers: raw.expression.name_fillers.clone(),
        ..Vocab::default()
    };
    let mut ops = HashMap::new();
    for (g, words) in &raw.keywords {
        vocab.group(g, words, Tok::Keyword(g.clone()))?;
    }
    for (g, words) in &raw.literals {
        vocab.group(g, words, Tok::Literal(g.clone()))?;
        if !raw.java.literals.contains_key(g) {
            return Err(Error::new(format!("hằng {g} chưa có trong [java.literals]")));
        }
    }
    for (g, op) in &raw.operators {
        vocab.group(g, &op.words, Tok::Op(g.clone()))?;
        if !raw.java.operators.contains_key(g) {
            return Err(Error::new(format!("toán tử {g} chưa có trong [java.operators]")));
        }
        if !op.prefix && !op.infix {
            return Err(Error::new(format!("toán tử {g}: prefix và infix không thể cùng tắt")));
        }
        ops.insert(g.clone(), Op { prec: op.prec, prefix_prec: op.prefix_prec, prefix: op.prefix, infix: op.infix });
    }
    for (g, words) in &raw.types {
        vocab.group(g, words, Tok::Type(g.clone()))?;
        if !raw.java.types.contains_key(g) {
            return Err(Error::new(format!("kiểu {g} chưa có trong [java.types]")));
        }
    }
    for (g, words) in &raw.fillers {
        vocab.group(g, words, Tok::Filler(vec![g.clone()]))?;
    }
    let mut separators = HashSet::new();
    for w in &raw.lists.separators {
        separators.insert(vocab.separator(w)?);
    }
    if let Some(g) = &raw.expression.name_fillers {
        if !raw.fillers.contains_key(g) {
            return Err(Error::new(format!("[expression] name_fillers: không có nhóm đệm {g}")));
        }
    }
    if let Some(g) = &raw.expression.operand_fillers {
        if !raw.fillers.contains_key(g) {
            return Err(Error::new(format!("[expression] operand_fillers: không có nhóm đệm {g}")));
        }
    }

    for (keyword, op) in &raw.expression.keyword_operators {
        if !raw.keywords.contains_key(keyword) || !ops.contains_key(op) {
            return Err(Error::new(format!(
                "[expression] keyword_operators: {keyword} phải là nhóm từ khóa và {op} phải là toán tử"
            )));
        }
    }
    let mut rules = Vec::new();
    let mut templates = HashMap::new();
    let mut seen = HashSet::new();
    for r in std::mem::take(&mut raw.rules) {
        if !seen.insert(r.name.clone()) {
            return Err(Error::new(format!("luật {} bị khai báo hai lần", r.name)));
        }
        let scope = parse_scope(&r.scope).ok_or_else(|| {
            Error::new(format!("luật {}: scope «{}» lạ; dùng: tệp, lớp, thân", r.name, r.scope))
        })?;
        let elems = parse_pattern(&r.name, &r.pattern, &raw)?;
        if scope == Scope::Expr && !matches!(elems.first(), Some(Elem::Keyword(_))) {
            return Err(Error::new(format!(
                "luật {}: luật scope biểu_thức phải mở đầu bằng một (TỪ_KHÓA)",
                r.name
            )));
        }
        let tpl = raw.java.rules.get(&r.name).ok_or_else(|| {
            Error::new(format!("luật {} chưa có mẫu trong [java.rules]", r.name))
        })?;
        let parts = parse_template(&r.name, tpl)?;
        for part in &parts {
            if let Part::Slot(s) = part {
                if !elems.iter().any(|e| matches!(e, Elem::Slot { name, .. } if name == s)) {
                    return Err(Error::new(format!(
                        "[java.rules] {}: {{{s}}} không phải chỗ trống nào của luật",
                        r.name
                    )));
                }
            }
        }
        // Mọi chỗ trống mà phần ngữ nghĩa nhắc tới phải có trong luật.
        let sem = &r.sem;
        let mut mentioned: Vec<&String> = sem.defines.iter().chain(&sem.gives).collect();
        if let Some(d) = &sem.declare {
            mentioned.extend(std::iter::once(&d.name).chain(&d.ty).chain(&d.value).chain(&d.element_of));
        }
        if let Some(f) = &sem.function {
            mentioned.extend(f.name.iter().chain(&f.params).chain(&f.returns));
        }
        if let Some(a) = &sem.assign {
            mentioned.extend([&a.name, &a.value]);
        }
        if let Some(a) = &sem.ascending {
            mentioned.extend([&a.from, &a.to]);
        }
        if let Some(i) = &sem.index {
            mentioned.extend([&i.of, &i.at]);
        }
        if let Some(fixed) = sem.declare.as_ref().and_then(|d| d.is.as_ref()) {
            if !raw.types.contains_key(fixed) {
                return Err(Error::new(format!("luật {}: declare.is = «{fixed}» không phải kiểu nào trong [types]", r.name)));
            }
        }
        for slot in mentioned {
            if !elems.iter().any(|e| matches!(e, Elem::Slot { name, .. } if name == slot)) {
                return Err(Error::new(format!(
                    "luật {}: phần ngữ nghĩa nhắc tới chỗ trống «{slot}» không có trong luật",
                    r.name
                )));
            }
        }
        templates.insert(r.name.clone(), parts);
        rules.push(Rule { name: r.name, scope, elems, sem: r.sem });
    }
    for g in &raw.lists.before_arguments {
        if !raw.keywords.contains_key(g) && !raw.fillers.contains_key(g) {
            return Err(Error::new(format!("[lists] before_arguments: không có nhóm từ khóa hay nhóm đệm {g}")));
        }
    }
    if let Some(g) = &raw.lists.index_keyword {
        if !raw.keywords.contains_key(g) {
            return Err(Error::new(format!("[lists] index_keyword: không có nhóm từ khóa {g}")));
        }
    }
    if let Some(g) = &raw.lists.type_keyword {
        if !raw.keywords.contains_key(g) {
            return Err(Error::new(format!("[lists] type_keyword: không có nhóm từ khóa {g}")));
        }
    }
    if let Some(name) = &raw.java.class_rule {
        if !seen.contains(name) {
            return Err(Error::new(format!("[java] class_rule: không có luật {name}")));
        }
    }

    vocab.symbols.sort_by_key(|s| std::cmp::Reverse(s.chars().count()));
    let mut messages: Vec<(String, String)> = raw.java.messages.into_iter().collect();
    messages.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));
    Ok(Lang {
        phrases: vocab.phrases,
        max_words: vocab.max_words,
        symbols: vocab.symbols,
        words: vocab.words,
        ops,
        operand_fillers: raw.expression.operand_fillers,
        name_fillers: raw.expression.name_fillers,
        keyword_name_fillers: vocab.keyword_name_fillers,
        keyword_ops: raw.expression.keyword_operators.into_iter().collect(),
        starters: rules.iter().filter_map(|r| match r.elems.first() {
            Some(Elem::Keyword(g)) => Some(g.clone()),
            _ => None,
        }).collect(),
        separators,
        list_keyword: raw.lists.type_keyword,
        index_keyword: raw.lists.index_keyword,
        before_arguments: raw.lists.before_arguments,
        rules,
        semantics: raw.semantics,
        java: Java {
            class_rule: raw.java.class_rule,
            class_slot: raw.java.class_slot,
            rules: templates,
            operators: raw.java.operators.into_iter().collect(),
            literals: raw.java.literals.into_iter().collect(),
            types: raw.java.types.into_iter().collect(),
            boxed: raw.java.boxed.into_iter().collect(),
            lists: raw.java.lists,
            concat: raw.java.concat,
            long_suffix: raw.java.long_suffix,
            header: raw.java.header,
            footer: raw.java.footer,
            reserved: raw.java.reserved.iter().map(|s| s.to_lowercase()).collect(),
            messages,
        },
    })
}
