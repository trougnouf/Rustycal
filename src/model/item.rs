// File: ./src/model/item.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarListEntry {
    pub name: String,
    pub href: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    NeedsAction,
    InProcess,
    Completed,
    Cancelled,
}

impl TaskStatus {
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

// A struct to hold raw ICS properties we don't natively support yet
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawProperty {
    pub key: String,
    pub value: String,
    pub params: Vec<(String, String)>, // e.g. "CN" -> "John Doe"
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub status: TaskStatus,
    pub estimated_duration: Option<u32>,
    pub due: Option<DateTime<Utc>>,
    pub dtstart: Option<DateTime<Utc>>,
    pub priority: u8,
    pub parent_uid: Option<String>,
    pub dependencies: Vec<String>,
    pub etag: String,
    pub href: String,
    pub calendar_href: String,
    pub categories: Vec<String>,
    pub depth: usize,
    pub rrule: Option<String>,

    // The bucket for everything else (Location, Attendees, etc.)
    pub unmapped_properties: Vec<RawProperty>,
}

impl Task {
    pub fn new(input: &str, aliases: &HashMap<String, Vec<String>>) -> Self {
        let mut task = Self {
            uid: Uuid::new_v4().to_string(),
            summary: String::new(),
            description: String::new(),
            status: TaskStatus::NeedsAction,
            estimated_duration: None,
            due: None,
            dtstart: None,
            priority: 0,
            parent_uid: None,
            dependencies: Vec::new(),
            etag: String::new(),
            href: String::new(),
            calendar_href: String::new(),
            categories: Vec::new(),
            depth: 0,
            rrule: None,
            unmapped_properties: Vec::new(), // Init empty
        };
        // Use the parser module
        task.apply_smart_input(input, aliases);
        task
    }

    pub fn compare_with_cutoff(&self, other: &Self, cutoff: Option<DateTime<Utc>>) -> Ordering {
        // 1. Sort by Status Priority
        fn status_prio(s: TaskStatus) -> u8 {
            match s {
                TaskStatus::InProcess => 0,
                TaskStatus::NeedsAction => 1,
                TaskStatus::Completed => 2,
                TaskStatus::Cancelled => 3,
            }
        }

        let s1 = status_prio(self.status);
        let s2 = status_prio(other.status);
        if s1 != s2 {
            return s1.cmp(&s2);
        }

        // 2. Future Start Date
        let now = Utc::now();
        let self_future = self.dtstart.map(|d| d > now).unwrap_or(false);
        let other_future = other.dtstart.map(|d| d > now).unwrap_or(false);

        match (self_future, other_future) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            _ => {}
        }

        // 3. Cutoff Window
        let is_in_window = |t: &Task| -> bool {
            match (t.due, cutoff) {
                (Some(d), Some(limit)) => d <= limit,
                (Some(_), None) => true,
                (None, _) => false,
            }
        };

        let self_in = is_in_window(self);
        let other_in = is_in_window(other);

        match (self_in, other_in) {
            (true, true) => {
                if self.due != other.due {
                    return self.due.cmp(&other.due);
                }
            }
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (false, false) => {}
        }

        // 4. Priority
        let p1 = if self.priority == 0 { 5 } else { self.priority };
        let p2 = if other.priority == 0 {
            5
        } else {
            other.priority
        };

        if p1 != p2 {
            return p1.cmp(&p2);
        }

        match (self.due, other.due) {
            (Some(d1), Some(d2)) => {
                if d1 != d2 {
                    return d1.cmp(&d2);
                }
            }
            (Some(_), None) => return Ordering::Less,
            (None, Some(_)) => return Ordering::Greater,
            _ => {}
        }

        self.summary.cmp(&other.summary)
    }

    pub fn organize_hierarchy(mut tasks: Vec<Task>, cutoff: Option<DateTime<Utc>>) -> Vec<Task> {
        let present_uids: HashSet<String> = tasks.iter().map(|t| t.uid.clone()).collect();
        let mut children_map: HashMap<String, Vec<Task>> = HashMap::new();
        let mut roots: Vec<Task> = Vec::new();

        tasks.sort_by(|a, b| a.compare_with_cutoff(b, cutoff));

        for mut task in tasks {
            let is_orphan = match &task.parent_uid {
                Some(p_uid) => !present_uids.contains(p_uid),
                None => true,
            };

            if is_orphan {
                if task.parent_uid.is_some() {
                    task.depth = 0;
                }
                roots.push(task);
            } else {
                let p_uid = task.parent_uid.as_ref().unwrap().clone();
                children_map.entry(p_uid).or_default().push(task);
            }
        }

        let mut result = Vec::new();
        for root in roots {
            Self::append_task_and_children(&root, &mut result, &children_map, 0);
        }
        result
    }

    fn append_task_and_children(
        task: &Task,
        result: &mut Vec<Task>,
        map: &HashMap<String, Vec<Task>>,
        depth: usize,
    ) {
        let mut t = task.clone();
        t.depth = depth;
        result.push(t);
        if let Some(children) = map.get(&task.uid) {
            for child in children {
                Self::append_task_and_children(child, result, map, depth + 1);
            }
        }
    }
}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        self.compare_with_cutoff(other, None)
    }
}
impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
