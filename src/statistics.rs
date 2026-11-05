//! Writing statistics derived from saved versions, using the editor's word rules.

use chrono::{Datelike, Days, Months, NaiveDate};
use similar::{DiffTag, TextDiff};
use std::collections::BTreeMap;
use std::ops::AddAssign;
use std::path::PathBuf;

pub fn count_words(text: &str) -> usize {
    let mut count = 0;
    let mut in_word = false;
    for c in text.chars() {
        if c.is_whitespace() {
            in_word = false;
        } else if is_cjk(c) {
            count += 1;
            in_word = false;
        } else if !in_word {
            count += 1;
            in_word = true;
        }
    }
    count
}

fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{20000}'..='\u{2A6DF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
        || ('\u{2F800}'..='\u{2FA1F}').contains(&c)
}

fn word_tokens(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut word_start = None;
    for (index, c) in text.char_indices() {
        if c.is_whitespace() || is_cjk(c) {
            if let Some(start) = word_start.take() {
                tokens.push(&text[start..index]);
            }
            if is_cjk(c) {
                tokens.push(&text[index..index + c.len_utf8()]);
            }
        } else {
            word_start.get_or_insert(index);
        }
    }
    if let Some(start) = word_start {
        tokens.push(&text[start..]);
    }
    tokens
}

