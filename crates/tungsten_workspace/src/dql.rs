//! DQL — a small subset of Dataview Query Language.
//!
//! Dataview's full DQL is large; this module implements a
//! pragmatic subset that's enough to power the most common
//! personal-knowledge queries a user types in the Daily Journal
//! panel. The grammar accepted here:
//!
//! ```text
//! query     := source [FROM from] [WHERE where] [SORT sort] [LIMIT n] [;]
//! source    := LIST
//!            | TABLE field ("," field)*
//! from      := tag
//!            | "string"
//!            | AND-clause   (folded into a list of froms; OR'd)
//! where     := condition (AND condition | OR condition)*
//! condition := comparison
//!            | CONTAINS "(" ident "," literal ")"
//!            | STARTSWITH "(" ident "," literal ")"
//! comparison := ident CmpOp literal
//! CmpOp     := "=" | "!=" | "<" | ">" | "<=" | ">="
//! ident     := dotted path (e.g. file.name, file.tags, file.path,
//!                         file.mtime, file.size, file.ctime,
//!                         or a YAML frontmatter key like "status")
//! literal   := string | number | bool
//! sort      := ident [ASC | DESC]
//! field     := ident   (for TABLE — the values to render per row)
//! ```
//!
//! `LIST` returns one row per matching note. `TABLE field1, ...`
//! returns one row per note with the requested field values. The
//! executor runs against a [`NoteIndex`] and produces a
//! [`DqlResult`].

use std::path::Path;

use crate::index::NoteIndex;
use crate::note::Note;
use crate::search::PropertyFilter;

/// The top-level AST node.
#[derive(Debug, Clone, PartialEq)]
pub struct DqlQuery {
    pub source: SourceType,
    pub from: Option<FromClause>,
    pub r#where: Option<WhereClause>,
    pub sort: Option<SortClause>,
    pub limit: Option<usize>,
}

/// What kind of result the query produces.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceType {
    /// One row per note (just the title/path).
    List,
    /// One row per note with the requested field values, in
    /// order. `Table(vec)` carries the field names in display
    /// order.
    Table(Vec<String>),
}

impl SourceType {
    pub fn keyword(&self) -> &'static str {
        match self {
            SourceType::List => "LIST",
            SourceType::Table(_) => "TABLE",
        }
    }
}

/// A `FROM` clause: the universe of notes the query operates on.
#[derive(Debug, Clone, PartialEq)]
pub enum FromClause {
    /// `#tag` — only notes that have this tag.
    Tag(String),
    /// `"path"` or `"folder"` — only notes whose path contains
    /// this substring. We don't enforce a folder vs file
    /// distinction at the AST level; the executor treats both
    /// as a substring match against the canonical path.
    Folder(String),
    /// `[[Note]]` — only notes that link to the given note.
    /// (For now, treated as a tag whose name is the note's title.)
    Note(String),
}

