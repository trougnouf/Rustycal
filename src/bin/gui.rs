use rustache::client::RustyClient;
use rustache::config::Config;
use rustache::model::Task as TodoTask;

use iced::widget::{checkbox, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Task, Theme};
use std::sync::OnceLock;
use tokio::runtime::Runtime;

// --- GLOBAL RUNTIME ---
// We need this because Iced's background threads don't have the
// Tokio Reactor context required by libdav/hyper.
static TOKIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn main() -> iced::Result {
    // 1. Initialize the Global Runtime
    let runtime = Runtime::new().expect("Failed to create Tokio runtime");
    TOKIO_RUNTIME
        .set(runtime)
        .expect("Failed to set global runtime");

    // 2. Run App
    iced::application("Rustache", RustacheGui::update, RustacheGui::view)
        .theme(RustacheGui::theme)
        .run_with(RustacheGui::new)
}

struct RustacheGui {
    tasks: Vec<TodoTask>,
    input_value: String,
    client: Option<RustyClient>,
    loading: bool,
    error_msg: Option<String>,
}

impl Default for RustacheGui {
    fn default() -> Self {
        Self {
            tasks: vec![],
            input_value: String::new(),
            client: None,
            loading: true,
            error_msg: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    CreateTask,
    ToggleTask(usize, bool),
    Loaded(Result<(RustyClient, Vec<TodoTask>), String>),
    SyncSaved(Result<TodoTask, String>),
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
            Message::Loaded(Ok((client, tasks))) => {
                self.client = Some(client);
                self.tasks = tasks;
                self.loading = false;
            }
            Message::Loaded(Err(e)) => {
                self.error_msg = Some(format!("Connection Failed: {}", e));
                self.loading = false;
            }

            Message::SyncSaved(Ok(updated_task)) => {
                // Update local state with server version (ETag)
                if let Some(index) = self.tasks.iter().position(|t| t.uid == updated_task.uid) {
                    self.tasks[index] = updated_task;
                }
            }
            Message::SyncSaved(Err(e)) => {
                self.error_msg = Some(format!("Sync Error: {}", e));
            }

            Message::InputChanged(value) => {
                self.input_value = value;
            }

            Message::CreateTask => {
                if !self.input_value.is_empty() {
                    let new_task = TodoTask::new(&self.input_value);
                    self.tasks.push(new_task.clone());
                    self.input_value.clear();

                    if let Some(client) = &self.client {
                        // Run on Global Runtime
                        return Task::perform(
                            async_create_wrapper(client.clone(), new_task),
                            Message::SyncSaved,
                        );
                    }
                }
            }

            Message::ToggleTask(index, is_checked) => {
                if let Some(task) = self.tasks.get_mut(index) {
                    task.completed = is_checked;

                    if let Some(client) = &self.client {
                        // Run on Global Runtime
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

    fn view(&self) -> Element<'_, Message> {
        let title_text = if self.loading {
            "Rustache (Loading...)"
        } else if let Some(err) = &self.error_msg {
            err
        } else {
            "Rustache"
        };

        let input = text_input("Add a task (e.g. Buy Milk !1)...", &self.input_value)
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
                        1..=4 => iced::Color::from_rgb(0.8, 0.2, 0.2),
                        5 => iced::Color::from_rgb(0.8, 0.8, 0.2),
                        _ => iced::Color::WHITE,
                    };

                    row![
                        checkbox("", task.completed).on_toggle(move |b| Message::ToggleTask(i, b)),
                        text(&task.summary).size(20).color(color),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center)
                    .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(10)
        .into();

        let content = column![text(title_text).size(40), input, scrollable(tasks_view)]
            .spacing(20)
            .max_width(800);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .padding(20)
            .into()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

// --- WRAPPERS TO FORCE TOKIO RUNTIME ---

async fn connect_and_fetch_wrapper() -> Result<(RustyClient, Vec<TodoTask>), String> {
    let rt = TOKIO_RUNTIME.get().expect("Runtime not initialized");
    rt.spawn(async { connect_and_fetch().await })
        .await
        .map_err(|e| e.to_string())? // Handle JoinError
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

// --- CORE LOGIC (Same as before) ---

async fn connect_and_fetch() -> Result<(RustyClient, Vec<TodoTask>), String> {
    let config = Config::load().map_err(|e| e.to_string())?;
    let mut client = RustyClient::new(&config.url, &config.username, &config.password)
        .map_err(|e| e.to_string())?;

    if let Some(def_cal) = config.default_calendar {
        if let Ok(cals) = client.get_calendars().await {
            if let Some(found) = cals.iter().find(|c| c.name == def_cal || c.href == def_cal) {
                client.set_calendar(&found.href);
            } else {
                client
                    .discover_calendar()
                    .await
                    .map_err(|e| e.to_string())?;
            }
        } else {
            client
                .discover_calendar()
                .await
                .map_err(|e| e.to_string())?;
        }
    } else {
        client
            .discover_calendar()
            .await
            .map_err(|e| e.to_string())?;
    }

    let mut tasks = client.get_tasks().await.map_err(|e| e.to_string())?;
    tasks.sort();
    Ok((client, tasks))
}

async fn async_create(client: RustyClient, mut task: TodoTask) -> Result<TodoTask, String> {
    client.create_task(&mut task).await?;
    Ok(task)
}

async fn async_update(client: RustyClient, mut task: TodoTask) -> Result<TodoTask, String> {
    client.update_task(&mut task).await?;
    Ok(task)
}
