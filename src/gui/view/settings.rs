// File: ./src/gui/view/settings.rs
// Settings view extracted from view.rs
use crate::gui::icon;
use crate::gui::message::Message;
use crate::gui::state::{AppState, GuiApp};

use iced::widget::{
    Rule, button, checkbox, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Color, Element, Length};

pub fn view_settings(app: &GuiApp) -> Element<'_, Message> {
    let is_settings = matches!(app.state, AppState::Settings);
    let title = text(if is_settings {
        "Settings"
    } else {
        "Welcome to Cfait"
    })
    .size(40);
    let error = if let Some(e) = &app.error_msg {
        text(e).color(Color::from_rgb(1.0, 0.0, 0.0))
    } else {
        text("")
    };

    let cal_names: Vec<String> = app.calendars.iter().map(|c| c.name.clone()).collect();
    let picker: Element<_> = if !cal_names.is_empty() && is_settings {
        column![
            text("Default Calendar:"),
            iced::widget::pick_list(
                cal_names,
                app.ob_default_cal.clone(),
                Message::ObDefaultCalChanged
            )
            .width(Length::Fill)
            .padding(10)
        ]
        .spacing(5)
        .into()
    } else {
        horizontal_space().width(0).into()
    };

    let prefs: Element<'_, Message> = if is_settings {
        std::convert::Into::<Element<'_, Message>>::into(container(
            column![
                std::convert::Into::<Element<'_, Message>>::into(
                    checkbox("Hide Completed Tasks (Everywhere)", app.hide_completed)
                        .on_toggle(Message::ToggleHideCompleted),
                ),
                // Conditional checkbox: only visible when 'Hide Completed Tasks (Everywhere)' is off
                if !app.hide_completed {
                    std::convert::Into::<Element<'_, Message>>::into(
                        checkbox(
                            "Hide Tags containing ONLY completed tasks",
                            app.hide_fully_completed_tags,
                        )
                        .on_toggle(Message::ToggleHideFullyCompletedTags),
                    )
                } else {
                    // Placeholder to keep spacing
                    std::convert::Into::<Element<'_, Message>>::into(horizontal_space().width(0))
                },
            ]
            .spacing(10),
        ))
    } else {
        std::convert::Into::<Element<'_, Message>>::into(horizontal_space().width(0))
    };

    let sorting_ui: Element<_> = if is_settings {
        column![
            text("Sorting Priority Cutoff (Months):"),
            text("(Tasks due within this range are shown first. Blank = All timed first)")
                .size(12)
                .color(Color::from_rgb(0.6, 0.6, 0.6)),
            text_input("6", &app.ob_sort_months_input)
                .on_input(Message::ObSortMonthsChanged)
                .padding(10)
                .width(Length::Fixed(100.0))
        ]
        .spacing(5)
        .into()
    } else {
        horizontal_space().width(0).into()
    };

    // Alias Section
    let aliases_ui: Element<_> = if is_settings {
        let mut list_col = column![text("Tag Aliases").size(20)].spacing(10);

        // Existing Aliases List
        for (key, vals) in &app.tag_aliases {
            let val_str = vals.join(", ");
            let row_item = row![
                text(format!("#{}", key)).width(Length::FillPortion(1)),
                text("->").width(Length::Fixed(20.0)),
                text(val_str).width(Length::FillPortion(2)),
                button(icon::icon(icon::CROSS).size(12))
                    .style(button::danger)
                    .padding(5)
                    .on_press(Message::RemoveAlias(key.clone()))
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center);
            list_col = list_col.push(row_item);
        }

        // Add New Alias Form
        let input_row = row![
            text_input("Alias (#cfait)", &app.alias_input_key)
                .on_input(Message::AliasKeyInput)
                .padding(5)
                .width(Length::FillPortion(1)),
            text_input("Tags (dev, rust)", &app.alias_input_values)
                .on_input(Message::AliasValueInput)
                .padding(5)
                .width(Length::FillPortion(2)),
            button("Add").padding(5).on_press(Message::AddAlias)
        ]
        .spacing(10);

        let area = container(column![list_col, Rule::horizontal(1), input_row].spacing(15))
            .padding(10)
            .style(|_| container::Style {
                border: iced::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: Color::from_rgb(0.3, 0.3, 0.3),
                },
                ..Default::default()
            });

        area.into()
    } else {
        horizontal_space().width(0).into()
    };

    let cal_mgmt_ui: Element<_> = if is_settings && !app.calendars.is_empty() {
        let mut col = column![text("Manage Calendars").size(20)].spacing(10);

        for cal in &app.calendars {
            // Logic inverted: Checkbox checked = Enabled (!Disabled)
            let is_enabled = !app.disabled_calendars.contains(&cal.href);

            let row_content = row![
                checkbox(&cal.name, is_enabled)
                    // When toggled, we send !v because the msg is "ToggleDisabled"
                    .on_toggle(move |v| Message::ToggleCalendarDisabled(cal.href.clone(), !v))
                    .width(Length::Fill)
            ];

            col = col.push(row_content.spacing(10).align_y(iced::Alignment::Center));
        }

        container(col)
            .padding(10)
            .style(|_| container::Style {
                border: iced::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: Color::from_rgb(0.3, 0.3, 0.3),
                },
                ..Default::default()
            })
            .into()
    } else {
        horizontal_space().width(0).into()
    };

    // Initialize the buttons row before using it
    let mut buttons = row![].spacing(10);

    if !is_settings {
        // Onboarding screen
        buttons = buttons.push(
            button("Use Offline Mode")
                .padding(10)
                .style(button::secondary)
                .on_press(Message::ObSubmitOffline),
        );
    }

    if is_settings {
        // Settings screen
        buttons = buttons.push(
            button("Cancel")
                .padding(10)
                .style(button::secondary)
                .on_press(Message::CancelSettings),
        );
    }

    // This button appears on both screens
    buttons = buttons.push(
        button(if is_settings {
            "Save & Connect"
        } else {
            "Connect"
        })
        .padding(10)
        .on_press(Message::ObSubmit),
    );
    let insecure_check = checkbox("Allow Insecure SSL (e.g. self-signed)", app.ob_insecure)
        .on_toggle(Message::ObInsecureToggled)
        .size(16)
        .text_size(14);

    let form = column![
        text("CalDAV Server URL:"),
        text_input("https://...", &app.ob_url)
            .on_input(Message::ObUrlChanged)
            .padding(10),
        text("Username:"),
        text_input("User", &app.ob_user)
            .on_input(Message::ObUserChanged)
            .padding(10),
        text("Password:"),
        text_input("Password", &app.ob_pass)
            .on_input(Message::ObPassChanged)
            .secure(true)
            .padding(10),
        insecure_check,
        picker,
        prefs,
        sorting_ui,
        aliases_ui,
        cal_mgmt_ui,
        buttons
    ]
    .spacing(15)
    .max_width(500);

    let content = column![title, error, form]
        .spacing(20)
        .align_x(iced::Alignment::Center);

    // Wrap in scrollable so buttons are accessible on small screens
    container(scrollable(
        container(content)
            .width(Length::Fill)
            .padding(20)
            .center_x(Length::Fill),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
