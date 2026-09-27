//! Taskwarrior-style urgency scoring.
//!
//! Urgency is a sum of weighted factors. Each factor evaluates to a value (usually in `0.0..=1.0`)
//! which is multiplied by a coefficient; the products are summed into a single score. The
//! defaults mirror Taskwarrior's `urgency.*.coefficient` settings where a todu equivalent exists:
//!
//! | factor      | todu source                                   | default |
//! |-------------|-----------------------------------------------|---------|
//! | `due`       | due date, scaled from 14 days out to 7 overdue | 12.0    |
//! | `blocking`  | subtask of a parent (it blocks the parent)     | 8.0     |
//! | `priority`  | high / medium / low                            | 6.0 / 3.9 / 1.8 |
//! | `impact`    | judged impact `0.0..=1.0` (see `todu impact`)  | 6.0     |
//! | `active`    | status `in-progress`                           | 4.0     |
//! | `age`       | days since creation, capped at `max_age`       | 2.0     |
//! | `desc`      | has a description (Taskwarrior: annotations)   | 1.0     |
//! | `tags`      | has a tag                                      | 1.0     |
//! | `tag.<t>`   | per-tag coefficient, `next` defaults to 15.0   | —       |
//! | `waiting`   | status `paused`                                | -3.0    |
//! | `blocked`   | has unfinished subtasks                        | -5.0    |
//!
//! Done and stopped todos always have zero urgency.

use chrono::{DateTime, Local, TimeZone, Utc};
use std::collections::HashMap;

use super::{ToduPriority, ToduRow, ToduStatus};

/// Weights applied to each urgency factor
#[derive(Debug, Clone, PartialEq)]
pub struct UrgencyCoefficients {
    /// Weight of the due-date factor
    pub due: f64,
    /// Weight for todos that block their parent
    pub blocking: f64,
    /// Weight for high priority
    pub priority_high: f64,
    /// Weight for medium priority
    pub priority_medium: f64,
    /// Weight for low priority
    pub priority_low: f64,
    /// Weight of the judged impact factor
    pub impact: f64,
    /// Weight for in-progress todos
    pub active: f64,
    /// Weight of the age factor
    pub age: f64,
    /// Age in days at which the age factor saturates
    pub max_age: f64,
    /// Weight for todos with a description
    pub desc: f64,
    /// Weight for todos with any tag
    pub tags: f64,
    /// Weight for paused todos
    pub waiting: f64,
    /// Weight for todos with unfinished subtasks
    pub blocked: f64,
    /// Extra weight for specific tags
    pub tag: HashMap<String, f64>,
}

impl Default for UrgencyCoefficients {
    fn default() -> Self {
        Self {
            due: 12.0,
            blocking: 8.0,
            priority_high: 6.0,
            priority_medium: 3.9,
            priority_low: 1.8,
            impact: 6.0,
            active: 4.0,
            age: 2.0,
            max_age: 365.0,
            desc: 1.0,
            tags: 1.0,
            waiting: -3.0,
            blocked: -5.0,
            tag: HashMap::from([("next".to_owned(), 15.0)]),
        }
    }
}

impl UrgencyCoefficients {
    /// Overrides a coefficient by name. Returns `false` if `key` is not a known coefficient.
    /// Per-tag coefficients are addressed as `tag.<name>`.
    pub fn set(&mut self, key: &str, value: f64) -> bool {
        if let Some(tag) = key.strip_prefix("tag.") {
            self.tag.insert(tag.to_owned(), value);
            return true;
        }
        let field = match key {
            "due" => &mut self.due,
            "blocking" => &mut self.blocking,
            "priority_high" => &mut self.priority_high,
            "priority_medium" => &mut self.priority_medium,
            "priority_low" => &mut self.priority_low,
            "impact" => &mut self.impact,
            "active" => &mut self.active,
            "age" => &mut self.age,
            "max_age" => &mut self.max_age,
            "desc" => &mut self.desc,
            "tags" => &mut self.tags,
            "waiting" => &mut self.waiting,
            "blocked" => &mut self.blocked,
            _ => return false,
        };
        *field = value;
        true
    }
}

