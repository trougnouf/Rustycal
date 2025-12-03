// File: ./src/tui/handlers.rs
// New file: Encapsulates Key Input and Event Handling
use crate::model::Task;
use crate::storage::LOCAL_CALENDAR_HREF;
use crate::tui::action::{Action, AppEvent, SidebarMode};
use crate::tui::state::{AppState, Focus, InputMode};

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::Sender;

pub fn handle_app_event(state: &mut AppState, event: AppEvent, default_cal: &Option<String>) {
    match event {
        AppEvent::Status(s) => state.message = s,
        AppEvent::Error(s) => {
            state.message = format!("Error: {}", s);
            state.loading = false;
        }
        AppEvent::CalendarsLoaded(cals) => {
            state.calendars = cals;
            if let Some(def) = default_cal
                && let Some(found) = state
                    .calendars
                    .iter()
                    .find(|c| c.name == *def || c.href == *def)
            {
                state.active_cal_href = Some(found.href.clone());
            }
            if state.active_cal_href.is_none() {
                state.active_cal_href = Some(LOCAL_CALENDAR_HREF.to_string());
            }
            state.refresh_filtered_view();
        }
        AppEvent::TasksLoaded(results) => {
            for (href, tasks) in results {
                state.store.insert(href.clone(), tasks.clone());
            }
            state.refresh_filtered_view();
            state.loading = false;
        }
    }
}

