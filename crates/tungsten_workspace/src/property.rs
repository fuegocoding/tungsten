//! Property (frontmatter) types and per-vault schemas.
//!
//! Obsidian 1.4+ stores user-defined property *types* in
//! `.obsidian/types.json`. The file is a JSON map from property
//! name (e.g. `"status"`) to a YAML string describing the type:
//!
//! ```json
//! {
//!   "status": "list",
//!   "status__options": "[\"draft\", \"published\"]",
//!   "tags": "multitext",
//!   "created": "date"
//! }
//! ```
//!
//! Tungsten reads these declarations so the Properties panel can
//! render the right editor for each field, and so a note's
//! frontmatter can be validated against the schema.
//!
//! The schema here is intentionally permissive: a property with no
//! declaration defaults to `text`, missing `__options` is treated as
//! free-form, and unknown type names are accepted as `text` rather
//! than rejected. This matches Obsidian's own behavior of never
//! refusing to load a vault because of a bad type entry.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::note::Note;

/// Supported property value kinds.
///
/// The list mirrors Obsidian's first-class types plus a few
/// Tungsten-specific extensions (`link` for wikilink fields,
/// `number` for numeric values that Obsidian calls `number`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyKind {
    /// Single-line free-form text.
    Text,
    /// Multi-line free-form text.
    Multitext,
    /// Single-select from a fixed list of string options.
    List,
    /// Multi-select from a fixed list of string options.
    MultiList,
    /// ISO-8601 date (`YYYY-MM-DD`).
    Date,
    /// ISO-8601 datetime (`YYYY-MM-DDTHH:MM`).
    Datetime,
    /// Boolean toggle.
    Checkbox,
    /// Floating-point number.
    Number,
    /// Wikilink target (string form `[[Target]]` or just `Target`).
    Link,
}

impl PropertyKind {
    /// Parse the kind from the lowercase string Obsidian writes.
    /// Unknown kinds fall back to [`PropertyKind::Text`] so the
    /// vault still opens.
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "text" => Self::Text,
            "multitext" | "multi-text" | "longtext" => Self::Multitext,
            "list" | "select" => Self::List,
            "multilist" | "multi-list" | "tags" | "multiselect" => Self::MultiList,
            "date" => Self::Date,
            "datetime" | "date-time" => Self::Datetime,
            "checkbox" | "boolean" | "bool" => Self::Checkbox,
            "number" | "num" | "float" => Self::Number,
            "link" | "wikilink" | "wiki" => Self::Link,
            _ => Self::Text,
        }
    }

    /// Human-readable name for the kind (used in error messages
    /// and the UI).
    pub fn label(&self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Multitext => "Multi-line text",
            Self::List => "List",
            Self::MultiList => "Multi-list",
            Self::Date => "Date",
            Self::Datetime => "Date & time",
            Self::Checkbox => "Checkbox",
            Self::Number => "Number",
            Self::Link => "Link",
        }
    }
}

/// A single property declaration: name, kind, and (for list kinds)
/// the allowed options plus an optional default value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertySchema {
    pub name: String,
    pub kind: PropertyKind,
    /// Allowed values for [`PropertyKind::List`] /
    /// [`PropertyKind::MultiList`]. Empty for other kinds.
    pub options: Vec<String>,
    /// Default value(s) if the field is missing from frontmatter.
    pub default: Option<String>,
}

impl PropertySchema {
    /// Build a schema entry from the kind string and an optional
    /// `__options` companion (Obsidian writes the options for
    /// `list` fields in a sibling key suffixed with `__options`).
    pub fn from_kind(name: &str, kind: &str, options: &[String]) -> Self {
        let kind = PropertyKind::from_str(kind);
        let options = match kind {
            PropertyKind::List | PropertyKind::MultiList => options.to_vec(),
            _ => Vec::new(),
        };
        Self {
            name: name.to_string(),
            kind,
            options,
            default: None,
        }
    }
}

/// A whole-vault property schema: all declared properties plus
/// convenience accessors.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PropertySchemaSet {
    schemas: BTreeMap<String, PropertySchema>,
}

impl PropertySchemaSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a schema entry. Overwrites any existing entry for
    /// `schema.name`.
    pub fn insert(&mut self, schema: PropertySchema) {
        self.schemas.insert(schema.name.clone(), schema);
    }

    /// Look up a schema by property name.
    pub fn get(&self, name: &str) -> Option<&PropertySchema> {
        self.schemas.get(name)
    }

    /// All declared property names, in declaration order.
    pub fn names(&self) -> Vec<String> {
        self.schemas.keys().cloned().collect()
    }

    /// True if `name` is declared (and therefore should be
    /// validated).
    pub fn contains(&self, name: &str) -> bool {
        self.schemas.contains_key(name)
    }

    /// All schemas.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &PropertySchema)> {
        self.schemas.iter().map(|(k, v)| (k.as_str(), v))
    }
}

