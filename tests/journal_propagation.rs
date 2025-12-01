// File: ./tests/journal_propagation.rs
use cfait::client::RustyClient;
use cfait::journal::{Action, Journal};
use cfait::model::Task;
use mockito::Server;
use std::collections::HashMap;
use std::env;
use std::fs;

#[tokio::test]
async fn test_move_propagates_href_to_pending_update() {
    // 0. Setup Isolation
    let temp_dir = env::temp_dir().join(format!("cfait_test_prop_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    unsafe {
        env::set_var("CFAIT_TEST_DIR", &temp_dir);
    }

    // 1. Setup Mock Server
    let mut server = Server::new_async().await;
    let url = server.url();

    let task_uid = "moved-task";
    let old_cal = "/cal1/";
    let new_cal = "/cal2/";

    let old_href = format!("{}{}.ics", old_cal, task_uid);
    let new_href = format!("{}{}.ics", new_cal, task_uid);

    // 2. Mock: The MOVE request
    let mock_move = server
        .mock("MOVE", old_href.as_str())
        .match_header(
            "Destination",
            mockito::Matcher::Regex(format!(r".*{}.*", new_href)),
        )
        .with_status(201)
        .create_async()
        .await;

    // 3. Mock: The UPDATE request
    let mock_update_at_new_loc = server
        .mock("PUT", new_href.as_str())
        .with_status(204)
        .with_header("ETag", "\"new-etag\"")
        .create_async()
        .await;

    // 4. Configure Client
    let client = RustyClient::new(&url, "user", "pass", true).unwrap();

    // 5. Setup Journal
    let mut task = Task::new("Task to Move", &HashMap::new());
    task.uid = task_uid.to_string();
    task.calendar_href = old_cal.to_string();
    task.href = old_href.clone();
    task.etag = "\"orig-etag\"".to_string();

    if let Some(p) = Journal::get_path() {
        if p.exists() {
            let _ = fs::remove_file(p);
        }
    }

    Journal::push(Action::Move(task.clone(), new_cal.to_string())).unwrap();

    let mut update_task = task.clone();
    update_task.summary = "Updated Summary".to_string();
    Journal::push(Action::Update(update_task)).unwrap();

    // 6. Run Sync
    println!("Starting Sync...");
    let result = client.sync_journal().await;

    // 7. Assertions
    assert!(result.is_ok(), "Sync should succeed: {:?}", result.err());

    mock_move.assert();
    mock_update_at_new_loc.assert();

    let j = Journal::load();
    assert!(j.is_empty(), "Journal should be empty");

    // CLEANUP
    unsafe {
        env::remove_var("CFAIT_TEST_DIR");
    }
    let _ = fs::remove_dir_all(&temp_dir);
}