/// A `WHERE` clause: a tree of AND/OR conditions.
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    pub root: WhereNode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WhereNode {
    And(Box<WhereNode>, Box<WhereNode>),
    Or(Box<WhereNode>, Box<WhereNode>),
    /// `CONTAINS(ident, literal)`.
    Contains(Ident, Literal),
    /// `STARTSWITH(ident, literal)`.
    StartsWith(Ident, Literal),
    /// `ident = literal`, `!=`, etc.
    Compare(Ident, CompareOp, Literal),
    /// `true` constant (matches everything); used internally as
    /// the empty WHERE.
    True,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SortClause {
    pub field: Ident,
    pub descending: bool,
}

/// A dotted identifier. `file.name`, `file.tags`, `status`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident(pub String);

impl Ident {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A literal value in a query.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

impl Literal {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Literal::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Literal::Number(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Literal::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// The result of executing a DqlQuery against an index.
#[derive(Debug, Clone, PartialEq)]
pub struct DqlResult {
    pub source_type: SourceType,
    pub rows: Vec<DqlRow>,
}

/// One row of a DqlResult.
#[derive(Debug, Clone, PartialEq)]
pub enum DqlRow {
    /// A single note (LIST result).
    Note(DqlNoteRow),
    /// A note with the requested field values, in the same order
    /// as the TABLE field list.
    Table(DqlNoteRow, Vec<String>),
}

impl DqlRow {
    pub fn note(&self) -> &DqlNoteRow {
        match self {
            DqlRow::Note(n) => n,
            DqlRow::Table(n, _) => n,
        }
    }
}

/// A trimmed-down note summary used in query results.
#[derive(Debug, Clone, PartialEq)]
pub struct DqlNoteRow {
    pub title: String,
    pub path: std::path::PathBuf,
    pub tags: Vec<String>,
}

impl DqlNoteRow {
    pub fn from_note(n: &Note) -> Self {
        Self {
            title: n.title.clone(),
            path: n.path.clone(),
            tags: n.tags.clone(),
        }
    }
}

impl DqlResult {
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Errors from the DQL parser.
#[derive(Debug, thiserror::Error)]
pub enum DqlError {
    #[error("unexpected token at position {0}: {1:?}")]
    UnexpectedToken(usize, Token),
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("invalid source type: expected LIST or TABLE, got {0:?}")]
    InvalidSource(Token),
    #[error("invalid FROM clause: expected #tag or \"path\", got {0:?}")]
    InvalidFrom(Token),
    #[error("invalid WHERE clause: {0}")]
    InvalidWhere(String),
    #[error("invalid SORT clause: {0}")]
    InvalidSort(String),
    #[error("invalid LIMIT value: {0}")]
    InvalidLimit(String),
    #[error("expected '{0}', got {1:?}")]
    Expected(String, Token),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    String(String),
    Number(f64),
    /// `#tag`
    Tag(String),
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `,`
    Comma,
    /// `=`, `!=`, `<`, `>`, `<=`, `>=`
    CmpOp(CompareOp),
    Keyword(Keyword),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    List,
    Table,
    From,
    Where,
    Sort,
    Limit,
    Asc,
    Desc,
    And,
    Or,
    Contains,
    StartsWith,
}

impl Keyword {
    fn from_str(s: &str) -> Option<Self> {
        Some(match s.to_ascii_uppercase().as_str() {
            "LIST" => Keyword::List,
            "TABLE" => Keyword::Table,
            "FROM" => Keyword::From,
            "WHERE" => Keyword::Where,
            "SORT" => Keyword::Sort,
            "LIMIT" => Keyword::Limit,
            "ASC" => Keyword::Asc,
            "DESC" => Keyword::Desc,
            "AND" => Keyword::And,
            "OR" => Keyword::Or,
            "CONTAINS" => Keyword::Contains,
            "STARTSWITH" => Keyword::StartsWith,
            _ => return None,
        })
    }
}

/// Lex a DQL query string into a flat list of tokens.
pub fn tokenize(input: &str) -> Result<Vec<Token>, DqlError> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // String literal
        if c == '"' || c == '\'' {
            let quote = c;
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != quote as u8 {
                end += 1;
            }
            if end >= bytes.len() {
                return Err(DqlError::UnexpectedEof);
            }
            let s = &input[start..end];
            tokens.push(Token::String(s.to_string()));
            i = end + 1;
            continue;
        }
        // Tag
        if c == '#' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() {
                let cc = bytes[end] as char;
                if cc.is_whitespace() || cc == '(' || cc == ')' || cc == ',' {
                    break;
                }
                end += 1;
            }
            if end == start {
                return Err(DqlError::InvalidFrom(Token::Tag(String::new())));
            }
            tokens.push(Token::Tag(input[start..end].to_string()));
            i = end;
            continue;
        }
        // Punctuation
        if c == '(' {
            tokens.push(Token::LParen);
            i += 1;
            continue;
        }
        if c == ')' {
            tokens.push(Token::RParen);
            i += 1;
            continue;
        }
        if c == ',' {
            tokens.push(Token::Comma);
            i += 1;
            continue;
        }
        // Comparison operators
        if c == '=' || c == '<' || c == '>' || c == '!' {
            let (op, len) = match c {
                '=' => (CompareOp::Eq, 1),
                '<' => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                        (CompareOp::Le, 2)
                    } else {
                        (CompareOp::Lt, 1)
                    }
                }
                '>' => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                        (CompareOp::Ge, 2)
                    } else {
                        (CompareOp::Gt, 1)
                    }
                }
                '!' => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                        (CompareOp::Ne, 2)
                    } else {
                        return Err(DqlError::UnexpectedToken(i, Token::Ident("!".into())));
                    }
                }
                _ => unreachable!(),
            };
            tokens.push(Token::CmpOp(op));
            i += len;
            continue;
        }
        // Number
        if c.is_ascii_digit() || (c == '-' && i + 1 < bytes.len() && (bytes[i + 1] as char).is_ascii_digit()) {
            let start = i;
            let mut end = i;
            if bytes[end] == b'-' {
                end += 1;
            }
            while end < bytes.len() {
                let cc = bytes[end] as char;
                if !(cc.is_ascii_digit() || cc == '.') {
                    break;
                }
                end += 1;
            }
            let s = &input[start..end];
            let n: f64 = s
                .parse()
                .map_err(|_| DqlError::InvalidLimit(s.to_string()))?;
            tokens.push(Token::Number(n));
            i = end;
            continue;
        }
        // Identifier or keyword
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            let mut end = i;
            while end < bytes.len() {
                let cc = bytes[end] as char;
                if !(cc.is_ascii_alphanumeric() || cc == '_' || cc == '.') {
                    break;
                }
                end += 1;
            }
            let s = &input[start..end];
            if let Some(kw) = Keyword::from_str(s) {
                tokens.push(Token::Keyword(kw));
            } else {
                tokens.push(Token::Ident(s.to_string()));
            }
            i = end;
            continue;
        }
        return Err(DqlError::UnexpectedToken(i, Token::Ident(c.to_string())));
    }
    Ok(tokens)
}

