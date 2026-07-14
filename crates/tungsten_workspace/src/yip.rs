//! Year-in-Pixels (YIP) calendar heatmap.
//!
//! A YIP is a 365-cell grid (one per day) where each cell's
//! color encodes the mood (or another scalar) recorded for
//! that day. The Tungsten daily-note convention is to keep
//! `mood: <0..=10>` and `energy: <0..=10>` in the journal
//! entry; this module reads those values and produces a
//! data structure that's easy to render.
//!
//! The output here is the *data* the renderer needs:
//!
//! - [`YipGrid`] — the year-wide data with per-day mood
//! - [`YipSvg`] — a self-contained SVG string the view
//!   layer can drop straight into a `div` or a `Html`
//!   preview
//!
//! The SVG is intentionally tiny and dependency-free: just
//! a series of `<rect>` elements arranged in a 12-row
//! grid, one row per month, plus a legend.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{Datelike, NaiveDate};

use crate::index::NoteIndex;
use crate::journal::JournalConfig;

/// One day's YIP cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YipCell {
    pub date: NaiveDate,
    pub mood: Option<u8>,
    pub energy: Option<u8>,
    /// True if the day has a journal note (any content),
    /// even if mood wasn't recorded.
    pub has_entry: bool,
}

impl YipCell {
    pub fn is_empty(&self) -> bool {
        self.mood.is_none() && self.energy.is_none() && !self.has_entry
    }
}

/// A year-wide grid of [`YipCell`]s, indexed by date.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct YipGrid {
    pub year: i32,
    pub cells: BTreeMap<NaiveDate, YipCell>,
}

impl YipGrid {
    /// Build a YIP grid for `year` by reading mood/energy
    /// from the journal folder.
    pub fn for_year(
        index: &NoteIndex,
        config: &JournalConfig,
        year: i32,
    ) -> Self {
        let mut cells = BTreeMap::new();
        // Seed every day of the year with a blank cell so
        // the SVG shows all 365 days, not just the ones
        // with notes.
        for month in 1..=12 {
            let days = days_in_month(year, month);
            for day in 1..=days {
                if let Some(d) = NaiveDate::from_ymd_opt(year, month, day) {
                    cells.insert(
                        d,
                        YipCell {
                            date: d,
                            mood: None,
                            energy: None,
                            has_entry: false,
                        },
                    );
                }
            }
        }
        // Walk every note in the journal folder and
        // populate cells whose date matches the requested
        // year.
        for note in index.notes() {
            let Some(parent_name) = note
                .path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
            else {
                continue;
            };
            if parent_name != config.folder {
                continue;
            }
            let Some(stem) = note.path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(date) = NaiveDate::parse_from_str(stem, "%Y-%m-%d") else {
                continue;
            };
            if date.year() != year {
                continue;
            }
            let mood = extract_int_field(&note.content, "mood");
            let energy = extract_int_field(&note.content, "energy");
            let cell = cells.entry(date).or_insert(YipCell {
                date,
                mood: None,
                energy: None,
                has_entry: false,
            });
            cell.mood = mood;
            cell.energy = energy;
            cell.has_entry = true;
        }
        Self { year, cells }
    }

    /// Number of cells with a mood value (used by the
    /// legend / coverage stat).
    pub fn filled(&self) -> usize {
        self.cells.values().filter(|c| c.mood.is_some()).count()
    }

    /// Number of days with a journal entry, regardless of
    /// mood.
    pub fn entries(&self) -> usize {
        self.cells.values().filter(|c| c.has_entry).count()
    }
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

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("valid month rollover");
    let last_this = first_next
        .pred_opt()
        .expect("month has a last day");
    last_this.day()
}

/// A self-contained SVG string for a YIP. Cells are 16×16
/// pixels with 2-pixel gaps, arranged 12 rows by 31
/// columns (max). Cells without entries are rendered as
/// outlines; cells with mood values use a 0..=10 gradient
/// from cool blue to warm red.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YipSvg {
    pub svg: String,
    pub width: u32,
    pub height: u32,
}