/// One factor's contribution to a todo's urgency
#[derive(Debug, Clone, PartialEq)]
pub struct UrgencyTerm {
    /// Factor name
    pub factor: String,
    /// Factor value, usually in `0.0..=1.0`
    pub value: f64,
    /// Coefficient the value is multiplied by
    pub coefficient: f64,
}

impl UrgencyTerm {
    fn new(factor: impl Into<String>, value: f64, coefficient: f64) -> Self {
        Self {
            factor: factor.into(),
            value,
            coefficient,
        }
    }

    /// `value * coefficient`
    pub fn contribution(&self) -> f64 {
        self.value * self.coefficient
    }
}

/// Taskwarrior's due-date curve: 0.2 when due 14+ days out, rising linearly to 1.0 at 7 days
/// overdue.
fn due_factor(days_overdue: f64) -> f64 {
    if days_overdue >= 7.0 {
        1.0
    } else if days_overdue >= -14.0 {
        (days_overdue + 14.0) * 0.8 / 21.0 + 0.2
    } else {
        0.2
    }
}

impl ToduRow {
    /// Returns the non-zero factors making up this todo's urgency at `now`
    pub fn urgency_terms(
        &self,
        coeffs: &UrgencyCoefficients,
        now: DateTime<Utc>,
    ) -> Vec<UrgencyTerm> {
        if !self.status.is_active() {
            return Vec::new();
        }
        let mut terms = Vec::new();

        if let Some(due) = self.due {
            let due = Local
                .from_local_datetime(&due.and_hms_opt(23, 59, 59).unwrap())
                .earliest()
                .map(|d| d.with_timezone(&Utc));
            if let Some(due) = due {
                let days_overdue = (now - due).num_seconds() as f64 / 86_400.0;
                terms.push(UrgencyTerm::new(
                    "due",
                    due_factor(days_overdue),
                    coeffs.due,
                ));
            }
        }

        if self.pptid.is_some() {
            terms.push(UrgencyTerm::new("blocking", 1.0, coeffs.blocking));
        }

        if let Some(priority) = self.priority {
            let coefficient = match priority {
                ToduPriority::High => coeffs.priority_high,
                ToduPriority::Medium => coeffs.priority_medium,
                ToduPriority::Low => coeffs.priority_low,
            };
            terms.push(UrgencyTerm::new(
                format!("priority.{}", priority.label()),
                1.0,
                coefficient,
            ));
        }

        if let Some(impact) = self.impact {
            terms.push(UrgencyTerm::new(
                "impact",
                impact.clamp(0.0, 1.0),
                coeffs.impact,
            ));
        }

        match self.status {
            ToduStatus::InProgress => terms.push(UrgencyTerm::new("active", 1.0, coeffs.active)),
            ToduStatus::Paused => terms.push(UrgencyTerm::new("waiting", 1.0, coeffs.waiting)),
            _ => {}
        }

        if coeffs.max_age > 0.0 {
            let age_days = (now - self.created).num_seconds().max(0) as f64 / 86_400.0;
            terms.push(UrgencyTerm::new(
                "age",
                (age_days / coeffs.max_age).min(1.0),
                coeffs.age,
            ));
        }

        if self.desc.is_some() {
            terms.push(UrgencyTerm::new("desc", 1.0, coeffs.desc));
        }

        if let Some(ref tag) = self.tag {
            terms.push(UrgencyTerm::new("tags", 1.0, coeffs.tags));
            if let Some(&c) = coeffs.tag.get(tag) {
                terms.push(UrgencyTerm::new(format!("tag.{tag}"), 1.0, c));
            }
        }

        if self.subtasks.iter().any(|s| s.status.is_active()) {
            terms.push(UrgencyTerm::new("blocked", 1.0, coeffs.blocked));
        }

        terms.retain(|t| t.contribution() != 0.0);
        terms
    }

    /// Computes this todo's urgency at `now`
    pub fn compute_urgency(&self, coeffs: &UrgencyCoefficients, now: DateTime<Utc>) -> f64 {
        let sum: f64 = self
            .urgency_terms(coeffs, now)
            .iter()
            .map(UrgencyTerm::contribution)
            .sum();
        (sum * 1000.0).round() / 1000.0
    }
}

