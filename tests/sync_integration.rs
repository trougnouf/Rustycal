use cfait::client::RustyClient;
use cfait::journal::Action;
use cfait::model::Task;
use mockito::Server;
use std::collections::HashMap;

#[tokio::test]
async fn test_sync_recovers_from_412() {
    // 1. Setup Mock Server
    let mut server = Server::new_async().await;
    let url = server.url();

    let task_uid = "test-uid";
    let _task_href = format!("{}/cal/test-uid.ics", url);

    // 2. Mock: The Initial Update (Returns 412 Conflict)
    let mock_412 = server
        .mock("PUT", "/cal/test-uid.ics")
        .match_header("If-Match", "old-etag")
        .with_status(412)
        .create_async()
        .await;

    // 3. Mock: The Safe Resolution (Create Conflict Copy)
    // The client should immediately create a NEW file with a NEW UUID.
    // We match any PUT to the calendar directory that is NOT the original task.
    let mock_conflict_copy = server
        .mock(
            "PUT",
            mockito::Matcher::Regex(r"^/cal/.*\.ics$".to_string()),
        )
        .match_header("If-None-Match", "*") // Ensure it's a CREATE
        .match_body(mockito::Matcher::Regex(r"Conflict Copy".to_string()))
        .with_status(201)
        .create_async()
        .await;

    // 5. Configure Client
    let client = RustyClient::new(&url, "user", "pass", true).unwrap();

    // 6. Setup Local State (Journal)
    let mut task = Task::new("Local Title", &HashMap::new());
    task.uid = task_uid.to_string();
    task.calendar_href = "/cal/".to_string();
    task.href = format!("/cal/{}.ics", task_uid);
    task.description = "Local Description".to_string();
    task.etag = "old-etag".to_string();

    // Clear existing journal
    if let Some(p) = cfait::journal::Journal::get_path() {
        let _ = std::fs::remove_file(p);
    }

    cfait::journal::Journal::push(Action::Update(task)).unwrap();

    // 7. Run Sync
    println!("Starting Sync...");
    let result = client.sync_journal().await;

    // 8. Assertions
    assert!(result.is_ok(), "Sync should succeed");

    mock_412.assert();
    mock_conflict_copy.assert();

    // Ensure Journal is empty
    let j = cfait::journal::Journal::load();
    assert!(
        j.is_empty(),
        "Journal should be empty after successful sync"
    );
}
