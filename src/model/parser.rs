// File: ./src/model/parser.rs
// Handles smart text input parsing
use crate::model::item::Task;
use chrono::Local;
use chrono::NaiveDate;
use std::collections::HashMap;

impl Task {
    pub fn apply_smart_input(&mut self, input: &str, aliases: &HashMap<String, Vec<String>>) {
        let mut summary_words = Vec::new();
        self.priority = 0;
        self.due = None;
        self.rrule = None;
        self.categories.clear();

        let mut tokens = input.split_whitespace().peekable();

        while let Some(word) = tokens.next() {
            if word.starts_with('!')
                && let Ok(p) = word[1..].parse::<u8>()
                && (1..=9).contains(&p)
            {
                self.priority = p;
                continue;
            }
            // 3. Duration (~30m, ~1h)
            if let Some(dur_str) = word.strip_prefix('~') {
                let lower = dur_str.to_lowercase();
                let minutes = if let Some(n) = lower.strip_suffix('m') {
                    n.parse::<u32>().ok()
                } else if let Some(n) = lower.strip_suffix('h') {
                    n.parse::<u32>().ok().map(|h| h * 60)
                } else if let Some(n) = lower.strip_suffix('d') {
                    n.parse::<u32>().ok().map(|d| d * 24 * 60)
                } else if let Some(n) = lower.strip_suffix('w') {
                    n.parse::<u32>().ok().map(|w| w * 7 * 24 * 60)
                } else if let Some(n) = lower.strip_suffix("mo") {
                    n.parse::<u32>().ok().map(|mo| mo * 30 * 24 * 60)
                } else if let Some(n) = lower.strip_suffix('y') {
                    n.parse::<u32>().ok().map(|y| y * 365 * 24 * 60)
                } else {
                    None
                };

                if let Some(m) = minutes {
                    self.estimated_duration = Some(m);
                    continue;
                }
            }
            // 2. Categories (#tag)
            if let Some(stripped) = word.strip_prefix('#') {
                let cat = stripped.to_string();
                if !cat.is_empty() {
                    if !self.categories.contains(&cat) {
                        self.categories.push(cat.clone());
                    }
                    if let Some(expanded_tags) = aliases.get(&cat) {
                        for extra_tag in expanded_tags {
                            if !self.categories.contains(extra_tag) {
                                self.categories.push(extra_tag.clone());
                            }
                        }
                    }
                    continue;
                }
            }

            if word == "@daily" {
                self.rrule = Some("FREQ=DAILY".to_string());
                continue;
            }
            if word == "@weekly" {
                self.rrule = Some("FREQ=WEEKLY".to_string());
                continue;
            }
            if word == "@monthly" {
                self.rrule = Some("FREQ=MONTHLY".to_string());
                continue;
            }
            if word == "@yearly" {
                self.rrule = Some("FREQ=YEARLY".to_string());
                continue;
            }

            if word == "@every" {
                if let Some(next_token) = tokens.peek()
                    && let Ok(interval) = next_token.parse::<u32>()
                {
                    tokens.next();
                    if let Some(unit_token) = tokens.peek() {
                        let unit = unit_token.to_lowercase();
                        let freq = if unit.starts_with("day") {
                            "DAILY"
                        } else if unit.starts_with("week") {
                            "WEEKLY"
                        } else if unit.starts_with("month") {
                            "MONTHLY"
                        } else if unit.starts_with("year") {
                            "YEARLY"
                        } else {
                            ""
                        };

                        if !freq.is_empty() {
                            tokens.next();
                            self.rrule = Some(format!("FREQ={};INTERVAL={}", freq, interval));
                            continue;
                        }
                    }
                }
                summary_words.push(word);
                continue;
            }

            if let Some(val) = word.strip_prefix('@') {
                if let Ok(date) = NaiveDate::parse_from_str(val, "%Y-%m-%d")
                    && let Some(dt) = date.and_hms_opt(23, 59, 59)
                {
                    self.due = Some(dt.and_utc());
                    continue;
                }
                let now = Local::now().date_naive();
                if val == "today"
                    && let Some(dt) = now.and_hms_opt(23, 59, 59)
                {
                    self.due = Some(dt.and_utc());
                    continue;
                }
                if val == "tomorrow" {
                    let d = now + chrono::Duration::days(1);
                    if let Some(dt) = d.and_hms_opt(23, 59, 59) {
                        self.due = Some(dt.and_utc());
                        continue;
                    }
                }
                if val == "next"
                    && let Some(unit_token) = tokens.peek()
                {
                    let unit = unit_token.to_lowercase();
                    let mut offset = 0;
                    if unit.starts_with("week") {
                        offset = 7;
                    } else if unit.starts_with("month") {
                        offset = 30;
                    } else if unit.starts_with("year") {
                        offset = 365;
                    }

                    if offset > 0 {
                        tokens.next();
                        let d = now + chrono::Duration::days(offset);
                        if let Some(dt) = d.and_hms_opt(23, 59, 59) {
                            self.due = Some(dt.and_utc());
                            continue;
                        }
                    }
                }
            }
            summary_words.push(word);
        }
        self.summary = summary_words.join(" ");
    }

    pub fn to_smart_string(&self) -> String {
        let mut s = self.summary.clone();
        if self.priority > 0 {
            s.push_str(&format!(" !{}", self.priority));
        }
        if let Some(d) = self.due {
            s.push_str(&format!(" @{}", d.format("%Y-%m-%d")));
        }
        if let Some(mins) = self.estimated_duration {
            // Helper for formatting
            let dur_str = if mins >= 525600 {
                format!("~{}y", mins / 525600)
            } else if mins >= 43200 {
                format!("~{}mo", mins / 43200)
            } else if mins >= 10080 {
                format!("~{}w", mins / 10080)
            } else if mins >= 1440 {
                format!("~{}d", mins / 1440)
            } else if mins >= 60 {
                format!("~{}h", mins / 60)
            } else {
                format!("~{}m", mins)
            };
            s.push_str(&format!(" {}", dur_str));
        }
        if let Some(r) = &self.rrule {
            if r == "FREQ=DAILY" {
                s.push_str(" @daily");
            } else if r == "FREQ=WEEKLY" {
                s.push_str(" @weekly");
            } else if r == "FREQ=MONTHLY" {
                s.push_str(" @monthly");
            } else if r == "FREQ=YEARLY" {
                s.push_str(" @yearly");
            }
        }
        for cat in &self.categories {
            s.push_str(&format!(" #{}", cat));
        }
        s
    }
}
