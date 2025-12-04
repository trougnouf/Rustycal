// File: ./src/gui/update/view.rs
use crate::gui::async_ops::*;
use crate::gui::message::Message;
use crate::gui::state::{AppState, GuiApp, SidebarMode};
use crate::gui::update::common::{refresh_filtered_tasks, save_config};
use iced::Task;

pub fn handle(app: &mut GuiApp, message: Message) -> Task<Message> {
    match message {
        Message::TabPressed(shift_held) => {
            if shift_held {
                iced::widget::focus_previous()
            } else {
                iced::widget::focus_next()
            }
        }
        Message::DismissError => {
            app.error_msg = None;
            Task::none()
        }
        Message::ToggleAllCalendars(show_all) => {
            if show_all {
                app.hidden_calendars.clear();
            } else {
                for cal in &app.calendars {
                    if app.active_cal_href.as_ref() != Some(&cal.href) {
                        app.hidden_calendars.insert(cal.href.clone());
                    }
                }
            }
            save_config(app);
            refresh_filtered_tasks(app);
            Task::perform(async { Ok::<(), String>(()) }, |_| Message::Refresh)
        }
        Message::IsolateCalendar(href) => {
            if app.sidebar_mode == SidebarMode::Categories {
                app.sidebar_mode = SidebarMode::Calendars;
            }
            app.active_cal_href = Some(href.clone());
            app.hidden_calendars.clear();
            for cal in &app.calendars {
                if cal.href != href {
                    app.hidden_calendars.insert(cal.href.clone());
                }
            }
            if app.disabled_calendars.contains(&href) {
                app.disabled_calendars.remove(&href);
            }
            save_config(app);
            refresh_filtered_tasks(app);

            if let Some(client) = &app.client {
                if !app.store.calendars.contains_key(&href) {
                    app.loading = true;
                }
                return Task::perform(
                    async_fetch_wrapper(client.clone(), href),
                    Message::TasksRefreshed,
                );
            }
            Task::none()
        }
        Message::SidebarModeChanged(mode) => {
            app.sidebar_mode = mode;
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::CategoryToggled(cat) => {
            if app.selected_categories.contains(&cat) {
                app.selected_categories.remove(&cat);
            } else {
                app.selected_categories.insert(cat);
            }
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::ClearAllTags => {
            app.selected_categories.clear();
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::CategoryMatchModeChanged(val) => {
            app.match_all_categories = val;
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::ToggleHideCompleted(val) => {
            app.hide_completed = val;
            save_config(app);
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::ToggleHideFullyCompletedTags(val) => {
            app.hide_fully_completed_tags = val;
            save_config(app);
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::SelectCalendar(href) => {
            if app.sidebar_mode == SidebarMode::Categories {
                app.sidebar_mode = SidebarMode::Calendars;
            }
            app.active_cal_href = Some(href.clone());
            if app.hidden_calendars.contains(&href) {
                app.hidden_calendars.remove(&href);
                save_config(app);
            }
            refresh_filtered_tasks(app);
            if let Some(client) = &app.client {
                if !app.store.calendars.contains_key(&href) {
                    app.loading = true;
                }
                return Task::perform(
                    async_fetch_wrapper(client.clone(), href),
                    Message::TasksRefreshed,
                );
            }
            Task::none()
        }
        Message::ToggleCalendarDisabled(href, is_disabled) => {
            if is_disabled {
                app.disabled_calendars.insert(href.clone());
                if app.active_cal_href.as_ref() == Some(&href) {
                    app.active_cal_href = None;
                }
            } else {
                app.disabled_calendars.remove(&href);
            }
            save_config(app);
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::ToggleCalendarVisibility(href, is_visible) => {
            if !is_visible && app.active_cal_href.as_ref() == Some(&href) {
                return Task::none();
            }
            if is_visible {
                app.hidden_calendars.remove(&href);
            } else {
                app.hidden_calendars.insert(href);
            }
            save_config(app);
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::SearchChanged(val) => {
            app.search_value = val;
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::SetMinDuration(val) => {
            app.filter_min_duration = val;
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::SetMaxDuration(val) => {
            app.filter_max_duration = val;
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::ToggleIncludeUnsetDuration(val) => {
            app.filter_include_unset_duration = val;
            refresh_filtered_tasks(app);
            Task::none()
        }
        Message::ToggleDetails(uid) => {
            if app.expanded_tasks.contains(&uid) {
                app.expanded_tasks.remove(&uid);
            } else {
                app.expanded_tasks.insert(uid);
            }
            Task::none()
        }
        Message::OpenHelp => {
            app.state = AppState::Help;
            Task::none()
        }
        Message::CloseHelp => {
            app.state = AppState::Active;
            Task::none()
        }
        _ => Task::none(),
    }
}