impl YipSvg {
    /// Render a YIP for the given year and grid.
    pub fn render(grid: &YipGrid) -> Self {
        const CELL: u32 = 16;
        const GAP: u32 = 2;
        const COLS: u32 = 31;
        const ROWS: u32 = 12;
        const MARGIN: u32 = 40;
        let width = MARGIN * 2 + COLS * (CELL + GAP);
        let height = MARGIN + ROWS * (CELL + GAP) + 20;

        let mut out = String::new();
        out.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" \
             viewBox=\"0 0 {width} {height}\" \
             width=\"{width}\" height=\"{height}\">\n"
        ));
        out.push_str(&format!(
            "  <text x=\"{MARGIN}\" y=\"24\" font-family=\"sans-serif\" \
             font-size=\"16\" fill=\"#dcddde\">{}</text>\n",
            grid.year
        ));

        for (date, cell) in &grid.cells {
            let col = date.day() - 1;
            let row = date.month() - 1;
            let x = MARGIN + col as u32 * (CELL + GAP);
            let y = MARGIN + row as u32 * (CELL + GAP);
            let fill = if cell.is_empty() {
                "#2f3034"
            } else {
                mood_color(cell.mood)
            };
            out.push_str(&format!(
                "  <rect x=\"{x}\" y=\"{y}\" width=\"{CELL}\" height=\"{CELL}\" \
                 fill=\"{fill}\" rx=\"2\" />\n"
            ));
        }

        // Legend at the bottom.
        let legend_y = MARGIN + ROWS * (CELL + GAP) + 4;
        out.push_str(&format!(
            "  <text x=\"{MARGIN}\" y=\"{legend_y}\" font-family=\"sans-serif\" \
             font-size=\"10\" fill=\"#999a9b\">mood</text>\n"
        ));
        for i in 0..=10u8 {
            let x = MARGIN + 36 + i as u32 * (CELL + GAP);
            let fill = mood_color(Some(i));
            out.push_str(&format!(
                "  <rect x=\"{x}\" y=\"{legend_y_}\" width=\"{CELL}\" \
                 height=\"{CELL}\" fill=\"{fill}\" rx=\"2\" />\n",
                legend_y_ = legend_y - 12
            ));
        }

        out.push_str("</svg>\n");
        Self {
            svg: out,
            width,
            height,
        }
    }
}

fn mood_color(mood: Option<u8>) -> &'static str {
    let m = match mood {
        Some(m) => m,
        None => return "#3a3b40",
    };
    // 11-step palette from cool (low mood) to warm (high
    // mood). Each step is a hex color.
    match m {
        0 => "#3a3b40",
        1 => "#5b8def",
        2 => "#4a7ce0",
        3 => "#3e6dcd",
        4 => "#4a8b5e",
        5 => "#7da83a",
        6 => "#c0b73a",
        7 => "#e0a32e",
        8 => "#e07a2e",
        9 => "#e0532e",
        10 => "#e0302e",
        _ => "#3a3b40",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let unique = format!(
            "tungsten-yip-test-{}-{}",
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
    fn grid_covers_full_year() {
        let dir = tempdir();
        let index = NoteIndex::build(&dir).unwrap();
        let grid = YipGrid::for_year(&index, &JournalConfig::default(), 2026);
        // 2026 is not a leap year.
        assert_eq!(grid.cells.len(), 365);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn grid_picks_up_mood_and_energy() {
        let dir = tempdir();
        let j = dir.join("Journal");
        fs::create_dir_all(&j).unwrap();
        fs::write(
            j.join("2026-03-15.md"),
            "## Mood\nmood: 8\nenergy: 6\n",
        )
        .unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let grid = YipGrid::for_year(&index, &JournalConfig::default(), 2026);
        let cell = grid
            .cells
            .get(&NaiveDate::from_ymd_opt(2026, 3, 15).unwrap())
            .unwrap();
        assert_eq!(cell.mood, Some(8));
        assert_eq!(cell.energy, Some(6));
        assert!(cell.has_entry);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn grid_ignores_other_years() {
        let dir = tempdir();
        let j = dir.join("Journal");
        fs::create_dir_all(&j).unwrap();
        fs::write(j.join("2025-01-01.md"), "mood: 9\n").unwrap();
        fs::write(j.join("2026-01-01.md"), "mood: 1\n").unwrap();
        let index = NoteIndex::build(&dir).unwrap();
        let g26 = YipGrid::for_year(&index, &JournalConfig::default(), 2026);
        let g25 = YipGrid::for_year(&index, &JournalConfig::default(), 2025);
        assert_eq!(
            g26.cells
                .get(&NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
                .unwrap()
                .mood,
            Some(1)
        );
        assert_eq!(
            g25.cells
                .get(&NaiveDate::from_ymd_opt(2025, 1, 1).unwrap())
                .unwrap()
                .mood,
            Some(9)
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn svg_includes_year_and_cells() {
        let dir = tempdir();
        let index = NoteIndex::build(&dir).unwrap();
        let grid = YipGrid::for_year(&index, &JournalConfig::default(), 2026);
        let svg = YipSvg::render(&grid);
        assert!(svg.svg.contains("2026"));
        // 365 cells + legend rectangles.
        let rect_count = svg.svg.matches("<rect").count();
        assert!(rect_count >= 365);
        assert!(svg.svg.contains("</svg>"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn svg_uses_warm_color_for_high_mood() {
        assert_eq!(mood_color(Some(10)), "#e0302e");
        assert_eq!(mood_color(Some(0)), "#3a3b40");
        assert_eq!(mood_color(None), "#3a3b40");
    }

    #[test]
    fn days_in_month_handles_february() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 1), 31);
        assert_eq!(days_in_month(2026, 4), 30);
    }
}
