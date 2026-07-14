//! Daily journal subsystem (M2.2).
//!
//! A *journal* is a folder of date-named notes — typically
//! `Journal/2026-07-14.md` — written one per day. Tungsten
//! extends [`NoteCreator::create_daily`] (which creates a
//! single day's note) with:
//!
//! - A configurable journal folder and filename template
//! - A registry of named *widgets* (mood, weather, gratitude,
//!   …) that can be mixed into the daily-note template
//! - A [`journal_home`] helper that auto-generates a home
//!   note with links to recent entries
//! - A [`calendar`] helper that returns a map of
//!   `date -> note_path` for the last N days
//! - Mood tracking via a frontmatter convention
//!
//! The journal is intentionally data-only: there is no
//! rendering here, just the structures a UI panel would
//! consume.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Local, NaiveDate};

use crate::index::NoteIndex;
use crate::note::Note;
use crate::notes_io::NoteCreator;

/// Configuration for the journal subsystem.
///
/// Defaults to `Journal/` in the vault root, one note per
/// day named `YYYY-MM-DD.md`. Both can be overridden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalConfig {
    /// Folder under the vault root where daily notes live.
    pub folder: String,
    /// Filename template. The default is `"{date}.md"`; the
    /// only substitution is `{date}` which becomes the ISO
    /// date. Path separators are not allowed.
    pub filename_template: String,
    /// Title for the auto-generated home note.
    pub home_title: String,
    /// Number of recent entries to show on the home note.
    pub home_recent: usize,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            folder: "Journal".into(),
            filename_template: "{date}.md".into(),
            home_title: "Journal".into(),
            home_recent: 30,
        }
    }
}

/// A single registered widget for the daily-note template.
///
/// Widgets are snippets of Markdown that get inlined into
/// the body of a daily note when it's created. They are
/// configurable so the user can pick which widgets they want
/// and in what order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widget {
    pub id: String,
    pub title: String,
    pub body: String,
}

impl Widget {
    pub fn new(id: impl Into<String>, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: body.into(),
        }
    }
}

/// A registry of widgets, ordered by user preference.
///
/// Tungsten ships with twelve widgets that Obsidian's daily
/// notes plugin popularized; users can enable, reorder, and
/// edit them in Settings.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WidgetRegistry {
    widgets: Vec<Widget>,
}

impl WidgetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a widget. The most recent registration wins on
    /// duplicate `id`.
    pub fn register(&mut self, widget: Widget) {
        self.widgets.retain(|w| w.id != widget.id);
        self.widgets.push(widget);
    }

    /// Reorder widgets. The new order is by `ids`; any ids
    /// not in the registry are appended at the end in the
    /// order given.
    pub fn reorder(&mut self, ids: &[String]) {
        let by_id: std::collections::HashMap<_, _> = self
            .widgets
            .iter()
            .map(|w| (w.id.clone(), w.clone()))
            .collect();
        let mut new_order: Vec<Widget> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            if let Some(w) = by_id.get(id) {
                if seen.insert(id) {
                    new_order.push(w.clone());
                }
            }
        }
        for w in &self.widgets {
            if seen.insert(&w.id) {
                new_order.push(w.clone());
            }
        }
        self.widgets = new_order;
    }

    pub fn iter(&self) -> impl Iterator<Item = &Widget> {
        self.widgets.iter()
    }

    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }
}

/// The standard twelve widgets Tungsten ships with.
///
/// These mirror the popular Obsidian Daily Notes plugin's
/// defaults. The body uses YAML frontmatter hints for
/// `mood` and `weather` so the mood/weather trend panels
/// can read them back without parsing freeform text.
pub fn default_widgets() -> WidgetRegistry {
    let mut r = WidgetRegistry::new();
    r.register(Widget::new(
        "mood",
        "Mood",
        "## Mood\n\nmood: \nenergy: \n",
    ));
    r.register(Widget::new(
        "weather",
        "Weather",
        "## Weather\n\nweather: \ntemp: \n",
    ));
    r.register(Widget::new(
        "gratitude",
        "Gratitude",
        "## Gratitude\n\n1. \n2. \n3. \n",
    ));
    r.register(Widget::new(
        "goals",
        "Today's Goals",
        "## Today's Goals\n\n- [ ] \n- [ ] \n- [ ] \n",
    ));
    r.register(Widget::new(
        "notes",
        "Notes",
        "## Notes\n\n",
    ));
    r.register(Widget::new(
        "ideas",
        "Ideas",
        "## Ideas\n\n",
    ));
    r.register(Widget::new(
        "reading",
        "Reading",
        "## Reading\n\n",
    ));
    r.register(Widget::new(
        "meals",
        "Meals",
        "## Meals\n\n- \n- \n- \n",
    ));
    r.register(Widget::new(
        "exercise",
        "Exercise",
        "## Exercise\n\n",
    ));
    r.register(Widget::new(
        "sleep",
        "Sleep",
        "## Sleep\n\nhours: \nquality: \n",
    ));
    r.register(Widget::new(
        "tomorrow",
        "Tomorrow",
        "## Tomorrow\n\n",
    ));
    r.register(Widget::new(
        "reflection",
        "Reflection",
        "## Reflection\n\n",
    ));
    r
}

