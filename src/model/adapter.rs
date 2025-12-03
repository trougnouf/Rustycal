// File: ./src/model/adapter.rs
// Handles ICS serialization/deserialization
use crate::model::item::{Task, TaskStatus};
use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use icalendar::{Calendar, CalendarComponent, Component, Todo, TodoStatus};
use rrule::RRuleSet;
use std::str::FromStr;
use uuid::Uuid;

impl Task {
    pub fn respawn(&self) -> Option<Task> {
        let rule_str = self.rrule.as_ref()?;
        let due_utc = self.due?;
        let dtstart = due_utc.format("%Y%m%dT%H%M%SZ").to_string();
        let rrule_string = format!("DTSTART:{}\nRRULE:{}", dtstart, rule_str);

        if let Ok(rrule_set) = RRuleSet::from_str(&rrule_string) {
            let result = rrule_set.all(2);
            let dates = result.dates;
            if dates.len() > 1 {
                let next_due = dates[1];
                let mut next_task = self.clone();
                next_task.uid = Uuid::new_v4().to_string();
                next_task.href = String::new();
                next_task.etag = String::new();
                next_task.status = TaskStatus::NeedsAction;
                next_task.due = Some(Utc.from_utc_datetime(&next_due.naive_utc()));
                next_task.dependencies.clear();
                return Some(next_task);
            }
        }
        None
    }

    pub fn to_ics(&self) -> String {
        let mut todo = Todo::new();
        todo.uid(&self.uid);
        todo.summary(&self.summary);
        if !self.description.is_empty() {
            todo.description(&self.description);
        }
        todo.timestamp(Utc::now());

        match self.status {
            TaskStatus::NeedsAction => todo.status(TodoStatus::NeedsAction),
            TaskStatus::InProcess => todo.status(TodoStatus::InProcess),
            TaskStatus::Completed => todo.status(TodoStatus::Completed),
            TaskStatus::Cancelled => todo.status(TodoStatus::Cancelled),
        };

        // Helper for ISO Duration
        fn format_iso_duration(mins: u32) -> String {
            if mins.is_multiple_of(24 * 60) {
                format!("P{}D", mins / (24 * 60))
            } else if mins.is_multiple_of(60) {
                format!("PT{}H", mins / 60)
            } else {
                format!("PT{}M", mins)
            }
        }

        if let Some(dt) = self.due {
            let formatted = dt.format("%Y%m%dT%H%M%SZ").to_string();
            todo.add_property("DUE", &formatted);

            // Due exists -> Use X-ESTIMATED-DURATION
            if let Some(mins) = self.estimated_duration {
                let val = format_iso_duration(mins);
                todo.add_property("X-ESTIMATED-DURATION", &val);
            }
        } else {
            // No Due -> Use Standard DURATION
            if let Some(mins) = self.estimated_duration {
                let val = format_iso_duration(mins);
                todo.add_property("DURATION", &val);
            }
        }
        if self.priority > 0 {
            todo.priority(self.priority.into());
        }
        if let Some(rrule) = &self.rrule {
            todo.add_property("RRULE", rrule.as_str());
        }
        // NOTE: We do NOT add categories here using the library.
        // The library escapes all commas, turning "A,B" into "A\,B", which treats it as one tag.
        // We manually inject the correctly formatted line below.

        // --- HIERARCHY & DEPENDENCIES ---
        // Use append_multi_property to support multiple RELATED-TO lines.
        if let Some(p_uid) = &self.parent_uid {
            let prop = icalendar::Property::new("RELATED-TO", p_uid.as_str());
            todo.append_multi_property(prop);
        }

        for dep_uid in &self.dependencies {
            let mut prop = icalendar::Property::new("RELATED-TO", dep_uid);
            prop.add_parameter("RELTYPE", "DEPENDS-ON");
            todo.append_multi_property(prop);
        }

        let mut calendar = Calendar::new();
        calendar.push(todo);
        let mut ics = calendar.to_string();

        // Manual injection of CATEGORIES to handle comma separation correctly
        if !self.categories.is_empty() {
            // 1. Escape commas inside tag names, but join tags with raw commas
            let escaped_cats: Vec<String> = self
                .categories
                .iter()
                .map(|c| c.replace(',', "\\,"))
                .collect();
            let cat_line = format!("CATEGORIES:{}", escaped_cats.join(","));

            // 2. Insert before END:VTODO
            // We assume standard formatting where END:VTODO is at the end
            if let Some(idx) = ics.rfind("END:VTODO") {
                // Simple insertion (Note: strictly this should be folded if >75 chars,
                // but most parsers handle long lines. We keep it simple for now).
                let (start, end) = ics.split_at(idx);
                ics = format!("{}{}\r\n{}", start, cat_line, end);
            }
        }

        ics
    }

