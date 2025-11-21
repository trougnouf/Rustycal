use rustache::client::RustyClient;
use rustache::config::Config;
use rustache::model::{CalendarListEntry, Task as TodoTask};

use iced::widget::{
    Rule, button, checkbox, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Background, Color, Element, Event, Length, Subscription, Task, Theme, keyboard}; // Import keyboard
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static TOKIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn main() -> iced::Result {
    let runtime = Runtime::new().expect("Failed to create Tokio runtime");
    TOKIO_RUNTIME
        .set(runtime)
        .expect("Failed to set global runtime");

    iced::application("Rustache", RustacheGui::update, RustacheGui::view)
        .subscription(RustacheGui::subscription) // Need this for keys
        .theme(RustacheGui::theme)
        .run_with(RustacheGui::new)
}

struct RustacheGui {
    tasks: Vec<TodoTask>,
    calendars: Vec<CalendarListEntry>,
    active_cal_href: Option<String>,
    input_value: String,
    client: Option<RustyClient>,
    loading: bool,
    error_msg: Option<String>,
    // Track selected task index for keyboard indentation
    selected_index: Option<usize>,
}

impl Default for RustacheGui {
    fn default() -> Self {
        Self {
            tasks: vec![],
            calendars: vec![],
            active_cal_href: None,
            input_value: String::new(),
            client: None,
            loading: true,
            error_msg: None,
            selected_index: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    CreateTask,
    ToggleTask(usize, bool),
    SelectCalendar(String),
    SelectTask(usize), // For selection tracking
    IndentTask(usize),
    OutdentTask(usize),

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

    // Key events
    EventOccurred(Event),
}

impl RustacheGui {
    fn new() -> (Self, Task<Message>) {
        (
            Self::default(),
            Task::perform(connect_and_fetch_wrapper(), Message::Loaded),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::event::listen().map(Message::EventOccurred)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EventOccurred(Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                ..
            })) => {
                // Handle > and < for Indent/Outdent if we have a selection
                if let Some(idx) = self.selected_index {
                    // Shift + . is >
                    if key == keyboard::Key::Character(".".into()) && modifiers.shift() {
                        return self.update(Message::IndentTask(idx));
                    }
                    // Shift + , is <
                    if key == keyboard::Key::Character(",".into()) && modifiers.shift() {
                        return self.update(Message::OutdentTask(idx));
                    }
                }
                Task::none()
            }
            Message::EventOccurred(_) => Task::none(),

            Message::Loaded(Ok((client, cals, tasks, active))) => {
                self.client = Some(client);
                self.calendars = cals;
                self.tasks = TodoTask::organize_hierarchy(tasks); // SORT HERE
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
                    // Re-sort hierarchy to maintain tree structure
                    let raw_tasks = self.tasks.clone();
                    self.tasks = TodoTask::organize_hierarchy(raw_tasks);
                }
                Task::none()
            }
            Message::SyncSaved(Err(e)) => {
                self.error_msg = Some(format!("Sync Error: {}", e));
                Task::none()
            }

            Message::TasksRefreshed(Ok(tasks)) => {
                self.tasks = TodoTask::organize_hierarchy(tasks); // SORT HERE
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

            Message::SelectTask(i) => {
                self.selected_index = Some(i);
                Task::none()
            }

            Message::IndentTask(index) => {
                // Logic: Make this task a child of the one immediately above it (index - 1)
                if index > 0 {
                    let parent_uid = self.tasks[index - 1].uid.clone();
                    // Prevent indenting under its own child (simple check)
                    if self.tasks[index].parent_uid != Some(parent_uid.clone()) {
                        if let Some(task) = self.tasks.get_mut(index) {
                            task.parent_uid = Some(parent_uid);
                            if let Some(client) = &self.client {
                                return Task::perform(
                                    async_update_wrapper(client.clone(), task.clone()),
                                    Message::SyncSaved,
                                );
                            }
                        }
                    }
                }
                Task::none()
            }

            Message::OutdentTask(index) => {
                if let Some(task) = self.tasks.get_mut(index) {
                    if task.parent_uid.is_some() {
                        task.parent_uid = None;
                        if let Some(client) = &self.client {
                            return Task::perform(
                                async_update_wrapper(client.clone(), task.clone()),
                                Message::SyncSaved,
                            );
                        }
                    }
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
                    // Temporarily push flat
                    self.tasks.push(new_task.clone());
                    // Re-organize immediately for display
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
        // 1. SIDEBAR
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

        // 2. MAIN CONTENT
        let title_text = if self.loading {
            "Loading..."
        } else {
            "Rustache"
        };

        let input = text_input("Add a task...", &self.input_value)
            .on_input(Message::InputChanged)
            .on_submit(Message::CreateTask)
            .padding(10)
            .size(20);

        let tasks_view: Element<_> = column(
            self.tasks
                .iter()
                .enumerate()
                .map(|(i, task)| {
                    let color = match task.priority {
                        1..=4 => Color::from_rgb(0.8, 0.2, 0.2),
                        5 => Color::from_rgb(0.8, 0.8, 0.2),
                        _ => Color::WHITE,
                    };

                    // INDENTATION SPACER
                    let indent = horizontal_space().width(Length::Fixed((task.depth * 20) as f32));

                    // Selection Style
                    let is_selected = self.selected_index == Some(i);
                    let row_bg = if is_selected {
                        Color::from_rgb(0.2, 0.2, 0.3)
                    } else {
                        Color::TRANSPARENT
                    };

                    let row_content = row![
                        indent,
                        checkbox("", task.completed).on_toggle(move |b| Message::ToggleTask(i, b)),
                        button(text(&task.summary).size(20).color(color))
                            .style(button::text)
                            .on_press(Message::SelectTask(i)) // Click text to select for indentation
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center);

                    container(row_content)
                        .style(move |_| container::Style::default().background(row_bg))
                        .padding(5)
                        .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(2)
        .into();

        let main_content = column![text(title_text).size(40), input, scrollable(tasks_view)]
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

// --- ASYNC WRAPPERS (Same as before) ---
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
        let mut tasks = client.get_tasks().await.map_err(|e| e.to_string())?;
        // NO SORT HERE - handled by organize_hierarchy in update
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