/// Parse `.obsidian/types.json` into a [`PropertySchemaSet`].
///
/// The format is a flat map of `name -> "kind"` strings, with
/// companion `name__options -> "[\"a\",\"b\"]"` JSON arrays. Tungsten
/// also accepts a richer shape:
///
/// ```json
/// { "status": { "type": "list", "options": ["a","b"] } }
/// ```
///
/// which is what the schema editor produces once it exists.
pub fn parse_types_json(content: &str) -> Result<PropertySchemaSet, serde_json::Error> {
    let raw: serde_json::Value = serde_json::from_str(content)?;
    let mut set = PropertySchemaSet::new();
    let Some(obj) = raw.as_object() else {
        return Ok(set);
    };

    for (key, value) in obj {
        if let Some(kind_str) = value.as_str() {
            // Flat form: name -> "list"
            // Options live in a companion key suffixed with
            // `__options`. Obsidian writes them as a *string*
            // containing a JSON array, so we may need to parse
            // twice.
            let options_key = format!("{key}__options");
            let options = obj
                .get(&options_key)
                .map(|v| parse_options(v))
                .unwrap_or_default();
            set.insert(PropertySchema::from_kind(key, kind_str, &options));
        } else if let Some(obj) = value.as_object() {
            // Rich form: name -> { type, options, default }
            let kind_str = obj
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("text");
            let options = obj
                .get("options")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut schema = PropertySchema::from_kind(key, kind_str, &options);
            schema.default = obj
                .get("default")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            set.insert(schema);
        }
    }
    Ok(set)
}

fn parse_options(v: &serde_json::Value) -> Vec<String> {
    if let Some(arr) = v.as_array() {
        return arr
            .iter()
            .filter_map(|s| s.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(s) = v.as_str() {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
            if let Some(arr) = parsed.as_array() {
                return arr
                    .iter()
                    .filter_map(|x| x.as_str().map(|x| x.to_string()))
                    .collect();
            }
        }
    }
    Vec::new()
}

/// A single validation problem with a note's frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyError {
    pub property: String,
    pub message: String,
}

impl std::fmt::Display for PropertyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.property, self.message)
    }
}

impl std::error::Error for PropertyError {}

/// Validate a [`Note`]'s frontmatter against a [`PropertySchemaSet`].
///
/// Returns an empty `Vec` if the note is valid. Unknown property
/// names in the note are *not* errors — they pass through
/// untouched — but known properties with wrong-typed values are.
pub fn validate(note: &Note, schema: &PropertySchemaSet) -> Vec<PropertyError> {
    let mut errors = Vec::new();
    let map = match note.frontmatter.as_mapping() {
        Some(m) => m,
        None => return errors,
    };
    for (key, value) in map {
        let key_str = match key.as_str() {
            Some(s) => s,
            None => continue,
        };
        let Some(decl) = schema.get(key_str) else {
            continue;
        };
        if let Some(err) = validate_value(key_str, value, decl) {
            errors.push(err);
        }
    }
    errors
}

