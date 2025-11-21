use rustache::client::RustyClient;
use rustache::config::Config;
use rustache::model::{CalendarListEntry, Task as TodoTask};

use iced::widget::{
    Rule, button, checkbox, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Background, Color, Element, Length, Padding, Task, Theme}; // <--- Added Padding
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static TOKIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn main() -> iced::Result {
    let runtime = Runtime::new().expect("Failed to create Tokio runtime");
    TOKIO_RUNTIME
        .set(runtime)
        .expect("Failed to set global runtime");

    iced::application("Rustache", RustacheGui::update, RustacheGui::view)
        .theme(RustacheGui::theme)
        .run_with(RustacheGui::new)
}

struct RustacheGui {
    tasks: Vec<TodoTask>,
    calendars: Vec<CalendarListEntry>,
    active_cal_href: Option<String>,

    input_value: String,
    search_value: String,

    client: Option<RustyClient>,
    loading: bool,
    error_msg: Option<String>,
}

impl Default for RustacheGui {
    fn default() -> Self {
        Self {
            tasks: vec![],
            calendars: vec![],
            active_cal_href: None,
            input_value: String::new(),
            search_value: String::new(),
            client: None,
            loading: true,
            error_msg: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    SearchChanged(String),
    CreateTask,
    ToggleTask(usize, bool),
    SelectCalendar(String),

    Loaded(
        Result<
            (
                RustyClient,
                Vec<CalendarListEntry>,
                Vec<TodoTask>,
                Option<String>,
            ),
            String,
        >,
    ),
    SyncSaved(Result<TodoTask, String>),
    TasksRefreshed(Result<Vec<TodoTask>, String>),
}

impl RustacheGui {
    fn new() -> (Self, Task<Message>) {
        (
            Self::default(),
            Task::perform(connect_and_fetch_wrapper(), Message::Loaded),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SearchChanged(val) => {
                self.search_value = val;
                Task::none()
            }

            Message::Loaded(Ok((client, cals, tasks, active))) => {
                self.client = Some(client);
                self.calendars = cals;
                self.tasks = TodoTask::organize_hierarchy(tasks);
                self.active_cal_href = active;
                self.loading = false;
                Task::none()
            }
            Message::Loaded(Err(e)) => {
                self.error_msg = Some(format!("Connection Failed: {}", e));
                self.loading = false;
                Task::none()
            }

            Message::SyncSaved(Ok(updated_task)) => {
                if let Some(index) = self.tasks.iter().position(|t| t.uid == updated_task.uid) {
                    self.tasks[index] = updated_task;
                    let raw = self.tasks.clone();
                    self.tasks = TodoTask::organize_hierarchy(raw);
                }
                Task::none()
            }
            Message::SyncSaved(Err(e)) => {
                self.error_msg = Some(format!("Sync Error: {}", e));
                Task::none()
            }

            Message::TasksRefreshed(Ok(tasks)) => {
                self.tasks = TodoTask::organize_hierarchy(tasks);
                self.loading = false;
                Task::none()
            }
            Message::TasksRefreshed(Err(e)) => {
                self.error_msg = Some(format!("Fetch Error: {}", e));
                self.loading = false;
                Task::none()
            }

            Message::SelectCalendar(href) => {
                if let Some(client) = &mut self.client {
                    self.loading = true;
                    self.active_cal_href = Some(href.clone());
                    client.set_calendar(&href);
                    return Task::perform(
                        async_fetch_wrapper(client.clone()),
                        Message::TasksRefreshed,
                    );
                }
                Task::none()
            }

            Message::InputChanged(value) => {
                self.input_value = value;
                Task::none()
            }

            Message::CreateTask => {
                if !self.input_value.is_empty() {
                    let new_task = TodoTask::new(&self.input_value);
                    self.tasks.push(new_task.clone());
                    let raw = self.tasks.clone();
                    self.tasks = TodoTask::organize_hierarchy(raw);

                    self.input_value.clear();

                    if let Some(client) = &self.client {
                        return Task::perform(
                            async_create_wrapper(client.clone(), new_task),
                            Message::SyncSaved,
                        );
                    }
                }
                Task::none()
            }

            Message::ToggleTask(index, is_checked) => {
                if let Some(task) = self.tasks.get_mut(index) {
                    task.completed = is_checked;
                    if let Some(client) = &self.client {
                        return Task::perform(
                            async_update_wrapper(client.clone(), task.clone()),
                            Message::SyncSaved,
                        );
                    }
                }
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let sidebar_content = column(
            self.calendars
                .iter()
                .map(|cal| {
                    let is_active = self.active_cal_href.as_ref() == Some(&cal.href);
                    let btn = button(text(&cal.name).size(16))
                        .padding(10)
                        .width(Length::Fill)
                        .on_press(Message::SelectCalendar(cal.href.clone()));

                    if is_active {
                        btn.style(button::primary)
                    } else {
                        btn.style(button::secondary)
                    }
                    .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(10)
        .padding(10);

        let sidebar = container(scrollable(sidebar_content))
            .width(200)
            .height(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style::default()
                    .background(Background::Color(palette.background.weak.color))
            });

        let title_text = if self.loading {
            "Loading..."
        } else {
            "Rustache"
        };

        let search_input = text_input("Search...", &self.search_value)
            .on_input(Message::SearchChanged)
            .padding(5)
            .size(16);

        let input = text_input("Add a task (Buy Milk !1 @tomorrow)...", &self.input_value)
            .on_input(Message::InputChanged)
            .on_submit(Message::CreateTask)
            .padding(10)
            .size(20);

        let is_searching = !self.search_value.is_empty();
        let filtered_tasks: Vec<(usize, &TodoTask)> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                if is_searching {
                    t.summary
                        .to_lowercase()
                        .contains(&self.search_value.to_lowercase())
                } else {
                    true
                }
            })
            .collect();

        let tasks_view: Element<_> = column(
            filtered_tasks
                .into_iter()
                .map(|(real_index, task)| {
                    let color = match task.priority {
                        1..=4 => Color::from_rgb(0.8, 0.2, 0.2),
                        5 => Color::from_rgb(0.8, 0.8, 0.2),
                        _ => Color::WHITE,
                    };

                    let indent_size = if is_searching { 0 } else { task.depth * 20 };
                    let indent = horizontal_space().width(Length::Fixed(indent_size as f32));

                    let summary = text(&task.summary)
                        .size(20)
                        .color(color)
                        .width(Length::Fill); // Takes remaining space

                    let date_content: Element<_> = match task.due {
                        Some(d) => text(d.format("%Y-%m-%d").to_string())
                            .size(14)
                            .color(Color::from_rgb(0.5, 0.5, 0.5))
                            .into(),
                        None => horizontal_space().width(0).into(),
                    };

                    let row_content = row![
                        indent,
                        checkbox("", task.completed)
                            .on_toggle(move |b| Message::ToggleTask(real_index, b)),
                        summary,
                        date_content
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center);

                    container(row_content)
                        // --- FIX: Explicit Padding Struct ---
                        .padding(Padding {
                            top: 5.0,
                            right: 20.0, // Extra right padding for scrollbar
                            bottom: 5.0,
                            left: 5.0,
                        })
                        .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(2)
        .into();

        let main_content = column![
            row![
                text(title_text).size(40),
                horizontal_space(),
                search_input.width(200)
            ]
            .align_y(iced::Alignment::Center),
            input,
            scrollable(tasks_view)
        ]
        .spacing(20)
        .padding(20)
        .max_width(800);

        let layout = row![
            sidebar,
            Rule::vertical(1),
            container(main_content)
                .width(Length::Fill)
                .center_x(Length::Fill)
        ];

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

// --- ASYNC WRAPPERS ---
async fn connect_and_fetch_wrapper() -> Result<
    (
        RustyClient,
        Vec<CalendarListEntry>,
        Vec<TodoTask>,
        Option<String>,
    ),
    String,
> {
    let rt = TOKIO_RUNTIME.get().expect("Runtime not initialized");
    rt.spawn(async { connect_and_fetch().await })
        .await
        .map_err(|e| e.to_string())?
}
async fn async_fetch_wrapper(client: RustyClient) -> Result<Vec<TodoTask>, String> {
    let rt = TOKIO_RUNTIME.get().expect("Runtime not initialized");
    rt.spawn(async move {
        let tasks = client.get_tasks().await.map_err(|e| e.to_string())?;
        Ok(tasks)
    })
    .await
    .map_err(|e| e.to_string())?
}
async fn async_create_wrapper(client: RustyClient, task: TodoTask) -> Result<TodoTask, String> {
    let rt = TOKIO_RUNTIME.get().expect("Runtime not initialized");
    rt.spawn(async move { async_create(client, task).await })
        .await
        .map_err(|e| e.to_string())?
}
async fn async_update_wrapper(client: RustyClient, task: TodoTask) -> Result<TodoTask, String> {
    let rt = TOKIO_RUNTIME.get().expect("Runtime not initialized");
    rt.spawn(async move { async_update(client, task).await })
        .await
        .map_err(|e| e.to_string())?
}

// --- LOGIC ---
async fn connect_and_fetch() -> Result<
    (
        RustyClient,
        Vec<CalendarListEntry>,
        Vec<TodoTask>,
        Option<String>,
    ),
    String,
> {
    let config = Config::load().map_err(|e| e.to_string())?;
    let mut client = RustyClient::new(&config.url, &config.username, &config.password)
        .map_err(|e| e.to_string())?;
    let calendars = client.get_calendars().await.unwrap_or_default();
    let mut active_href = None;

    if let Some(def_cal) = config.default_calendar {
        if let Some(found) = calendars
            .iter()
            .find(|c| c.name == def_cal || c.href == def_cal)
        {
            client.set_calendar(&found.href);
            active_href = Some(found.href.clone());
        } else {
            if let Ok(href) = client.discover_calendar().await {
                active_href = Some(href);
            }
        }
    } else {
        if let Ok(href) = client.discover_calendar().await {
            active_href = Some(href);
        }
    }

    let tasks = client.get_tasks().await.map_err(|e| e.to_string())?;
    Ok((client, calendars, tasks, active_href))
}
async fn async_create(client: RustyClient, mut task: TodoTask) -> Result<TodoTask, String> {
    client.create_task(&mut task).await?;
    Ok(task)
}
async fn async_update(client: RustyClient, mut task: TodoTask) -> Result<TodoTask, String> {
    client.update_task(&mut task).await?;
    Ok(task)
}