/// Render the body of a daily note by stitching the enabled
/// widgets together with a heading for the date.
pub fn render_daily_body(date: NaiveDate, registry: &WidgetRegistry) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", date.format("%A, %B %-d, %Y")));
    for w in registry.iter() {
        out.push_str(&w.body);
        out.push('\n');
    }
    out
}

/// Compute the path of a daily note given a journal
/// configuration and date.
pub fn daily_note_path(
    vault_root: &Path,
    config: &JournalConfig,
    date: NaiveDate,
) -> PathBuf {
    let folder = vault_root.join(&config.folder);
    let date_str = date.format("%Y-%m-%d").to_string();
    let name = config.filename_template.replace("{date}", &date_str);
    folder.join(name)
}

/// Create today's daily note.
///
/// If the note already exists, it is returned unchanged
/// (the function never overwrites). Otherwise it is created
/// with the configured widgets rendered into its body.
pub fn create_today(
    vault_root: &Path,
    creator: &mut NoteCreator,
    config: &JournalConfig,
    registry: &WidgetRegistry,
) -> Result<Note, crate::notes_io::NoteCreateError> {
    let today = Local::now().date_naive();
    create_for_date(vault_root, creator, config, registry, today)
}

/// Create the daily note for a specific date.
pub fn create_for_date(
    vault_root: &Path,
    creator: &mut NoteCreator,
    config: &JournalConfig,
    registry: &WidgetRegistry,
    date: NaiveDate,
) -> Result<Note, crate::notes_io::NoteCreateError> {
    let path = daily_note_path(vault_root, config, date);
    if path.exists() {
        return creator.read(&path);
    }
    let body = render_daily_body(date, registry);
    creator.create_with_path(
        &path,
        &date.format("%A, %B %-d, %Y").to_string(),
        &body,
    )
}

/// Calendar view: which dates in `[start, end]` have a
/// journal note under `config.folder`?
///
/// The result is a `BTreeMap<date, note_path>` so callers
/// can iterate in chronological order and miss dates that
/// have no entry.
pub fn calendar(
    index: &NoteIndex,
    config: &JournalConfig,
    start: NaiveDate,
    end: NaiveDate,
) -> BTreeMap<NaiveDate, PathBuf> {
    let folder_name = config.folder.clone();
    let mut out = BTreeMap::new();
    for note in index.notes() {
        let parent_name = note
            .path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        if parent_name != Some(folder_name.as_str()) {
            continue;
        }
        let Some(stem) = note.path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Ok(date) = NaiveDate::parse_from_str(stem, "%Y-%m-%d") {
            if date >= start && date <= end {
                out.insert(date, note.path.clone());
            }
        }
    }
    out
}

/// One row of the mood-trend view: a date and the recorded
/// mood value (0–10) and energy value (0–10), if present.
#[derive(Debug, Clone, PartialEq)]
pub struct MoodRow {
    pub date: NaiveDate,
    pub mood: Option<u8>,
    pub energy: Option<u8>,
}

impl MoodRow {
    pub fn is_complete(&self) -> bool {
        self.mood.is_some() && self.energy.is_some()
    }
}

/// Read the mood trend from the journal: parse the `mood:`
/// and `energy:` lines out of every daily note in the
/// journal folder. Values must be digits in 0..=10; anything
/// else is treated as missing.
pub fn mood_trend(index: &NoteIndex, config: &JournalConfig) -> Vec<MoodRow> {
    let mut out: Vec<MoodRow> = Vec::new();
    for note in index.notes() {
        let Some(parent) = note.path.parent() else {
            continue;
        };
        if parent
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == config.folder)
            != Some(true)
        {
            continue;
        }
        let Some(stem) = note.path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(date) = NaiveDate::parse_from_str(stem, "%Y-%m-%d") else {
            continue;
        };
        let mood = extract_int_field(&note.content, "mood");
        let energy = extract_int_field(&note.content, "energy");
        out.push(MoodRow {
            date,
            mood,
            energy,
        });
    }
    out.sort_by_key(|r| r.date);
    out
}

