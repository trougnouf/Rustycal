// File: ./src/gui/view/sidebar.rs
// Sidebar logic extracted from view.rs
use crate::gui::icon;
use crate::gui::message::Message;
use crate::gui::state::GuiApp;
use crate::store::UNCATEGORIZED_ID;
use iced::widget::{Rule, button, checkbox, column, container, row, text, toggler};
use iced::{Color, Element, Length};

pub fn view_sidebar_calendars(app: &GuiApp) -> Element<'_, Message> {
    // 1. Calculate "Select All" state
    let are_all_visible = app
        .calendars
        .iter()
        .filter(|c| !app.disabled_calendars.contains(&c.href))
        .all(|c| !app.hidden_calendars.contains(&c.href));

    let toggle_all = toggler(are_all_visible)
        .label("Show All")
        .text_size(12) // Tiny bit smaller
        .text_alignment(iced::alignment::Horizontal::Left)
        .spacing(10)
        .width(Length::Fill)
        .on_toggle(Message::ToggleAllCalendars);

    // Wrap in container for padding
    let toggle_container = container(toggle_all).padding(5);

    let list = column(
        app.calendars
            .iter()
            .filter(|c| !app.disabled_calendars.contains(&c.href)) // Filter directly here
            .map(|cal| {
                let is_visible = !app.hidden_calendars.contains(&cal.href);
                let is_target = app.active_cal_href.as_ref() == Some(&cal.href);

                let check = checkbox("", is_visible)
                    .on_toggle(move |v| Message::ToggleCalendarVisibility(cal.href.clone(), v));

                let mut label = button(text(&cal.name).size(16))
                    .width(Length::Fill)
                    .padding(10)
                    .on_press(Message::SelectCalendar(cal.href.clone()));

                label = if is_target {
                    label.style(button::primary)
                } else {
                    label.style(button::text)
                };

                let focus_btn = button(icon::icon(icon::ARROW_RIGHT).size(14))
                    .style(button::text)
                    .padding(10)
                    .on_press(Message::IsolateCalendar(cal.href.clone()));

                row![check, label, focus_btn] // Added focus_btn
                    .spacing(2)
                    .align_y(iced::Alignment::Center)
                    .into()
            })
            .collect::<Vec<_>>(),
    )
    .spacing(5)
    .width(Length::Fill);

    column![toggle_container, list].spacing(5).into()
}

// Helper struct for duration logic
#[derive(Debug, Clone, PartialEq, Eq)]
struct DurationOpt(Option<u32>, String);

impl std::fmt::Display for DurationOpt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.1)
    }
}

// Formatting Helper
fn format_mins(m: u32) -> String {
    if m >= 525600 {
        format!("{}y", m / 525600)
    } else if m >= 43200 {
        format!("{}mo", m / 43200)
    } else if m >= 10080 {
        format!("{}w", m / 10080)
    } else if m >= 1440 {
        format!("{}d", m / 1440)
    } else if m >= 60 {
        format!("{}h", m / 60)
    } else {
        format!("{}m", m)
    }
}

pub fn view_sidebar_categories(app: &GuiApp) -> Element<'_, Message> {
    // 1. Existing Category Logic
    let all_cats = app.store.get_all_categories(
        app.hide_completed,
        app.hide_fully_completed_tags,
        &app.selected_categories,
        &app.hidden_calendars,
    );

    let logic_text = if app.match_all_categories {
        "Match: AND"
    } else {
        "Match: OR"
    };
    let logic_btn = button(text(logic_text).size(12))
        .style(button::secondary)
        .padding(5)
        .on_press(Message::CategoryMatchModeChanged(!app.match_all_categories));

    let header = row![
        text("Filter Tags")
            .size(14)
            .color(Color::from_rgb(0.7, 0.7, 0.7)),
        iced::widget::horizontal_space(),
        logic_btn
    ]
    .align_y(iced::Alignment::Center)
    .padding(iced::Padding {
        right: 15.0,
        ..Default::default()
    });

    let tags_list: Element<'_, Message> = if all_cats.is_empty() {
        column![
            header,
            text("No tags found")
                .size(14)
                .color(Color::from_rgb(0.5, 0.5, 0.5))
        ]
        .spacing(10)
        .into()
    } else {
        let list = column(
            all_cats
                .into_iter()
                .map(|cat| {
                    let is_selected = app.selected_categories.contains(&cat);
                    let cat_clone = cat.clone();
                    let display_name = if cat == UNCATEGORIZED_ID {
                        "Uncategorized".to_string()
                    } else {
                        format!("#{}", cat)
                    };

                    checkbox(display_name, is_selected)
                        .size(18)
                        .text_size(16)
                        .on_toggle(move |_| Message::CategoryToggled(cat_clone.clone()))
                        .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(5);
        column![header, list].spacing(10).into()
    };

    // 2. Dynamic Duration Filter Section
    let mut dur_set = std::collections::HashSet::new();
    // Scan ALL tasks
    for tasks in app.store.calendars.values() {
        for t in tasks {
            if let Some(d) = t.estimated_duration {
                dur_set.insert(d);
            }
        }
    }
    let mut sorted_durs: Vec<u32> = dur_set.into_iter().collect();
    sorted_durs.sort();

    // Build Options
    let mut opts = vec![DurationOpt(None, "Any".to_string())];
    for d in sorted_durs {
        opts.push(DurationOpt(Some(d), format_mins(d)));
    }

    // Determine Current Selection (Robust matching)
    let current_min = opts
        .iter()
        .find(|o| o.0 == app.filter_min_duration)
        .cloned()
        .unwrap_or_else(|| opts[0].clone());

    let current_max = opts
        .iter()
        .find(|o| o.0 == app.filter_max_duration)
        .cloned()
        .unwrap_or_else(|| opts[0].clone());

    let dur_filters = column![
        Rule::horizontal(1),
        text("Filter Duration")
            .size(14)
            .color(Color::from_rgb(0.7, 0.7, 0.7)),
        row![
            text("Min:").size(12).width(30),
            iced::widget::pick_list(opts.clone(), Some(current_min), |o| {
                Message::SetMinDuration(o.0)
            })
            .text_size(12)
            .padding(5)
            .width(Length::Fill)
        ]
        .spacing(5)
        .align_y(iced::Alignment::Center),
        row![
            text("Max:").size(12).width(30),
            iced::widget::pick_list(opts, Some(current_max), |o| Message::SetMaxDuration(o.0))
                .text_size(12)
                .padding(5)
                .width(Length::Fill)
        ]
        .spacing(5)
        .align_y(iced::Alignment::Center),
        checkbox("Include Unset", app.filter_include_unset_duration)
            .text_size(12)
            .size(16)
            .on_toggle(Message::ToggleIncludeUnsetDuration)
    ]
    .spacing(8)
    .padding(iced::Padding {
        top: 10.0,
        ..Default::default()
    });

    // Combine Tag List + Duration Filters
    column![tags_list, dur_filters].spacing(10).into()
}
