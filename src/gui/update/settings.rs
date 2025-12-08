// File: src/gui/update/settings.rs
use crate::cache::Cache;
use crate::config::Config;
use crate::gui::async_ops::*;
use crate::gui::message::Message;
use crate::gui::state::{AppState, GuiApp};
use crate::gui::update::common::{refresh_filtered_tasks, save_config};
use crate::storage::{LOCAL_CALENDAR_HREF, LOCAL_CALENDAR_NAME, LocalStorage};
use iced::Task;

pub fn handle(app: &mut GuiApp, message: Message) -> Task<Message> {
    match message {
        Message::ConfigLoaded(Ok(config)) => {
            app.hidden_calendars = config.hidden_calendars.clone().into_iter().collect();
            app.disabled_calendars = config.disabled_calendars.clone().into_iter().collect();
            app.sort_cutoff_months = config.sort_cutoff_months;
            app.ob_sort_months_input = match config.sort_cutoff_months {
                Some(m) => m.to_string(),
                None => "".to_string(),
            };
            app.ob_insecure = config.allow_insecure_certs;
            app.tag_aliases = config.tag_aliases.clone();
            app.hide_completed = config.hide_completed;
            app.hide_fully_completed_tags = config.hide_fully_completed_tags;

            app.ob_url = config.url.clone();
            app.ob_user = config.username.clone();
            app.ob_pass = config.password.clone();
            app.ob_default_cal = config.default_calendar.clone();

            let mut cached_cals = Cache::load_calendars().unwrap_or_default();

            if !cached_cals.iter().any(|c| c.href == LOCAL_CALENDAR_HREF) {
                cached_cals.push(crate::model::CalendarListEntry {
                    name: LOCAL_CALENDAR_NAME.to_string(),
                    href: LOCAL_CALENDAR_HREF.to_string(),
                    color: None,
                });
            }
            app.calendars = cached_cals;

            app.store.clear();

            if let Ok(local_tasks) = LocalStorage::load() {
                app.store
                    .insert(LOCAL_CALENDAR_HREF.to_string(), local_tasks);
            }

            for cal in &app.calendars {
                if cal.href != LOCAL_CALENDAR_HREF
                    && let Ok((tasks, _)) = Cache::load(&cal.href)
                {
                    app.store.insert(cal.href.clone(), tasks);
                }
            }

            // --- Set Active Calendar (with new unhide logic) ---
            let mut target_href = None;
            if let Some(def) = &app.ob_default_cal
                && let Some(cal) = app
                    .calendars
                    .iter()
                    .find(|c| c.name == *def || c.href == *def)
            {
                // Unhide the default calendar if it was hidden
                if app.hidden_calendars.contains(&cal.href) {
                    app.hidden_calendars.remove(&cal.href);
                }
                target_href = Some(cal.href.clone());
            }

            if target_href.is_none() {
                target_href = Some(LOCAL_CALENDAR_HREF.to_string());
            }
            app.active_cal_href = target_href;

            refresh_filtered_tasks(app);
            app.state = AppState::Active;
            app.loading = true;
            Task::perform(connect_and_fetch_wrapper(config), Message::Loaded)
        }
        Message::ConfigLoaded(Err(_)) => {
            app.state = AppState::Onboarding;
            Task::none()
        }
        Message::ObUrlChanged(v) => {
            app.ob_url = v;
            Task::none()
        }
        Message::ObUserChanged(v) => {
            app.ob_user = v;
            Task::none()
        }
        Message::ObPassChanged(v) => {
            app.ob_pass = v;
            Task::none()
        }
        Message::ObDefaultCalChanged(v) => {
            app.ob_default_cal = Some(v);
            Task::none()
        }
        Message::ObInsecureToggled(val) => {
            app.ob_insecure = val;
            Task::none()
        }
        Message::ObSubmit => {
            if app.ob_sort_months_input.trim().is_empty() {
                app.sort_cutoff_months = None;
            } else if let Ok(n) = app.ob_sort_months_input.trim().parse::<u32>() {
                app.sort_cutoff_months = Some(n);
            }

            let mut config_to_save = Config::load().unwrap_or_else(|_| Config {
                url: String::new(),
                username: String::new(),
                password: String::new(),
                default_calendar: None,
                allow_insecure_certs: false,
                hidden_calendars: Vec::new(),
                disabled_calendars: Vec::new(),
                hide_completed: app.hide_completed,
                hide_fully_completed_tags: app.hide_fully_completed_tags,
                tag_aliases: app.tag_aliases.clone(),
                sort_cutoff_months: Some(6),
            });

            config_to_save.url = app.ob_url.clone();
            config_to_save.username = app.ob_user.clone();
            config_to_save.password = app.ob_pass.clone();
            config_to_save.default_calendar = app.ob_default_cal.clone();
            config_to_save.allow_insecure_certs = app.ob_insecure;
            config_to_save.hidden_calendars = app.hidden_calendars.iter().cloned().collect();
            config_to_save.disabled_calendars = app.disabled_calendars.iter().cloned().collect();
            config_to_save.hide_completed = app.hide_completed;
            config_to_save.hide_fully_completed_tags = app.hide_fully_completed_tags;
            config_to_save.tag_aliases = app.tag_aliases.clone();
            config_to_save.sort_cutoff_months = app.sort_cutoff_months;

            let _ = config_to_save.save();

            app.state = AppState::Loading;
            app.error_msg = Some("Connecting...".to_string());

            Task::perform(connect_and_fetch_wrapper(config_to_save), Message::Loaded)
        }
        Message::OpenSettings => {
            if let Ok(cfg) = Config::load() {
                app.ob_url = cfg.url;
                app.ob_user = cfg.username;
                app.ob_pass = cfg.password;
                app.ob_default_cal = cfg.default_calendar;
                app.hide_completed = cfg.hide_completed;
                app.hide_fully_completed_tags = cfg.hide_fully_completed_tags;
                app.ob_insecure = cfg.allow_insecure_certs;
                app.hidden_calendars = cfg.hidden_calendars.into_iter().collect();
                app.tag_aliases = cfg.tag_aliases;
                app.sort_cutoff_months = cfg.sort_cutoff_months;
                app.ob_sort_months_input = match cfg.sort_cutoff_months {
                    Some(m) => m.to_string(),
                    None => "".to_string(),
                };
            }
            app.state = AppState::Settings;
            Task::none()
        }
        Message::CancelSettings => {
            app.state = AppState::Active;
            Task::none()
        }
        Message::ObSubmitOffline => {
            app.ob_url.clear();
            app.ob_user.clear();
            app.ob_pass.clear();

            let config_to_save = Config {
                url: String::new(),
                username: String::new(),
                password: String::new(),
                default_calendar: None,
                allow_insecure_certs: false,
                hidden_calendars: Vec::new(),
                disabled_calendars: Vec::new(),
                hide_completed: app.hide_completed,
                hide_fully_completed_tags: app.hide_fully_completed_tags,
                tag_aliases: app.tag_aliases.clone(),
                sort_cutoff_months: app.sort_cutoff_months,
            };

            let _ = config_to_save.save();

            app.state = AppState::Loading;
            Task::perform(connect_and_fetch_wrapper(config_to_save), Message::Loaded)
        }
        Message::AliasKeyInput(v) => {
            app.alias_input_key = v;
            Task::none()
        }
        Message::AliasValueInput(v) => {
            app.alias_input_values = v;
            Task::none()
        }
        Message::AddAlias => {
            if !app.alias_input_key.is_empty() && !app.alias_input_values.is_empty() {
                let tags: Vec<String> = app
                    .alias_input_values
                    .split(',')
                    .map(|s| s.trim().trim_start_matches('#').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                if !tags.is_empty() {
                    let key = app
                        .alias_input_key
                        .trim()
                        .trim_start_matches('#')
                        .to_string();
                    app.tag_aliases.insert(key, tags);
                    app.alias_input_key.clear();
                    app.alias_input_values.clear();
                    save_config(app);
                }
            }
            Task::none()
        }
        Message::RemoveAlias(key) => {
            app.tag_aliases.remove(&key);
            save_config(app);
            Task::none()
        }
        Message::ObSortMonthsChanged(val) => {
            if val.is_empty() || val.chars().all(|c| c.is_numeric()) {
                app.ob_sort_months_input = val;
            }
            Task::none()
        }
        _ => Task::none(),
    }
}