    pub fn from_ics(
        raw_ics: &str,
        etag: String,
        href: String,
        calendar_href: String,
    ) -> Result<Self, String> {
        let calendar: Calendar = raw_ics.parse().map_err(|e| format!("Parse: {}", e))?;
        let todo = calendar
            .components
            .iter()
            .find_map(|c| match c {
                CalendarComponent::Todo(t) => Some(t),
                _ => None,
            })
            .ok_or("No VTODO")?;

        let summary = todo.get_summary().unwrap_or("No Title").to_string();
        let description = todo.get_description().unwrap_or("").to_string();
        let uid = todo.get_uid().unwrap_or_default().to_string();

        let status = if let Some(prop) = todo.properties().get("STATUS") {
            match prop.value().trim().to_uppercase().as_str() {
                "COMPLETED" => TaskStatus::Completed,
                "IN-PROCESS" => TaskStatus::InProcess,
                "CANCELLED" => TaskStatus::Cancelled,
                _ => TaskStatus::NeedsAction,
            }
        } else {
            TaskStatus::NeedsAction
        };
        let priority = todo
            .properties()
            .get("PRIORITY")
            .and_then(|p| p.value().parse::<u8>().ok())
            .unwrap_or(0);

        let due = todo.properties().get("DUE").and_then(|p| {
            let val = p.value();
            if val.len() == 8 {
                NaiveDate::parse_from_str(val, "%Y%m%d")
                    .ok()
                    .and_then(|d| d.and_hms_opt(23, 59, 59))
                    .map(|d| d.and_utc())
            } else {
                NaiveDateTime::parse_from_str(
                    val,
                    if val.ends_with('Z') {
                        "%Y%m%dT%H%M%SZ"
                    } else {
                        "%Y%m%dT%H%M%S"
                    },
                )
                .ok()
                .map(|d| Utc.from_utc_datetime(&d))
            }
        });

        let rrule = todo
            .properties()
            .get("RRULE")
            .map(|p| p.value().to_string());
        // Duration Parsing
        let parse_dur = |val: &str| -> Option<u32> {
            let mut minutes = 0;
            let mut num_buf = String::new();
            let mut in_time = false;
            for c in val.chars() {
                if c == 'T' {
                    in_time = true;
                } else if c.is_numeric() {
                    num_buf.push(c);
                } else if !num_buf.is_empty() {
                    let n = num_buf.parse::<u32>().unwrap_or(0);
                    match c {
                        'D' => minutes += n * 24 * 60,
                        'H' => {
                            if in_time {
                                minutes += n * 60
                            }
                        }
                        'M' => {
                            if in_time {
                                minutes += n
                            }
                        }
                        'W' => minutes += n * 7 * 24 * 60,
                        _ => {}
                    }
                    num_buf.clear();
                }
            }
            if minutes > 0 { Some(minutes) } else { None }
        };

        let mut estimated_duration = todo
            .properties()
            .get("X-ESTIMATED-DURATION")
            .and_then(|p| parse_dur(p.value()));

        if estimated_duration.is_none() {
            estimated_duration = todo
                .properties()
                .get("DURATION")
                .and_then(|p| parse_dur(p.value()));
        }
        let mut categories = Vec::new();
        if let Some(multi_props) = todo.multi_properties().get("CATEGORIES") {
            for prop in multi_props {
                let parts: Vec<String> = prop
                    .value()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                categories.extend(parts);
            }
        }
        if let Some(prop) = todo.properties().get("CATEGORIES") {
            let parts: Vec<String> = prop
                .value()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            categories.extend(parts);
        }
        categories.sort();
        categories.dedup();

        // --- MANUAL RELATED-TO PARSING (Fix for library overwrite issue) ---
        let mut parent_uid = None;
        let mut dependencies = Vec::new();

        // Standard Parse (if lucky)
        let mut related_props = Vec::new();
        if let Some(multi) = todo.multi_properties().get("RELATED-TO") {
            related_props.extend(multi.iter());
        }
        if let Some(single) = todo.properties().get("RELATED-TO") {
            related_props.push(single);
        }

        // Manual Parse (Fallback for lost duplicates)
        // Unfold lines (remove CRLF+Space)
        let unfolded = raw_ics.replace("\r\n ", "").replace("\n ", "");

        for line in unfolded.lines() {
            if line.starts_with("RELATED-TO")
                && let Some((key_part, value)) = line.split_once(':')
            {
                let value = value.trim().to_string();
                let key_upper = key_part.to_uppercase();

                if key_upper.contains("RELTYPE=DEPENDS-ON") {
                    if !dependencies.contains(&value) {
                        dependencies.push(value);
                    }
                } else if !key_upper.contains("RELTYPE=") || key_upper.contains("RELTYPE=PARENT") {
                    // Only set parent if not already found (or overwrite if multiple? RFC says 1 parent)
                    parent_uid = Some(value);
                }
            }
        }

        Ok(Task {
            uid,
            summary,
            description,
            status,
            estimated_duration,
            due,
            priority,
            parent_uid,
            dependencies,
            etag,
            href,
            calendar_href,
            categories,
            depth: 0,
            rrule,
        })
    }
}
