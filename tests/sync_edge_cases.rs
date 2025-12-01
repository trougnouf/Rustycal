// File: ./tests/sync_edge_cases.rs
use cfait::client::RustyClient;
use cfait::journal::{Action, Journal};
use cfait::model::Task;
use mockito::Server;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::Mutex;

// Global lock to prevent tests from clobbering the shared ENV var
static TEST_MUTEX: Mutex<()> = Mutex::new(());

fn setup_env(suffix: &str) -> std::path::PathBuf {
    let temp_dir =
        env::temp_dir().join(format!("cfait_test_edge_{}_{}", suffix, std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);

    // UNSAFE: modifying process environment
    unsafe {
        env::set_var("CFAIT_TEST_DIR", &temp_dir);
    }

    // Clean potential previous run
    if let Some(p) = Journal::get_path() {
        if p.exists() {
            let _ = fs::remove_file(p);
        }
    }
    temp_dir
}

fn teardown(path: std::path::PathBuf) {
    unsafe {
        env::remove_var("CFAIT_TEST_DIR");
    }
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn test_sync_delete_404_is_success() {
    // 0. Acquire Lock to run exclusively
    let _guard = TEST_MUTEX.lock().unwrap();

    let temp_dir = setup_env("404");

    // 1. Mock Server returning 404 Not Found for a DELETE
    let mut server = Server::new_async().await;
    let url = server.url();
    let mock = server
        .mock("DELETE", "/cal/task.ics")
        .with_status(404)
        .create_async()
        .await;

    // 2. Setup Client
    let client = RustyClient::new(&url, "u", "p", true).unwrap();

    // 3. Add Delete Action to Journal
    let mut task = Task::new("T", &HashMap::new());
    // Note: client.rs uses strip_host, so we ensure the href implies the relative path
    task.href = format!("{}/cal/task.ics", url);
    task.etag = "\"123\"".to_string();
    Journal::push(Action::Delete(task)).unwrap();

    // 4. Sync
    let res = client.sync_journal().await;

    // 5. Assertions
    // 404 on delete means "already deleted", so sync should succeed (Ok)
    assert!(res.is_ok(), "Sync failed: {:?}", res.err());
    mock.assert();

    // Item should be removed from journal
    let j = Journal::load();
    assert!(j.is_empty(), "Journal should be empty after 404 delete");

    teardown(temp_dir);
}

#[tokio::test]
async fn test_sync_500_keeps_item_in_queue() {
    // 0. Acquire Lock to run exclusively
    let _guard = TEST_MUTEX.lock().unwrap();

    let temp_dir = setup_env("500");

    // 1. Mock Server returning 500 Error
    let mut server = Server::new_async().await;
    let url = server.url();
    let mock = server
        .mock("PUT", "/cal/task.ics")
        .with_status(500)
        .create_async()
        .await;

    // 2. Setup Client
    let client = RustyClient::new(&url, "u", "p", true).unwrap();

    // 3. Add Create Action
    let mut task = Task::new("T", &HashMap::new());
    task.uid = "task".to_string();
    task.calendar_href = "/cal/".to_string();
    Journal::push(Action::Create(task)).unwrap();

    // 4. Sync
    let res = client.sync_journal().await;

    // 5. Assertions
    // Should return Error
    assert!(res.is_err(), "Sync should have failed due to 500");
    mock.assert();

    // Item should REMAIN in journal because it failed
    let j = Journal::load();
    assert!(
        !j.is_empty(),
        "Journal should still contain the failed item"
    );
    assert_eq!(j.queue.len(), 1);

    teardown(temp_dir);
}
