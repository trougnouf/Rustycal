use cfait::client::RustyClient;
use cfait::journal::{Action, Journal};
use cfait::model::Task;
use mockito::Server;
use std::collections::HashMap;

#[tokio::test]
async fn test_move_propagates_href_to_pending_update() {
    // 1. Setup Mock Server
    let mut server = Server::new_async().await;
    let url = server.url();

    let task_uid = "moved-task";
    let old_cal = "/cal1/";
    let new_cal = "/cal2/";

    // Construct paths as the client expects them (relative to base)
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
    // CRITICAL: We expect the PUT to happen at the NEW URI.
    // If the propagation logic fails, the client will try to PUT to old_href.
    let mock_update_at_new_loc = server
        .mock("PUT", new_href.as_str())
        .with_status(204) // Success
        .with_header("ETag", "\"new-etag\"")
        .create_async()
        .await;

    // 4. Configure Client
    let client = RustyClient::new(&url, "user", "pass", true).unwrap();

    // 5. Setup Journal with dependent actions
    let mut task = Task::new("Task to Move", &HashMap::new());
    task.uid = task_uid.to_string();
    task.calendar_href = old_cal.to_string();
    task.href = old_href.clone();
    task.etag = "\"orig-etag\"".to_string();

    // Clear existing journal
    if let Some(p) = Journal::get_path() {
        let _ = std::fs::remove_file(p);
    }

    // Push Action 1: Move
    Journal::push(Action::Move(task.clone(), new_cal.to_string())).unwrap();

    // Push Action 2: Update (This task struct still points to OLD HREF)
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

    // Ensure Journal is empty
    let j = Journal::load();
    assert!(j.is_empty(), "Journal should be empty");
}
