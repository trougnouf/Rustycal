// File: ./tests/sync_integration.rs
use cfait::client::RustyClient;
use cfait::journal::Action;
use cfait::model::Task;
use mockito::Server;
use std::collections::HashMap;
use std::env;
use std::fs;

#[tokio::test]
async fn test_sync_recovers_from_412() {
    // 0. Setup Isolation
    let temp_dir = env::temp_dir().join(format!("cfait_test_sync_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    unsafe {
        env::set_var("CFAIT_TEST_DIR", &temp_dir);
    }

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
    let mock_conflict_copy = server
        .mock(
            "PUT",
            mockito::Matcher::Regex(r"^/cal/.*\.ics$".to_string()),
        )
        .match_header("If-None-Match", "*")
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

    // Clean any residual file in temp (unlikely, but safe)
    if let Some(p) = cfait::journal::Journal::get_path() {
        if p.exists() {
            let _ = fs::remove_file(p);
        }
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

    // CLEANUP
    unsafe {
        env::remove_var("CFAIT_TEST_DIR");
    }
    let _ = fs::remove_dir_all(&temp_dir);
}
