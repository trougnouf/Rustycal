// File: src/gui/update/network.rs
use crate::cache::Cache;
use crate::config::Config;
use crate::gui::async_ops::*;
use crate::gui::message::Message;
use crate::gui::state::{AppState, GuiApp};
use crate::gui::update::common::{refresh_filtered_tasks, save_config};
use crate::journal::Journal;
use crate::model::CalendarListEntry;
use crate::storage::{LOCAL_CALENDAR_HREF, LOCAL_CALENDAR_NAME};
use iced::Task;

pub fn handle(app: &mut GuiApp, message: Message) -> Task<Message> {
    match message {
        Message::Refresh => {
            app.loading = true;
            app.error_msg = None;

            if app.client.is_some()
                && let Ok(cfg) = Config::load()
            {
                return Task::perform(connect_and_fetch_wrapper(cfg), Message::Loaded);
            }
            Task::none()
        }
        Message::Loaded(Ok((client, mut cals, tasks, mut active, warning))) => {
            app.client = Some(client.clone());

            if let Some(w) = warning {
                app.error_msg = Some(w);
            } else {
                app.error_msg = None;
            }

            app.unsynced_changes = !Journal::load().is_empty();

            let local_entry = CalendarListEntry {
                name: LOCAL_CALENDAR_NAME.to_string(),
                href: LOCAL_CALENDAR_HREF.to_string(),
                color: None,
            };

            if !cals.iter().any(|c| c.href == LOCAL_CALENDAR_HREF) {
                cals.push(local_entry);
            }

            app.calendars = cals.clone();
            app.store.clear();

            if let Ok(local_t) = crate::gui::async_ops::get_runtime()
                .block_on(async { client.get_tasks(LOCAL_CALENDAR_HREF).await })
            {
                app.store.insert(LOCAL_CALENDAR_HREF.to_string(), local_t);
            }

            for cal in &app.calendars {
                if cal.href == LOCAL_CALENDAR_HREF {
                    continue;
                }
                if let Ok((cached_tasks, _)) = Cache::load(&cal.href) {
                    app.store.insert(cal.href.clone(), cached_tasks);
                }
            }

            // Re-validate active calendar against current hidden state
            let mut valid_active = None;
            if let Some(current) = &app.active_cal_href
                && app.calendars.iter().any(|c| c.href == *current)
                && !app.hidden_calendars.contains(current)
            {
                valid_active = Some(current.clone());
            }

            // If the optimistic active calendar was invalid or hidden, check the one returned from network
            if valid_active.is_none()
                && let Some(net_active) = active
                && !app.hidden_calendars.contains(&net_active)
            {
                valid_active = Some(net_active);
            }

            // Fallback
            if valid_active.is_none() {
                valid_active = Some(LOCAL_CALENDAR_HREF.to_string());
            }

            active = valid_active;
            app.active_cal_href = active.clone();

            if let Some(href) = &active
                && href != LOCAL_CALENDAR_HREF
                && app.error_msg.is_none()
            {
                app.store.insert(href.clone(), tasks);
            }

            if let Ok(cfg) = Config::load() {
                app.hide_completed = cfg.hide_completed;
                app.hide_fully_completed_tags = cfg.hide_fully_completed_tags;
                app.tag_aliases = cfg.tag_aliases;
                app.disabled_calendars = cfg.disabled_calendars.into_iter().collect();
            }

            if !app.ob_url.is_empty() {
                save_config(app);
            }

            app.state = AppState::Active;
            refresh_filtered_tasks(app);
            app.loading = false;

            if app.error_msg.is_none() {
                app.loading = true;
                Task::perform(async_fetch_all_wrapper(client, cals), Message::RefreshedAll)
            } else {
                Task::none()
            }
        }
        Message::Loaded(Err(e)) => {
            app.error_msg = Some(format!("Connection Failed: {}", e));
            app.state = AppState::Onboarding;
            app.loading = false;
            Task::none()
        }
        Message::RefreshedAll(Ok(results)) => {
            for (href, tasks) in results {
                app.store.insert(href.clone(), tasks.clone());
            }
            refresh_filtered_tasks(app);
            app.loading = false;
            Task::none()
        }
        Message::RefreshedAll(Err(e)) => {
            app.error_msg = Some(format!("Sync warning: {}", e));
            app.loading = false;
            Task::none()
        }
        Message::TasksRefreshed(Ok((href, tasks))) => {
            app.error_msg = None;
            app.store.insert(href.clone(), tasks.clone());

            if app.active_cal_href.as_deref() == Some(&href) {
                refresh_filtered_tasks(app);
                app.loading = false;
            }
            Task::none()
        }
        Message::TasksRefreshed(Err(e)) => {
            app.error_msg = Some(format!("Fetch: {}", e));
            app.loading = false;
            Task::none()
        }
        Message::SyncSaved(Ok(updated)) => {
            if let Some(tasks) = app.store.calendars.get_mut(&updated.calendar_href) {
                if let Some(idx) = tasks.iter().position(|t| t.uid == updated.uid) {
                    tasks[idx] = updated.clone();
                } else {
                    tasks.push(updated.clone());
                }
                let (_, token) = Cache::load(&updated.calendar_href).unwrap_or((vec![], None));
                let _ = Cache::save(&updated.calendar_href, tasks, token);
            }

            app.unsynced_changes = !Journal::load().is_empty();
            if app.unsynced_changes {
                app.error_msg = Some("Offline: Changes queued.".to_string());
            }
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::SyncSaved(Err(e)) => {
            app.error_msg = Some(format!("Sync Error: {}", e));
            Task::none()
        }
        Message::SyncToggleComplete(boxed_res) => match *boxed_res {
            Ok((updated, created_opt)) => {
                if let Some(tasks) = app.store.calendars.get_mut(&updated.calendar_href) {
                    if let Some(idx) = tasks.iter().position(|t| t.uid == updated.uid) {
                        tasks[idx] = updated.clone();
                    }
                    if let Some(created) = created_opt {
                        tasks.push(created);
                    }
                    let (_, token) = Cache::load(&updated.calendar_href).unwrap_or((vec![], None));
                    let _ = Cache::save(&updated.calendar_href, tasks, token);
                }
                refresh_filtered_tasks(app);
                Task::none()
            }
            Err(e) => {
                app.error_msg = Some(format!("Toggle Error: {}", e));
                Task::none()
            }
        },
        Message::TaskMoved(Ok(new_task)) => {
            // Note: Store update already handled by synchronous wrapper in handlers.rs
            // We just ensure cache is consistent and re-render
            if let Some(list) = app.store.calendars.get_mut(&new_task.calendar_href) {
                if let Some(idx) = list.iter().position(|t| t.uid == new_task.uid) {
                    list[idx] = new_task.clone();
                } else {
                    list.push(new_task.clone());
                }
                let (_, token) = Cache::load(&new_task.calendar_href).unwrap_or((vec![], None));
                let _ = Cache::save(&new_task.calendar_href, list, token);
            }
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::TaskMoved(Err(e)) => {
            app.error_msg = Some(format!("Move failed: {}", e));
            Task::none()
        }
        Message::MigrationComplete(Ok(count)) => {
            app.loading = false;
            app.error_msg = Some(format!("Exported {} tasks successfully.", count));
            if let Some(client) = &app.client {
                app.loading = true;
                return Task::perform(
                    async_fetch_all_wrapper(client.clone(), app.calendars.clone()),
                    Message::RefreshedAll,
                );
            }
            Task::none()
        }
        Message::MigrationComplete(Err(e)) => {
            app.loading = false;
            app.error_msg = Some(format!("Export failed: {}", e));
            Task::none()
        }
        _ => Task::none(),
    }
}