fn validate_value(
    name: &str,
    value: &serde_yaml::Value,
    schema: &PropertySchema,
) -> Option<PropertyError> {
    match schema.kind {
        PropertyKind::Text => match value {
            serde_yaml::Value::String(_) | serde_yaml::Value::Null => None,
            other => Some(PropertyError {
                property: name.to_string(),
                message: format!("expected text, got {}", yaml_kind(other)),
            }),
        },
        PropertyKind::Multitext => match value {
            serde_yaml::Value::String(_) | serde_yaml::Value::Null => None,
            other => Some(PropertyError {
                property: name.to_string(),
                message: format!("expected multi-line text, got {}", yaml_kind(other)),
            }),
        },
        PropertyKind::List => match value {
            serde_yaml::Value::String(s) => {
                if schema.options.is_empty() || schema.options.iter().any(|o| o == s) {
                    None
                } else {
                    Some(PropertyError {
                        property: name.to_string(),
                        message: format!(
                            "value {:?} not in options {:?}",
                            s, schema.options
                        ),
                    })
                }
            }
            serde_yaml::Value::Null => None,
            other => Some(PropertyError {
                property: name.to_string(),
                message: format!("expected list option, got {}", yaml_kind(other)),
            }),
        },
        PropertyKind::MultiList => match value {
            serde_yaml::Value::Sequence(seq) => {
                for item in seq {
                    let Some(s) = item.as_str() else {
                        return Some(PropertyError {
                            property: name.to_string(),
                            message: "multi-list entries must be strings".into(),
                        });
                    };
                    if !schema.options.is_empty()
                        && !schema.options.iter().any(|o| o == s)
                    {
                        return Some(PropertyError {
                            property: name.to_string(),
                            message: format!(
                                "value {:?} not in options {:?}",
                                s, schema.options
                            ),
                        });
                    }
                }
                None
            }
            serde_yaml::Value::Null => None,
            other => Some(PropertyError {
                property: name.to_string(),
                message: format!("expected multi-list, got {}", yaml_kind(other)),
            }),
        },
        PropertyKind::Date => match value {
            serde_yaml::Value::String(s) => {
                if is_iso_date(s) {
                    None
                } else {
                    Some(PropertyError {
                        property: name.to_string(),
                        message: format!("expected YYYY-MM-DD date, got {:?}", s),
                    })
                }
            }
            serde_yaml::Value::Null => None,
            other => Some(PropertyError {
                property: name.to_string(),
                message: format!("expected date, got {}", yaml_kind(other)),
            }),
        },
        PropertyKind::Datetime => match value {
            serde_yaml::Value::String(s) => {
                if is_iso_datetime(s) {
                    None
                } else {
                    Some(PropertyError {
                        property: name.to_string(),
                        message: format!("expected ISO-8601 datetime, got {:?}", s),
                    })
                }
            }
            serde_yaml::Value::Null => None,
            other => Some(PropertyError {
                property: name.to_string(),
                message: format!("expected datetime, got {}", yaml_kind(other)),
            }),
        },
        PropertyKind::Checkbox => match value {
            serde_yaml::Value::Bool(_) => None,
            serde_yaml::Value::Null => None,
            other => Some(PropertyError {
                property: name.to_string(),
                message: format!("expected boolean, got {}", yaml_kind(other)),
            }),
        },
        PropertyKind::Number => match value {
            serde_yaml::Value::Number(_) => None,
            serde_yaml::Value::Null => None,
            other => Some(PropertyError {
                property: name.to_string(),
                message: format!("expected number, got {}", yaml_kind(other)),
            }),
        },
        PropertyKind::Link => match value {
            serde_yaml::Value::String(_) => None,
            serde_yaml::Value::Sequence(_) => None,
            serde_yaml::Value::Null => None,
            other => Some(PropertyError {
                property: name.to_string(),
                message: format!("expected link, got {}", yaml_kind(other)),
            }),
        },
    }
}

fn yaml_kind(v: &serde_yaml::Value) -> &'static str {
    match v {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Bool(_) => "boolean",
        serde_yaml::Value::Number(_) => "number",
        serde_yaml::Value::String(_) => "string",
        serde_yaml::Value::Sequence(_) => "sequence",
        serde_yaml::Value::Mapping(_) => "mapping",
        serde_yaml::Value::Tagged(_) => "tagged",
    }
}

fn is_iso_date(s: &str) -> bool {
    // YYYY-MM-DD
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(|c| c.is_ascii_digit())
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[8..10].iter().all(|c| c.is_ascii_digit())
}