fn extract_int_field(content: &str, field: &str) -> Option<u8> {
    for line in content.lines() {
        let line = line.trim();
        let prefix = format!("{field}:");
        if let Some(rest) = line.strip_prefix(&prefix) {
            let rest = rest.trim();
            if let Ok(n) = rest.parse::<u8>() {
                if n <= 10 {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Render the home-note Markdown: a list of links to recent
/// daily notes plus a count of total entries.
pub fn journal_home(
    index: &NoteIndex,
    config: &JournalConfig,
    today: NaiveDate,
) -> String {
    let start = today - chrono::Duration::days(config.home_recent as i64);
    let cal = calendar(index, config, start, today);
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", config.home_title));
    out.push_str(&format!(
        "Total entries: **{}**\n\n",
        cal.len()
    ));
    out.push_str("## Recent\n\n");
    let mut entries: Vec<(NaiveDate, PathBuf)> = cal.into_iter().collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    for (date, path) in entries.into_iter().take(config.home_recent) {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        out.push_str(&format!("- [[{}]] ({})\n", name, date.format("%Y-%m-%d")));
    }
    out.push_str(&format!(
        "\n*Last generated: {}*\n",
        today.format("%Y-%m-%d")
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-journal-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let p = base.join(unique);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn default_widgets_count_is_twelve() {
        let r = default_widgets();
        assert_eq!(r.len(), 12);
    }

    #[test]
    fn default_widgets_have_unique_ids() {
        let r = default_widgets();
        let mut seen = std::collections::HashSet::new();
        for w in r.iter() {
            assert!(seen.insert(w.id.clone()), "duplicate id: {}", w.id);
        }
    }

    #[test]
    fn render_daily_body_includes_date_heading() {
        let r = default_widgets();
        let body = render_daily_body(
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap(),
            &r,
        );
        assert!(body.contains("# Tuesday"));
        assert!(body.contains("Mood"));
    }

    #[test]
    fn daily_note_path_uses_template() {
        let config = JournalConfig::default();
        let date = NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        let path = daily_note_path(Path::new("/vault"), &config, date);
        assert_eq!(
            path,
            std::path::PathBuf::from("/vault/Journal/2026-07-14.md")
        );
    }

    #[test]
    fn calendar_finds_dated_notes() {
        let dir = tempdir();
        let j = dir.join("Journal");
        fs::create_dir_all(&j).unwrap();
        fs::write(j.join("2026-07-13.md"), "# Mon\n").unwrap();
        fs::write(j.join("2026-07-14.md"), "# Tue\n").unwrap();
        fs::write(j.join("not-a-date.md"), "x").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let cal = calendar(
            &index,
            &JournalConfig::default(),
            NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 7, 31).unwrap(),
        );
        assert_eq!(cal.len(), 2);
        assert!(cal.contains_key(&NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mood_trend_parses_mood_and_energy() {
        let dir = tempdir();
        let j = dir.join("Journal");
        fs::create_dir_all(&j).unwrap();
        fs::write(
            j.join("2026-07-13.md"),
            "## Mood\n\nmood: 7\nenergy: 5\n",
        )
        .unwrap();
        fs::write(
            j.join("2026-07-14.md"),
            "## Mood\n\nmood: 8\nenergy: 6\n",
        )
        .unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let trend = mood_trend(&index, &JournalConfig::default());
        assert_eq!(trend.len(), 2);
        assert_eq!(trend[0].mood, Some(7));
        assert_eq!(trend[1].mood, Some(8));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn journal_home_lists_recent() {
        let dir = tempdir();
        let j = dir.join("Journal");
        fs::create_dir_all(&j).unwrap();
        for d in 1..=5 {
            let date = NaiveDate::from_ymd_opt(2026, 7, d).unwrap();
            fs::write(
                j.join(format!("{}.md", date.format("%Y-%m-%d"))),
                format!("# {}\n", date),
            )
            .unwrap();
        }
        let index = NoteIndex::build(&dir).unwrap();
        let home = journal_home(
            &index,
            &JournalConfig::default(),
            NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
        );
        assert!(home.contains("# Journal"));
        assert!(home.contains("Total entries: **5**"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reorder_changes_widget_order() {
        let mut r = default_widgets();
        let ids: Vec<String> =
            vec!["weather", "mood", "goals"].into_iter().map(String::from).collect();
        r.reorder(&ids);
        let order: Vec<&str> = r.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(order[0], "weather");
        assert_eq!(order[1], "mood");
        assert_eq!(order[2], "goals");
    }
}