/// Sets `urgency` on every row in the tree
pub(crate) fn apply_urgency(
    rows: &mut [ToduRow],
    coeffs: &UrgencyCoefficients,
    now: DateTime<Utc>,
) {
    for row in rows.iter_mut() {
        apply_urgency(&mut row.subtasks, coeffs, now);
        row.urgency = row.compute_urgency(coeffs, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToduSource;
    use chrono::{Duration, NaiveDate};
    use rstest::rstest;

    fn row(status: ToduStatus) -> ToduRow {
        ToduRow {
            ptid: 1,
            title: String::new(),
            status,
            priority: None,
            due: None,
            desc: None,
            created: Utc::now(),
            pptid: None,
            tag: None,
            branch: None,
            source: ToduSource::Local,
            impact: None,
            urgency: 0.0,
            subtasks: vec![],
        }
    }

    fn urgency(r: &ToduRow) -> f64 {
        r.compute_urgency(&UrgencyCoefficients::default(), Utc::now())
    }

    #[rstest]
    #[case(30.0, 1.0)]
    #[case(7.0, 1.0)]
    #[case(0.0, 14.0 * 0.8 / 21.0 + 0.2)]
    #[case(-14.0, 0.2)]
    #[case(-60.0, 0.2)]
    fn due_curve(#[case] days: f64, #[case] expected: f64) {
        assert!((due_factor(days) - expected).abs() < 1e-9);
    }

    #[test]
    fn plain_pending_todo_is_zero() {
        assert_eq!(urgency(&row(ToduStatus::Pending)), 0.0);
    }

    #[rstest]
    #[case(ToduStatus::Done)]
    #[case(ToduStatus::Stopped)]
    fn finished_todos_are_zero(#[case] status: ToduStatus) {
        let mut r = row(status);
        r.priority = Some(ToduPriority::High);
        r.tag = Some("next".into());
        assert_eq!(urgency(&r), 0.0);
    }

    #[test]
    fn priority_and_status_add_up() {
        let mut r = row(ToduStatus::InProgress);
        r.priority = Some(ToduPriority::High);
        assert_eq!(urgency(&r), 6.0 + 4.0);
    }

    #[test]
    fn next_tag_counts_as_tag_and_next() {
        let mut r = row(ToduStatus::Pending);
        r.tag = Some("next".into());
        assert_eq!(urgency(&r), 1.0 + 15.0);
    }

    #[test]
    fn impact_is_clamped_and_weighted() {
        let mut r = row(ToduStatus::Pending);
        r.impact = Some(0.5);
        assert_eq!(urgency(&r), 3.0);
        r.impact = Some(4.0);
        assert_eq!(urgency(&r), 6.0);
    }

    #[test]
    fn parent_with_open_subtasks_is_blocked_and_child_is_blocking() {
        let mut child = row(ToduStatus::Pending);
        child.pptid = Some(1);
        let mut parent = row(ToduStatus::Pending);
        parent.subtasks.push(child);
        assert_eq!(urgency(&parent), -5.0);
        assert_eq!(urgency(&parent.subtasks[0]), 8.0);
    }

    #[test]
    fn overdue_beats_far_future() {
        let today = Local::now().date_naive();
        let mut soon = row(ToduStatus::Pending);
        soon.due = Some(today - Duration::days(10));
        let mut later = row(ToduStatus::Pending);
        later.due = Some(NaiveDate::from_ymd_opt(2999, 1, 1).unwrap());
        assert_eq!(urgency(&soon), 12.0);
        assert!((urgency(&later) - 2.4).abs() < 1e-9);
    }

    #[test]
    fn age_saturates() {
        let mut r = row(ToduStatus::Pending);
        r.created = Utc::now() - Duration::days(1000);
        assert_eq!(urgency(&r), 2.0);
    }

    #[test]
    fn paused_is_negative() {
        assert_eq!(urgency(&row(ToduStatus::Paused)), -3.0);
    }

    #[test]
    fn set_coefficients() {
        let mut c = UrgencyCoefficients::default();
        assert!(c.set("due", 1.0));
        assert!(c.set("tag.work", 2.5));
        assert!(!c.set("bogus", 1.0));
        assert_eq!(c.due, 1.0);
        assert_eq!(c.tag["work"], 2.5);
    }
}