fn is_iso_datetime(s: &str) -> bool {
    // Accept YYYY-MM-DD or YYYY-MM-DDTHH:MM or YYYY-MM-DDTHH:MM:SS
    // or with a trailing Z/offset.
    if is_iso_date(s) {
        return true;
    }
    let (date, rest) = match s.find('T') {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => match s.find(' ') {
            Some(i) => (&s[..i], &s[i + 1..]),
            None => return false,
        },
    };
    if !is_iso_date(date) {
        return false;
    }
    let time = rest.split(|c| c == 'Z' || c == '+' || c == '-').next().unwrap_or("");
    let parts: Vec<&str> = time.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return false;
    }
    parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_parser::parse;
    use std::path::PathBuf;

    fn note_with(content: &str) -> Note {
        let parsed = parse(content);
        Note {
            path: PathBuf::from("/v/note.md"),
            title: parsed.title.unwrap_or_else(|| "Untitled".into()),
            content: content.to_string(),
            frontmatter: parsed.frontmatter,
            links: parsed.links,
            tags: parsed.tags,
            callouts: parsed.callouts,
            mtime: None,
            size_bytes: 0,
        }
    }

    #[test]
    fn kind_parsing_is_case_insensitive() {
        assert_eq!(PropertyKind::from_str("Text"), PropertyKind::Text);
        assert_eq!(PropertyKind::from_str("LIST"), PropertyKind::List);
        assert_eq!(PropertyKind::from_str("MultiText"), PropertyKind::Multitext);
        assert_eq!(PropertyKind::from_str("unknown_thing"), PropertyKind::Text);
    }

    #[test]
    fn parse_flat_types_json() {
        let s = r#"{
            "status": "list",
            "status__options": "[\"draft\", \"published\"]",
            "created": "date",
            "tags": "multilist"
        }"#;
        let set = parse_types_json(s).unwrap();
        assert_eq!(set.get("status").unwrap().kind, PropertyKind::List);
        assert_eq!(
            set.get("status").unwrap().options,
            vec!["draft", "published"]
        );
        assert_eq!(set.get("created").unwrap().kind, PropertyKind::Date);
        assert_eq!(set.get("tags").unwrap().kind, PropertyKind::MultiList);
    }

    #[test]
    fn parse_rich_types_json() {
        let s = r#"{
            "status": { "type": "list", "options": ["draft", "published"] }
        }"#;
        let set = parse_types_json(s).unwrap();
        assert_eq!(set.get("status").unwrap().kind, PropertyKind::List);
        assert_eq!(
            set.get("status").unwrap().options,
            vec!["draft", "published"]
        );
    }

    #[test]
    fn parse_empty_types_json() {
        let s = "{}";
        let set = parse_types_json(s).unwrap();
        assert!(set.names().is_empty());
    }

    #[test]
    fn validate_list_accepts_known_option() {
        let mut set = PropertySchemaSet::new();
        set.insert(PropertySchema::from_kind(
            "status",
            "list",
            &["draft".into(), "published".into()],
        ));
        let n = note_with("---\nstatus: draft\n---\nbody");
        assert!(validate(&n, &set).is_empty());
    }

    #[test]
    fn validate_list_rejects_unknown_option() {
        let mut set = PropertySchemaSet::new();
        set.insert(PropertySchema::from_kind(
            "status",
            "list",
            &["draft".into()],
        ));
        let n = note_with("---\nstatus: published\n---\nbody");
        let errs = validate(&n, &set);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].property, "status");
    }

    #[test]
    fn validate_date_accepts_iso() {
        let mut set = PropertySchemaSet::new();
        set.insert(PropertySchema::from_kind("created", "date", &[]));
        let n = note_with("---\ncreated: 2026-07-14\n---\nbody");
        assert!(validate(&n, &set).is_empty());
    }

    #[test]
    fn validate_date_rejects_garbage() {
        let mut set = PropertySchemaSet::new();
        set.insert(PropertySchema::from_kind("created", "date", &[]));
        let n = note_with("---\ncreated: yesterday\n---\nbody");
        let errs = validate(&n, &set);
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn validate_checkbox() {
        let mut set = PropertySchemaSet::new();
        set.insert(PropertySchema::from_kind("done", "checkbox", &[]));
        let n = note_with("---\ndone: true\n---\nbody");
        assert!(validate(&n, &set).is_empty());
        let n2 = note_with("---\ndone: yes\n---\nbody");
        assert_eq!(validate(&n2, &set).len(), 1);
    }

    #[test]
    fn validate_number() {
        let mut set = PropertySchemaSet::new();
        set.insert(PropertySchema::from_kind("count", "number", &[]));
        let n = note_with("---\ncount: 42\n---\nbody");
        assert!(validate(&n, &set).is_empty());
        let n2 = note_with("---\ncount: many\n---\nbody");
        assert_eq!(validate(&n2, &set).len(), 1);
    }

    #[test]
    fn validate_ignores_unknown_properties() {
        let set = PropertySchemaSet::new();
        let n = note_with("---\nrandom_field: hello\n---\nbody");
        assert!(validate(&n, &set).is_empty());
    }

    #[test]
    fn validate_multilist_sequence() {
        let mut set = PropertySchemaSet::new();
        set.insert(PropertySchema::from_kind(
            "tags",
            "multilist",
            &["red".into(), "blue".into()],
        ));
        let n = note_with("---\ntags:\n  - red\n  - blue\n---\nbody");
        assert!(validate(&n, &set).is_empty());
        let n2 = note_with("---\ntags:\n  - red\n  - green\n---\nbody");
        assert_eq!(validate(&n2, &set).len(), 1);
    }

    #[test]
    fn iso_date_recognizes_valid() {
        assert!(is_iso_date("2026-07-14"));
        assert!(!is_iso_date("2026-7-14"));
        assert!(!is_iso_date("20260714"));
        assert!(!is_iso_date("yesterday"));
    }

    #[test]
    fn iso_datetime_recognizes_valid() {
        assert!(is_iso_datetime("2026-07-14"));
        assert!(is_iso_datetime("2026-07-14T12:30"));
        assert!(is_iso_datetime("2026-07-14T12:30:45"));
        assert!(is_iso_datetime("2026-07-14T12:30:45Z"));
        assert!(!is_iso_datetime("not-a-date"));
    }
}