/// Compare words rather than just lengths, so equal-length rewrites count too.
pub fn word_changes(previous: &str, current: &str) -> (usize, usize) {
    let old = word_tokens(previous);
    let new = word_tokens(current);
    let diff = TextDiff::from_slices(&old, &new);
    let mut added = 0;
    let mut removed = 0;
    for op in diff.ops() {
        if op.tag() != DiffTag::Equal {
            added += op.new_range().len();
            removed += op.old_range().len();
        }
    }
    (added, removed)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WritingActivity {
    pub added: usize,
    pub removed: usize,
    pub seconds: u64,
}

impl WritingActivity {
    pub fn is_active(self) -> bool {
        self.added > 0 || self.removed > 0
    }

    pub fn net(self) -> i128 {
        self.added as i128 - self.removed as i128
    }
}

impl AddAssign for WritingActivity {
    fn add_assign(&mut self, rhs: Self) {
        self.added += rhs.added;
        self.removed += rhs.removed;
        self.seconds += rhs.seconds;
    }
}

#[derive(Debug, Default)]
pub struct DocumentStatistics {
    pub path: Option<PathBuf>,
    pub id: String,
    /// None when the latest version cannot be read. Older versions are not totals.
    pub words: Option<usize>,
    pub days: BTreeMap<NaiveDate, WritingActivity>,
}

#[derive(Debug, Default)]
pub struct WritingStatistics {
    pub documents: Vec<DocumentStatistics>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StatisticsPeriod {
    Last7Days,
    #[default]
    Last30Days,
    ThisYear,
    AllTime,
}

impl StatisticsPeriod {
    pub fn start(self, today: NaiveDate, earliest: NaiveDate) -> NaiveDate {
        match self {
            Self::Last7Days => today - Days::new(6),
            Self::Last30Days => today - Days::new(29),
            Self::ThisYear => today.with_ordinal(1).unwrap(),
            Self::AllTime => earliest.min(today),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StatisticsGrouping {
    #[default]
    Day,
    Week,
    Month,
}

impl StatisticsGrouping {
    pub fn start(self, date: NaiveDate) -> NaiveDate {
        match self {
            Self::Day => date,
            Self::Week => date - Days::new(u64::from(date.weekday().num_days_from_monday())),
            Self::Month => date.with_day(1).unwrap(),
        }
    }

    fn next(self, date: NaiveDate) -> NaiveDate {
        match self {
            Self::Day => date + Days::new(1),
            Self::Week => date + Days::new(7),
            Self::Month => date + Months::new(1),
        }
    }
}

pub struct StatisticsReport {
    pub start: NaiveDate,
    pub activity: WritingActivity,
    pub active_days: usize,
    pub buckets: Vec<(NaiveDate, WritingActivity)>,
}

impl WritingStatistics {
    pub fn total_words(&self) -> usize {
        self.documents.iter().filter_map(|doc| doc.words).sum()
    }

    pub fn report(
        &self,
        period: StatisticsPeriod,
        grouping: StatisticsGrouping,
        today: NaiveDate,
    ) -> StatisticsReport {
        let mut days = BTreeMap::<NaiveDate, WritingActivity>::new();
        for doc in &self.documents {
            for (&date, &activity) in &doc.days {
                if date <= today {
                    *days.entry(date).or_default() += activity;
                }
            }
        }
        let earliest = days.first_key_value().map_or(today, |(&date, _)| date);
        let start = period.start(today, earliest);
        let mut buckets = BTreeMap::<NaiveDate, WritingActivity>::new();
        let mut date = grouping.start(start);
        while date <= today {
            buckets.insert(date, WritingActivity::default());
            date = grouping.next(date);
        }
        let mut activity = WritingActivity::default();
        let mut active_days = 0;
        for (&date, &day) in days.range(start..=today) {
            activity += day;
            active_days += usize::from(day.is_active());
            *buckets.entry(grouping.start(date)).or_default() += day;
        }
        StatisticsReport {
            start,
            activity,
            active_days,
            buckets: buckets.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    #[test]
    fn tokens_match_editor_counts_for_mixed_text() {
        for text in [
            "",
            " \n\t",
            "hello world",
            "你好世界",
            "你好 hello 世界!",
            "𠀀test　字",
        ] {
            assert_eq!(word_tokens(text).len(), count_words(text), "{text:?}");
        }
        assert_eq!(count_words("你好 hello 世界"), 5);
    }

    #[test]
    fn revisions_count_replacements_deletions_and_repeated_saves() {
        assert_eq!(word_changes("hello cat", "hello dog"), (1, 1));
        assert_eq!(word_changes("我爱你", "我想你"), (1, 1));
        assert_eq!(word_changes("甲乙 hello", "甲"), (0, 2));
        assert_eq!(word_changes("你好 world", "你好 world"), (0, 0));
        assert_eq!(word_changes("hello world", "hello\n  world"), (0, 0));
        assert_eq!(word_changes("", "你好 world"), (3, 0));
    }

    #[test]
    fn period_includes_today_and_fills_inactive_days() {
        let stats = WritingStatistics {
            documents: vec![DocumentStatistics {
                words: Some(12),
                days: [
                    (
                        date("2026-10-29"),
                        WritingActivity {
                            added: 10,
                            ..Default::default()
                        },
                    ),
                    (
                        date("2026-10-30"),
                        WritingActivity {
                            added: 4,
                            seconds: 60,
                            ..Default::default()
                        },
                    ),
                    (
                        date("2026-11-05"),
                        WritingActivity {
                            removed: 2,
                            ..Default::default()
                        },
                    ),
                    (
                        date("2026-11-06"),
                        WritingActivity {
                            added: 100,
                            ..Default::default()
                        },
                    ),
                ]
                .into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let report = stats.report(
            StatisticsPeriod::Last7Days,
            StatisticsGrouping::Day,
            date("2026-11-05"),
        );
        assert_eq!(report.start, date("2026-10-30"));
        assert_eq!(report.buckets.len(), 7);
        assert_eq!(report.active_days, 2);
        assert_eq!(
            report.activity,
            WritingActivity {
                added: 4,
                removed: 2,
                seconds: 60
            }
        );
        assert_eq!(report.buckets[1].1, WritingActivity::default());
    }

    #[test]
    fn grouping_uses_monday_and_calendar_month_boundaries() {
        assert_eq!(
            StatisticsGrouping::Week.start(date("2027-01-01")),
            date("2026-12-28")
        );
        assert_eq!(
            StatisticsGrouping::Month.start(date("2026-11-05")),
            date("2026-11-01")
        );
        assert_eq!(
            StatisticsPeriod::ThisYear.start(date("2026-11-05"), date("2020-01-01")),
            date("2026-01-01")
        );
        let stats = WritingStatistics::default();
        let report = stats.report(
            StatisticsPeriod::Last30Days,
            StatisticsGrouping::Week,
            date("2027-01-01"),
        );
        assert_eq!(report.buckets.first().unwrap().0, date("2026-11-30"));
        assert_eq!(report.buckets.last().unwrap().0, date("2026-12-28"));
    }

    #[test]
    fn totals_and_active_days_combine_documents_without_counting_versions() {
        let day = date("2026-11-05");
        let stats = WritingStatistics {
            documents: vec![
                DocumentStatistics {
                    words: Some(10),
                    days: [(
                        day,
                        WritingActivity {
                            added: 15,
                            removed: 5,
                            seconds: 30,
                        },
                    )]
                    .into(),
                    ..Default::default()
                },
                DocumentStatistics {
                    words: Some(3),
                    days: [(
                        day,
                        WritingActivity {
                            added: 3,
                            ..Default::default()
                        },
                    )]
                    .into(),
                    ..Default::default()
                },
                DocumentStatistics {
                    words: None,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let report = stats.report(StatisticsPeriod::AllTime, StatisticsGrouping::Month, day);
        assert_eq!(stats.total_words(), 13);
        assert_eq!(report.active_days, 1);
        assert_eq!(report.activity.added, 18);
        assert_eq!(report.activity.net(), 13);
        assert_eq!(report.buckets.len(), 1);
    }
}