/// Parse a token stream into a DqlQuery.
pub fn parse(tokens: Vec<Token>) -> Result<DqlQuery, DqlError> {
    let mut p = Parser { tokens, pos: 0 };
    let query = p.parse_query()?;
    Ok(query)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
    fn next(&mut self) -> Result<Token, DqlError> {
        if self.pos >= self.tokens.len() {
            return Err(DqlError::UnexpectedEof);
        }
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        Ok(t)
    }
    fn expect_keyword(&mut self, kw: Keyword) -> Result<(), DqlError> {
        let t = self.next()?;
        match t {
            Token::Keyword(k) if k == kw => Ok(()),
            other => Err(DqlError::Expected(format!("{:?}", kw), other)),
        }
    }
    fn parse_query(&mut self) -> Result<DqlQuery, DqlError> {
        let source = self.parse_source()?;
        let from = self.parse_from_optional()?;
        let r#where = self.parse_where_optional()?;
        let sort = self.parse_sort_optional()?;
        let limit = self.parse_limit_optional()?;
        Ok(DqlQuery {
            source,
            from,
            r#where,
            sort,
            limit,
        })
    }
    fn parse_source(&mut self) -> Result<SourceType, DqlError> {
        let t = self.next()?;
        match t {
            Token::Keyword(Keyword::List) => Ok(SourceType::List),
            Token::Keyword(Keyword::Table) => {
                let mut fields = Vec::new();
                loop {
                    let t = self.next()?;
                    match t {
                        Token::Ident(s) => fields.push(s),
                        other => {
                            return Err(DqlError::UnexpectedToken(
                                self.pos - 1,
                                other,
                            ))
                        }
                    }
                    if let Some(Token::Comma) = self.peek() {
                        self.next()?;
                        continue;
                    }
                    break;
                }
                if fields.is_empty() {
                    return Err(DqlError::Expected("field name".into(), Token::Ident(String::new())));
                }
                Ok(SourceType::Table(fields))
            }
            other => Err(DqlError::InvalidSource(other)),
        }
    }
    fn parse_from_optional(&mut self) -> Result<Option<FromClause>, DqlError> {
        if !matches!(self.peek(), Some(Token::Keyword(Keyword::From))) {
            return Ok(None);
        }
        self.next()?;
        let t = self.next()?;
        match t {
            Token::Tag(tag) => Ok(Some(FromClause::Tag(tag))),
            Token::String(s) => Ok(Some(FromClause::Folder(s))),
            // [[Note]] is lexed as '[' + ident + ']' but we don't
            // tokenize brackets — accept "Note" as a note ref.
            Token::Ident(s) => Ok(Some(FromClause::Note(s))),
            other => Err(DqlError::InvalidFrom(other)),
        }
    }
    fn parse_where_optional(&mut self) -> Result<Option<WhereClause>, DqlError> {
        if !matches!(self.peek(), Some(Token::Keyword(Keyword::Where))) {
            return Ok(None);
        }
        self.next()?;
        let node = self.parse_where_or()?;
        Ok(Some(WhereClause { root: node }))
    }
    fn parse_where_or(&mut self) -> Result<WhereNode, DqlError> {
        let mut left = self.parse_where_and()?;
        while matches!(self.peek(), Some(Token::Keyword(Keyword::Or))) {
            self.next()?;
            let right = self.parse_where_and()?;
            left = WhereNode::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn parse_where_and(&mut self) -> Result<WhereNode, DqlError> {
        let mut left = self.parse_where_atom()?;
        while matches!(self.peek(), Some(Token::Keyword(Keyword::And))) {
            self.next()?;
            let right = self.parse_where_atom()?;
            left = WhereNode::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn parse_where_atom(&mut self) -> Result<WhereNode, DqlError> {
        // Function form: CONTAINS(ident, literal) | STARTSWITH(ident, literal)
        if let Some(Token::Keyword(Keyword::Contains)) = self.peek() {
            self.next()?;
            self.next()?; // (
            let ident = self.parse_ident()?;
            self.next()?; // ,
            let lit = self.parse_literal()?;
            self.next()?; // )
            return Ok(WhereNode::Contains(ident, lit));
        }
        if let Some(Token::Keyword(Keyword::StartsWith)) = self.peek() {
            self.next()?;
            self.next()?;
            let ident = self.parse_ident()?;
            self.next()?;
            let lit = self.parse_literal()?;
            self.next()?;
            return Ok(WhereNode::StartsWith(ident, lit));
        }
        // ident CmpOp literal
        let ident = self.parse_ident()?;
        let op = match self.next()? {
            Token::CmpOp(op) => op,
            other => return Err(DqlError::Expected("comparison operator".into(), other)),
        };
        let lit = self.parse_literal()?;
        Ok(WhereNode::Compare(ident, op, lit))
    }
    fn parse_ident(&mut self) -> Result<Ident, DqlError> {
        match self.next()? {
            Token::Ident(s) => Ok(Ident(s)),
            other => Err(DqlError::Expected("identifier".into(), other)),
        }
    }
    fn parse_literal(&mut self) -> Result<Literal, DqlError> {
        match self.next()? {
            Token::String(s) => Ok(Literal::String(s)),
            Token::Number(n) => Ok(Literal::Number(n)),
            Token::Ident(s) if s.eq_ignore_ascii_case("true") => Ok(Literal::Bool(true)),
            Token::Ident(s) if s.eq_ignore_ascii_case("false") => Ok(Literal::Bool(false)),
            Token::Ident(s) if s.eq_ignore_ascii_case("null") => Ok(Literal::Null),
            other => Err(DqlError::Expected("literal".into(), other)),
        }
    }
    fn parse_sort_optional(&mut self) -> Result<Option<SortClause>, DqlError> {
        if !matches!(self.peek(), Some(Token::Keyword(Keyword::Sort))) {
            return Ok(None);
        }
        self.next()?;
        let ident = self.parse_ident()?;
        let mut descending = false;
        if let Some(Token::Keyword(Keyword::Asc)) = self.peek() {
            self.next()?;
        } else if let Some(Token::Keyword(Keyword::Desc)) = self.peek() {
            self.next()?;
            descending = true;
        }
        Ok(Some(SortClause { field: ident, descending }))
    }
    fn parse_limit_optional(&mut self) -> Result<Option<usize>, DqlError> {
        if !matches!(self.peek(), Some(Token::Keyword(Keyword::Limit))) {
            return Ok(None);
        }
        self.next()?;
        match self.next()? {
            Token::Number(n) => Ok(Some(n as usize)),
            other => Err(DqlError::InvalidLimit(format!("{other:?}"))),
        }
    }
}

/// Parse a DQL query from a string.
pub fn parse_query(input: &str) -> Result<DqlQuery, DqlError> {
    let tokens = tokenize(input)?;
    parse(tokens)
}

/// Execute a parsed DQL query against an index.
pub fn execute(query: &DqlQuery, index: &NoteIndex) -> DqlResult {
    let candidates: Vec<&Note> = match &query.from {
        Some(FromClause::Tag(t)) => {
            let t_lower = t.to_lowercase();
            index.with_tag(&t_lower).collect()
        }
        Some(FromClause::Folder(p)) => {
            let p_lower = p.to_lowercase();
            index
                .notes()
                .filter(|n| n.path.to_string_lossy().to_lowercase().contains(&p_lower))
                .collect()
        }
        Some(FromClause::Note(target)) => {
            // Treat as "notes that link to this target". For a
            // single-noted start, that's the backlinks set.
            let key = crate::index::link_key(target);
            index.backlinks(&key).collect()
        }
        None => index.notes().collect(),
    };

    let filtered: Vec<&Note> = if let Some(w) = &query.r#where {
        candidates
            .into_iter()
            .filter(|n| eval_where(&w.root, n))
            .collect()
    } else {
        candidates
    };

    let mut sorted = filtered;
    if let Some(sort) = &query.sort {
        sort_notes(&mut sorted, &sort.field, sort.descending);
    }

    let limited: Vec<&Note> = if let Some(n) = query.limit {
        sorted.into_iter().take(n).collect()
    } else {
        sorted
    };

    let rows: Vec<DqlRow> = limited
        .into_iter()
        .map(|n| match &query.source {
            SourceType::List => DqlRow::Note(DqlNoteRow::from_note(n)),
            SourceType::Table(fields) => {
                let values: Vec<String> = fields
                    .iter()
                    .map(|f| render_field(n, f))
                    .collect();
                DqlRow::Table(DqlNoteRow::from_note(n), values)
            }
        })
        .collect();

    DqlResult {
        source_type: query.source.clone(),
        rows,
    }
}

fn eval_where(node: &WhereNode, note: &Note) -> bool {
    match node {
        WhereNode::True => true,
        WhereNode::And(a, b) => eval_where(a, note) && eval_where(b, note),
        WhereNode::Or(a, b) => eval_where(a, note) || eval_where(b, note),
        WhereNode::Contains(ident, lit) => {
            let value = render_field(note, &ident.0);
            let needle = match lit {
                Literal::String(s) => s.to_lowercase(),
                _ => format!("{lit:?}").to_lowercase(),
            };
            value.to_lowercase().contains(&needle)
        }
        WhereNode::StartsWith(ident, lit) => {
            let value = render_field(note, &ident.0);
            let prefix = match lit {
                Literal::String(s) => s.clone(),
                _ => format!("{lit:?}"),
            };
            value.starts_with(&prefix)
        }
        WhereNode::Compare(ident, op, lit) => {
            let value = render_field(note, &ident.0);
            compare(&value, *op, lit)
        }
    }
}

fn sort_notes(notes: &mut Vec<&Note>, field: &Ident, descending: bool) {
    notes.sort_by(|a, b| {
        let va = render_field(a, &field.0);
        let vb = render_field(b, &field.0);
        let cmp = va.cmp(&vb);
        if descending {
            cmp.reverse()
        } else {
            cmp
        }
    });
}

/// Render an identifier to a string value for a given note. The
/// supported `file.*` namespace is:
/// - `file.name`     — basename including extension
/// - `file.path`     — absolute path
/// - `file.folder`   — parent directory
/// - `file.size`     — size in bytes (decimal string)
/// - `file.tags`     — comma-separated tags (used by CONTAINS)
/// - `file.ext`      — extension without dot
/// - `file.mtime`    — mtime as unix seconds (decimal string)
/// - `file.ctime`    — ctime as unix seconds (decimal string)
///
/// Any other identifier is treated as a YAML frontmatter key and
/// looked up in `note.frontmatter`. If the key is missing, the
/// value is the empty string (which compares as < any non-empty
/// value and is "not contains" any needle).
pub fn render_field(note: &Note, ident: &str) -> String {
    if let Some(rest) = ident.strip_prefix("file.") {
        return match rest {
            "name" => note
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
            "path" => note.path.to_string_lossy().to_string(),
            "folder" => note
                .path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
            "size" => note.size_bytes.to_string(),
            "tags" => note.tags.join(","),
            "ext" => note
                .path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
            "mtime" | "ctime" => note
                .mtime
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default(),
            _ => String::new(),
        };
    }
    // YAML frontmatter lookup.
    let key = ident;
    if let serde_yaml::Value::Mapping(m) = &note.frontmatter {
        for (k, v) in m {
            if let serde_yaml::Value::String(s) = k {
                if s == key {
                    return yaml_value_to_string(v);
                }
            }
        }
    }
    String::new()
}

fn yaml_value_to_string(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => String::new(),
        serde_yaml::Value::Sequence(seq) => {
            let parts: Vec<String> = seq.iter().map(yaml_value_to_string).collect();
            parts.join(",")
        }
        _ => format!("{v:?}"),
    }
}

fn compare(value: &str, op: CompareOp, lit: &Literal) -> bool {
    let rhs = match lit {
        Literal::String(s) => s.clone(),
        Literal::Number(n) => n.to_string(),
        Literal::Bool(b) => b.to_string(),
        Literal::Null => String::new(),
    };
    match op {
        CompareOp::Eq => value == rhs,
        CompareOp::Ne => value != rhs,
        CompareOp::Lt => value < rhs.as_str(),
        CompareOp::Gt => value > rhs.as_str(),
        CompareOp::Le => value <= rhs.as_str(),
        CompareOp::Ge => value >= rhs.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_vault(label: &str) -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("tungsten-dql-{label}-{pid}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &std::path::Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(&path, body).unwrap();
    }

    fn make_vault() -> std::path::PathBuf {
        let dir = unique_vault("v");
        write(
            &dir,
            "Rust intro.md",
            "---\ntags: [rust, lang]\nstatus: draft\n---\n# Rust intro\nbody\n",
        );
        write(
            &dir,
            "Python intro.md",
            "---\ntags: [python, lang]\nstatus: published\n---\n# Python intro\nbody\n",
        );
        write(
            &dir,
            "Journal/2026-07-08.md",
            "---\ntags: [journal]\nmood: 7\n---\n# Wed\nbody\n",
        );
        dir
    }

    // ---- Lexer ----

    #[test]
    fn tokenize_keywords() {
        let t = tokenize("LIST FROM #tag WHERE x = 1 SORT y ASC LIMIT 5").unwrap();
        let kinds: Vec<String> = t
            .iter()
            .map(|t| match t {
                Token::Keyword(k) => format!("KW({:?})", k),
                Token::Ident(s) => format!("ID({s})"),
                Token::Tag(s) => format!("TAG({s})"),
                Token::Number(n) => format!("NUM({n})"),
                Token::CmpOp(op) => format!("CMP({:?})", op),
                _ => format!("{t:?}"),
            })
            .collect();
        assert!(kinds[0].contains("List"));
        assert!(kinds.iter().any(|k| k.contains("From")));
        assert!(kinds.iter().any(|k| k.contains("Where")));
        assert!(kinds.iter().any(|k| k.contains("Sort")));
        assert!(kinds.iter().any(|k| k.contains("Asc")));
        assert!(kinds.iter().any(|k| k.contains("Limit")));
    }

    #[test]
    fn tokenize_string() {
        let t = tokenize(r#"FROM "Journal" "#).unwrap();
        match &t[1] {
            Token::String(s) => assert_eq!(s, "Journal"),
            _ => panic!(),
        }
    }

    #[test]
    fn tokenize_tag() {
        let t = tokenize("FROM #rust").unwrap();
        match &t[1] {
            Token::Tag(s) => assert_eq!(s, "rust"),
            _ => panic!(),
        }
    }

    // ---- Parser ----

    #[test]
    fn parse_list_simple() {
        let q = parse_query("LIST").unwrap();
        assert_eq!(q.source, SourceType::List);
        assert!(q.from.is_none());
        assert!(q.r#where.is_none());
    }

    #[test]
    fn parse_table_with_fields() {
        let q = parse_query("TABLE file.name, file.tags, status FROM #rust").unwrap();
        match q.source {
            SourceType::Table(fields) => {
                assert_eq!(fields, vec!["file.name", "file.tags", "status"]);
            }
            _ => panic!(),
        }
        assert_eq!(q.from, Some(FromClause::Tag("rust".into())));
    }

    #[test]
    fn parse_where_contains() {
        let q = parse_query("LIST WHERE CONTAINS(file.tags, \"rust\")").unwrap();
        let w = q.r#where.unwrap();
        match w.root {
            WhereNode::Contains(ident, lit) => {
                assert_eq!(ident.0, "file.tags");
                assert_eq!(lit.as_string(), Some("rust"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_where_compare() {
        let q = parse_query("LIST WHERE status = \"draft\"").unwrap();
        let w = q.r#where.unwrap();
        match w.root {
            WhereNode::Compare(ident, op, lit) => {
                assert_eq!(ident.0, "status");
                assert_eq!(op, CompareOp::Eq);
                assert_eq!(lit.as_string(), Some("draft"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_sort_with_direction() {
        let q = parse_query("LIST SORT file.mtime DESC").unwrap();
        let s = q.sort.unwrap();
        assert_eq!(s.field.0, "file.mtime");
        assert!(s.descending);
    }

    #[test]
    fn parse_limit() {
        let q = parse_query("LIST LIMIT 10").unwrap();
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn parse_full_query() {
        let q = parse_query(
            r#"TABLE file.name, status FROM #lang WHERE status = "draft" SORT file.name ASC LIMIT 5"#,
        )
        .unwrap();
        assert!(matches!(q.source, SourceType::Table(_)));
        assert_eq!(q.from, Some(FromClause::Tag("lang".into())));
        assert!(q.r#where.is_some());
        assert!(q.sort.is_some());
        assert_eq!(q.limit, Some(5));
    }

    // ---- Executor ----

    #[test]
    fn execute_list_all() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query("LIST").unwrap();
        let r = execute(&q, &idx);
        assert_eq!(r.len(), 3);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_list_from_tag() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query("LIST FROM #lang").unwrap();
        let r = execute(&q, &idx);
        assert_eq!(r.len(), 2);
        for row in &r.rows {
            let n = row.note();
            assert!(n.tags.contains(&"lang".to_string()));
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_list_from_folder() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query("LIST FROM \"Journal\"").unwrap();
        let r = execute(&q, &idx);
        assert_eq!(r.len(), 1);
        let n = r.rows[0].note();
        assert!(n.path.to_string_lossy().contains("Journal"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_where_contains() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query(r#"LIST WHERE CONTAINS(file.tags, "rust")"#).unwrap();
        let r = execute(&q, &idx);
        assert_eq!(r.len(), 1);
        let n = r.rows[0].note();
        assert_eq!(n.title, "Rust intro");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_where_compare_yaml() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query(r#"LIST WHERE status = "draft""#).unwrap();
        let r = execute(&q, &idx);
        assert_eq!(r.len(), 1);
        let n = r.rows[0].note();
        assert_eq!(n.title, "Rust intro");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_sort_ascending() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query("LIST SORT file.name ASC").unwrap();
        let r = execute(&q, &idx);
        let names: Vec<String> = r
            .rows
            .iter()
            .map(|row| {
                row.note()
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_sort_descending() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query("LIST SORT file.name DESC").unwrap();
        let r = execute(&q, &idx);
        let names: Vec<String> = r
            .rows
            .iter()
            .map(|row| {
                row.note()
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.reverse();
        assert_eq!(names, sorted);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_limit() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query("LIST LIMIT 2").unwrap();
        let r = execute(&q, &idx);
        assert_eq!(r.len(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_table_renders_fields() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query("TABLE file.name, status FROM #lang").unwrap();
        let r = execute(&q, &idx);
        assert_eq!(r.len(), 2);
        for row in &r.rows {
            let n = row.note();
            let values = match row {
                DqlRow::Table(_, v) => v.clone(),
                _ => panic!("expected table row"),
            };
            assert_eq!(values.len(), 2);
            assert_eq!(values[0], n.path.file_name().unwrap().to_string_lossy().to_string());
            // values[1] is the status (rust=draft, python=published)
            assert!(!values[1].is_empty());
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execute_combined_query() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        let q = parse_query(
            r#"TABLE file.name, status FROM #lang WHERE status != "published" SORT file.name LIMIT 10"#,
        )
        .unwrap();
        let r = execute(&q, &idx);
        // Only "Rust intro" is draft (not published). Python is
        // published so excluded.
        assert_eq!(r.len(), 1);
        let n = r.rows[0].note();
        assert_eq!(n.title, "Rust intro");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_field_yaml_number() {
        let dir = make_vault();
        let idx = NoteIndex::build(&dir).unwrap();
        // by_title keys on the H1 ("Wed"); iterate notes() to
        // find the one with the "mood" frontmatter key.
        let n = idx
            .notes()
            .find(|n| n.frontmatter.as_mapping().is_some_and(|m| m.contains_key("mood")))
            .unwrap();
        assert_eq!(render_field(n, "mood"), "7");
        fs::remove_dir_all(&dir).ok();
    }
}