pub async fn handle_key_event(
    key: KeyEvent,
    state: &mut AppState,
    action_tx: &Sender<Action>,
) -> Option<Action> {
    match state.mode {
        InputMode::Creating => match key.code {
            KeyCode::Enter => {
                if !state.input_buffer.is_empty() {
                    let summary = state.input_buffer.clone();
                    let target_href = state
                        .active_cal_href
                        .clone()
                        .or_else(|| state.calendars.first().map(|c| c.href.clone()));
                    if let Some(href) = target_href {
                        let mut task = Task::new(&summary, &state.tag_aliases);
                        let new_uid = task.uid.clone();
                        task.calendar_href = href.clone();

                        if let Some(p_uid) = &state.creating_child_of {
                            task.parent_uid = Some(p_uid.clone());
                        }

                        if let Some(list) = state.store.calendars.get_mut(&href) {
                            list.push(task.clone());
                        }
                        state.refresh_filtered_view();

                        if let Some(idx) = state.tasks.iter().position(|t| t.uid == new_uid) {
                            state.list_state.select(Some(idx));
                        }

                        state.mode = InputMode::Normal;
                        state.reset_input();
                        state.creating_child_of = None;
                        return Some(Action::CreateTask(task));
                    }
                    state.mode = InputMode::Normal;
                    state.reset_input();
                    state.creating_child_of = None;
                }
            }
            KeyCode::Esc => {
                state.mode = InputMode::Normal;
                state.reset_input();
                state.creating_child_of = None;
            }
            KeyCode::Char(c) => state.enter_char(c),
            KeyCode::Backspace => state.delete_char(),
            _ => {}
        },
        InputMode::Editing => match key.code {
            KeyCode::Enter => {
                if let Some(idx) = state.editing_index
                    && let Some(view_task) = state.tasks.get(idx).cloned()
                {
                    let cal_href = view_task.calendar_href.clone();
                    if let Some(list) = state.store.calendars.get_mut(&cal_href)
                        && let Some(t) = list.iter_mut().find(|t| t.uid == view_task.uid)
                    {
                        t.apply_smart_input(&state.input_buffer, &state.tag_aliases);
                        let t_clone = t.clone();
                        state.refresh_filtered_view();
                        state.mode = InputMode::Normal;
                        state.reset_input();
                        state.editing_index = None;
                        return Some(Action::UpdateTask(t_clone));
                    }
                    state.refresh_filtered_view();
                }
                state.mode = InputMode::Normal;
                state.reset_input();
                state.editing_index = None;
            }
            KeyCode::Esc => {
                state.mode = InputMode::Normal;
                state.reset_input();
                state.editing_index = None;
            }
            KeyCode::Char(c) => state.enter_char(c),
            KeyCode::Backspace => state.delete_char(),
            KeyCode::Left => state.move_cursor_left(),
            KeyCode::Right => state.move_cursor_right(),
            _ => {}
        },
        InputMode::EditingDescription => match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(crossterm::event::KeyModifiers::ALT)
                    || key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::SHIFT)
                {
                    state.enter_char('\n');
                } else {
                    if let Some(idx) = state.editing_index
                        && let Some(view_task) = state.tasks.get(idx).cloned()
                    {
                        let cal_href = view_task.calendar_href.clone();
                        if let Some(list) = state.store.calendars.get_mut(&cal_href)
                            && let Some(t) = list.iter_mut().find(|t| t.uid == view_task.uid)
                        {
                            t.description = state.input_buffer.clone();
                            let t_clone = t.clone();
                            state.refresh_filtered_view();
                            state.mode = InputMode::Normal;
                            state.reset_input();
                            state.editing_index = None;
                            return Some(Action::UpdateTask(t_clone));
                        }
                        state.refresh_filtered_view();
                    }
                    state.mode = InputMode::Normal;
                    state.reset_input();
                    state.editing_index = None;
                }
            }
            KeyCode::Esc => {
                state.mode = InputMode::Normal;
                state.reset_input();
                state.editing_index = None;
            }
            KeyCode::Char(c) => state.enter_char(c),
            KeyCode::Backspace => state.delete_char(),
            KeyCode::Left => state.move_cursor_left(),
            KeyCode::Right => state.move_cursor_right(),
            _ => {}
        },
        InputMode::Searching => match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                state.mode = InputMode::Normal;
            }
            KeyCode::Left => state.move_cursor_left(),
            KeyCode::Right => state.move_cursor_right(),
            KeyCode::Char(c) => {
                state.enter_char(c);
                state.refresh_filtered_view();
            }
            KeyCode::Backspace => {
                state.delete_char();
                state.refresh_filtered_view();
            }
            _ => {}
        },
        InputMode::Moving => match key.code {
            KeyCode::Esc => {
                state.mode = InputMode::Normal;
                state.message = String::new();
            }
            KeyCode::Down | KeyCode::Char('j') => state.next_move_target(),
            KeyCode::Up | KeyCode::Char('k') => state.previous_move_target(),
            KeyCode::Enter => {
                if let Some(task) = state.get_selected_task().cloned()
                    && let Some(idx) = state.move_selection_state.selected()
                    && let Some(target_cal) = state.move_targets.get(idx)
                {
                    let target_href = target_cal.href.clone();
                    if let Some(old_list) = state.store.calendars.get_mut(&task.calendar_href) {
                        old_list.retain(|t| t.uid != task.uid);
                    }
                    let mut new_task_local = task.clone();
                    new_task_local.calendar_href = target_href.clone();
                    state
                        .store
                        .calendars
                        .entry(target_href.clone())
                        .or_default()
                        .push(new_task_local);
                    state.refresh_filtered_view();
                    state.message = "Moving task...".to_string();
                    state.mode = InputMode::Normal;
                    return Some(Action::MoveTask(task, target_href));
                }
                state.mode = InputMode::Normal;
            }
            _ => {}
        },
        InputMode::Exporting => match key.code {
            KeyCode::Esc => {
                state.mode = InputMode::Normal;
                state.message = String::new();
            }
            KeyCode::Down | KeyCode::Char('j') => state.next_export_target(),
            KeyCode::Up | KeyCode::Char('k') => state.previous_export_target(),
            KeyCode::Enter => {
                if let Some(idx) = state.export_selection_state.selected()
                    && let Some(target) = state.export_targets.get(idx)
                {
                    let href = target.href.clone();
                    state.mode = InputMode::Normal;
                    return Some(Action::MigrateLocal(href));
                }
            }
            _ => {}
        },
        InputMode::Normal => match key.code {
            KeyCode::Char('?') => {
                state.show_full_help = !state.show_full_help;
            }
            KeyCode::Char('s') => {
                if state.active_focus == Focus::Main
                    && let Some(task) = state.get_selected_task().cloned()
                {
                    let href = task.calendar_href.clone();
                    if let Some(list) = state.store.calendars.get_mut(&href)
                        && let Some(t) = list.iter_mut().find(|t| t.uid == task.uid)
                    {
                        if t.status == crate::model::TaskStatus::InProcess {
                            t.status = crate::model::TaskStatus::NeedsAction;
                        } else {
                            t.status = crate::model::TaskStatus::InProcess;
                        }
                    }
                    state.refresh_filtered_view();
                    return Some(Action::MarkInProcess(task));
                }
            }
            KeyCode::Char('x') => {
                if state.active_focus == Focus::Main
                    && let Some(task) = state.get_selected_task().cloned()
                {
                    let href = task.calendar_href.clone();
                    if let Some(list) = state.store.calendars.get_mut(&href)
                        && let Some(t) = list.iter_mut().find(|t| t.uid == task.uid)
                    {
                        if t.status == crate::model::TaskStatus::Cancelled {
                            t.status = crate::model::TaskStatus::NeedsAction;
                        } else {
                            t.status = crate::model::TaskStatus::Cancelled;
                        }
                    }
                    state.refresh_filtered_view();
                    return Some(Action::MarkCancelled(task));
                }
            }
            KeyCode::Char('q') if state.mode == InputMode::Normal => {
                return Some(Action::Quit);
            }
            KeyCode::Esc => {
                state.reset_input();
                state.refresh_filtered_view();
                state.yanked_uid = None;
            }
            KeyCode::Char('c') => {
                if let Some(parent_uid) = &state.yanked_uid
                    && let Some(view_task) = state.get_selected_task().cloned()
                {
                    if view_task.uid == *parent_uid {
                        state.message = "Cannot be child of self!".to_string();
                    } else {
                        let href = view_task.calendar_href.clone();
                        if let Some(list) = state.store.calendars.get_mut(&href)
                            && let Some(t) = list.iter_mut().find(|t| t.uid == view_task.uid)
                        {
                            t.parent_uid = Some(parent_uid.clone());
                            let t_clone = t.clone();
                            state.refresh_filtered_view();
                            return Some(Action::UpdateTask(t_clone));
                        }
                        state.refresh_filtered_view();
                    }
                }
            }
            KeyCode::Char('C') => {
                if state.active_focus == Focus::Main
                    && let Some(task) = state.get_selected_task().cloned()
                {
                    state.mode = InputMode::Creating;
                    state.reset_input();
                    state.creating_child_of = Some(task.uid.clone());
                    state.message = format!("New Child of '{}'...", task.summary);
                }
            }
            KeyCode::Char('/') => {
                state.mode = InputMode::Searching;
                state.reset_input();
            }
            KeyCode::Tab => state.toggle_focus(),
            KeyCode::Char('1') => {
                state.sidebar_mode = SidebarMode::Calendars;
                state.refresh_filtered_view();
            }
            KeyCode::Char('2') => {
                state.sidebar_mode = SidebarMode::Categories;
                state.refresh_filtered_view();
            }
            KeyCode::Char('m') => {
                state.match_all_categories = !state.match_all_categories;
                state.refresh_filtered_view();
            }
            KeyCode::Char('H') => {
                state.hide_completed = !state.hide_completed;
                state.refresh_filtered_view();
            }
            KeyCode::Char('M') => {
                if let Some(task) = state.get_selected_task() {
                    let current_href = task.calendar_href.clone();
                    state.move_targets = state
                        .calendars
                        .iter()
                        .filter(|c| {
                            c.href != current_href && !state.disabled_calendars.contains(&c.href)
                        })
                        .cloned()
                        .collect();
                    if !state.move_targets.is_empty() {
                        state.move_selection_state.select(Some(0));
                        state.mode = InputMode::Moving;
                        state.message = "Select a calendar and press Enter.".to_string();
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => state.next(),
            KeyCode::Up | KeyCode::Char('k') => state.previous(),
            KeyCode::PageDown => state.jump_forward(10),
            KeyCode::PageUp => state.jump_backward(10),

            KeyCode::Char(' ') => {
                if state.active_focus == Focus::Sidebar {
                    if state.sidebar_mode == SidebarMode::Calendars
                        && let Some(idx) = state.cal_state.selected()
                    {
                        let filtered = state.get_filtered_calendars();
                        if let Some(cal) = filtered.get(idx) {
                            let href = cal.href.clone();
                            if state.active_cal_href.as_ref() != Some(&href) {
                                if state.hidden_calendars.contains(&href) {
                                    state.hidden_calendars.remove(&href);
                                    let _ = action_tx
                                        .send(Action::ToggleCalendarVisibility(href))
                                        .await;
                                } else {
                                    state.hidden_calendars.insert(href);
                                }
                                state.refresh_filtered_view();
                            }
                        }
                    }
                } else if state.active_focus == Focus::Main
                    && let Some(task) = state.get_selected_task().cloned()
                {
                    let cal_href = task.calendar_href.clone();
                    if let Some(list) = state.store.calendars.get_mut(&cal_href)
                        && let Some(t) = list.iter_mut().find(|t| t.uid == task.uid)
                    {
                        t.status = if t.status == crate::model::TaskStatus::Completed {
                            crate::model::TaskStatus::NeedsAction
                        } else {
                            crate::model::TaskStatus::Completed
                        };
                        let t_flipped = t.clone();
                        state.refresh_filtered_view();
                        return Some(Action::ToggleTask(t_flipped));
                    }
                    state.refresh_filtered_view();
                }
            }

            KeyCode::Enter => {
                if state.active_focus == Focus::Sidebar {
                    match state.sidebar_mode {
                        SidebarMode::Calendars => {
                            if let Some(idx) = state.cal_state.selected() {
                                let filtered = state.get_filtered_calendars();
                                if let Some(cal) = filtered.get(idx) {
                                    let href = cal.href.clone();
                                    state.active_cal_href = Some(href.clone());

                                    if state.hidden_calendars.contains(&href) {
                                        state.hidden_calendars.remove(&href);
                                    }
                                    state.refresh_filtered_view();

                                    if href != LOCAL_CALENDAR_HREF {
                                        return Some(Action::SwitchCalendar(href));
                                    }
                                }
                            }
                        }
                        SidebarMode::Categories => {
                            let cats = state.store.get_all_categories(
                                state.hide_completed,
                                state.hide_fully_completed_tags,
                                &state.selected_categories,
                                &state.hidden_calendars,
                            );
                            if let Some(idx) = state.cal_state.selected()
                                && let Some(c) = cats.get(idx)
                            {
                                if state.selected_categories.contains(c) {
                                    state.selected_categories.remove(c);
                                } else {
                                    state.selected_categories.insert(c.clone());
                                }
                                state.refresh_filtered_view();
                            }
                        }
                    }
                }
            }
            KeyCode::Char('a') => {
                state.mode = InputMode::Creating;
                state.reset_input();
                state.message = "New Task (e.g. 'Buy Milk !1 @tomorrow')...".to_string();
            }
            KeyCode::Char('e') => {
                if state.active_focus == Focus::Main
                    && let Some(smart_str) = state.get_selected_task().map(|t| t.to_smart_string())
                {
                    state.editing_index = state.list_state.selected();
                    state.input_buffer = smart_str;
                    state.cursor_position = state.input_buffer.chars().count();
                    state.mode = InputMode::Editing;
                }
            }
            KeyCode::Char('E') => {
                if state.active_focus == Focus::Main
                    && let Some(d) = state.get_selected_task().map(|t| t.description.clone())
                {
                    state.editing_index = state.list_state.selected();
                    state.input_buffer = d;
                    state.cursor_position = state.input_buffer.chars().count();
                    state.mode = InputMode::EditingDescription;
                }
            }
            KeyCode::Char('d') => {
                if state.active_focus == Focus::Main
                    && let Some(task) = state.get_selected_task().cloned()
                {
                    let href = task.calendar_href.clone();
                    if let Some(list) = state.store.calendars.get_mut(&href) {
                        list.retain(|t| t.uid != task.uid);
                    }
                    state.refresh_filtered_view();
                    return Some(Action::DeleteTask(task));
                }
            }
            KeyCode::Char('+') => {
                if state.active_focus == Focus::Main
                    && let Some(view_task) = state.get_selected_task().cloned()
                {
                    let href = view_task.calendar_href.clone();
                    if let Some(list) = state.store.calendars.get_mut(&href)
                        && let Some(t) = list.iter_mut().find(|t| t.uid == view_task.uid)
                    {
                        let new_prio = match t.priority {
                            0 => 9,
                            9 => 5,
                            5 => 1,
                            1 => 1,
                            _ => 5,
                        };
                        t.priority = new_prio;
                        let t_clone = t.clone();
                        state.refresh_filtered_view();
                        return Some(Action::UpdateTask(t_clone));
                    }
                    state.refresh_filtered_view();
                }
            }
            KeyCode::Char('-') => {
                if state.active_focus == Focus::Main
                    && let Some(view_task) = state.get_selected_task().cloned()
                {
                    let href = view_task.calendar_href.clone();
                    if let Some(list) = state.store.calendars.get_mut(&href)
                        && let Some(t) = list.iter_mut().find(|t| t.uid == view_task.uid)
                    {
                        let new_prio = match t.priority {
                            1 => 5,
                            5 => 9,
                            9 => 0,
                            0 => 0,
                            _ => 0,
                        };
                        t.priority = new_prio;
                        let t_clone = t.clone();
                        state.refresh_filtered_view();
                        return Some(Action::UpdateTask(t_clone));
                    }
                    state.refresh_filtered_view();
                }
            }
            KeyCode::Char('.') | KeyCode::Char('>') => {
                if state.active_focus == Focus::Main
                    && let Some(idx) = state.list_state.selected()
                    && idx > 0
                    && idx < state.tasks.len()
                {
                    let parent_uid = state.tasks[idx - 1].uid.clone();
                    let current_uid = state.tasks[idx].uid.clone();
                    let cal_href = state.tasks[idx].calendar_href.clone();
                    if let Some(list) = state.store.calendars.get_mut(&cal_href)
                        && let Some(t) = list.iter_mut().find(|t| t.uid == current_uid)
                        && t.parent_uid != Some(parent_uid.clone())
                    {
                        t.parent_uid = Some(parent_uid);
                        let t_clone = t.clone();
                        state.refresh_filtered_view();
                        return Some(Action::UpdateTask(t_clone));
                    }
                    state.refresh_filtered_view();
                }
            }
            KeyCode::Char(',') | KeyCode::Char('<') => {
                if state.active_focus == Focus::Main
                    && let Some(view_task) = state.get_selected_task().cloned()
                {
                    let cal_href = view_task.calendar_href.clone();
                    if let Some(list) = state.store.calendars.get_mut(&cal_href)
                        && let Some(t) = list.iter_mut().find(|t| t.uid == view_task.uid)
                        && t.parent_uid.is_some()
                    {
                        t.parent_uid = None;
                        let t_clone = t.clone();
                        state.refresh_filtered_view();
                        return Some(Action::UpdateTask(t_clone));
                    }
                    state.refresh_filtered_view();
                }
            }
            KeyCode::Char('y') => {
                if let Some(t_data) = state
                    .get_selected_task()
                    .map(|t| (t.uid.clone(), t.summary.clone()))
                {
                    state.yanked_uid = Some(t_data.0);
                    state.message = format!("Yanked: {}", t_data.1);
                }
            }
            KeyCode::Char('r') => {
                return Some(Action::Refresh);
            }
            KeyCode::Char('b') => {
                if let Some(yanked) = &state.yanked_uid
                    && let Some(current) = state.get_selected_task()
                {
                    if current.uid == *yanked {
                        state.message = "Cannot depend on self!".to_string();
                    } else {
                        let mut t_clone = current.clone();
                        if !t_clone.dependencies.contains(yanked) {
                            t_clone.dependencies.push(yanked.clone());
                            let href = t_clone.calendar_href.clone();
                            if let Some(list) = state.store.calendars.get_mut(&href)
                                && let Some(t) = list.iter_mut().find(|t| t.uid == t_clone.uid)
                            {
                                t.dependencies.push(yanked.clone());
                            }
                            state.refresh_filtered_view();
                            return Some(Action::UpdateTask(t_clone));
                        }
                    }
                }
            }
            KeyCode::Char('X') => {
                if state.active_cal_href.as_deref() == Some(LOCAL_CALENDAR_HREF) {
                    state.export_targets = state
                        .calendars
                        .iter()
                        .filter(|c| {
                            c.href != LOCAL_CALENDAR_HREF
                                && !state.disabled_calendars.contains(&c.href)
                        })
                        .cloned()
                        .collect();
                    if !state.export_targets.is_empty() {
                        state.export_selection_state.select(Some(0));
                        state.mode = InputMode::Exporting;
                    }
                }
            }
            KeyCode::Char('*') => {
                if state.active_focus == Focus::Sidebar
                    && state.sidebar_mode == SidebarMode::Calendars
                {
                    let enabled_count = state
                        .calendars
                        .iter()
                        .filter(|c| !state.disabled_calendars.contains(&c.href))
                        .count();

                    let visible_count = state
                        .calendars
                        .iter()
                        .filter(|c| {
                            !state.disabled_calendars.contains(&c.href)
                                && !state.hidden_calendars.contains(&c.href)
                        })
                        .count();

                    if visible_count == enabled_count {
                        for cal in &state.calendars {
                            if state.active_cal_href.as_ref() != Some(&cal.href) {
                                state.hidden_calendars.insert(cal.href.clone());
                            }
                        }
                    } else {
                        state.hidden_calendars.clear();
                        let _ = action_tx.send(Action::Refresh).await;
                    }
                    state.refresh_filtered_view();
                }
            }
            KeyCode::Right => {
                if state.active_focus == Focus::Sidebar
                    && state.sidebar_mode == SidebarMode::Calendars
                {
                    if let Some(idx) = state.cal_state.selected() {
                        let filtered = state.get_filtered_calendars();
                        if let Some(cal) = filtered.get(idx) {
                            let href = cal.href.clone();
                            state.active_cal_href = Some(href.clone());

                            state.hidden_calendars.clear();
                            for c in &state.calendars {
                                if c.href != href {
                                    state.hidden_calendars.insert(c.href.clone());
                                }
                            }

                            state.refresh_filtered_view();
                            if href != LOCAL_CALENDAR_HREF {
                                return Some(Action::IsolateCalendar(href));
                            }
                        }
                    }
                } else if state.mode == InputMode::Editing {
                    state.move_cursor_right();
                }
            }
            _ => {}
        },
    }
    None
}
