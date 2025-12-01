// File: ./tests/locking.rs
use cfait::journal::{Action, Journal};
use cfait::model::Task;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

#[test]
fn test_concurrent_journal_writes() {
    // 1. Setup Isolation
    let temp_dir = env::temp_dir().join(format!("cfait_test_lock_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    // We must set this var in the test process so the threads inherit it
    unsafe {
        env::set_var("CFAIT_TEST_DIR", &temp_dir);
    }

    // 2. Setup Barrier to ensure threads start writing exactly at the same time
    let thread_count = 10;
    let barrier = Arc::new(Barrier::new(thread_count));

    let mut handles = vec![];

    for i in 0..thread_count {
        let b = barrier.clone();
        // Clone the path string to pass to thread (env var is process global, but just to be safe)
        let handle = thread::spawn(move || {
            b.wait(); // Wait for everyone to be ready

            let mut task = Task::new(&format!("Task {}", i), &HashMap::new());
            task.uid = format!("uid-{}", i);

            // This calls Journal::push which does Lock -> Load -> Append -> Save -> Unlock
            let res = Journal::push(Action::Create(task));
            assert!(res.is_ok(), "Journal push failed in thread {}", i);
        });
        handles.push(handle);
    }

    // 3. Wait for all threads
    for h in handles {
        h.join().unwrap();
    }

    // 4. Verify Data Integrity
    let journal = Journal::load();

    // Clean up before asserting, so we don't leave trash on failure
    unsafe {
        env::remove_var("CFAIT_TEST_DIR");
    }
    let _ = fs::remove_dir_all(&temp_dir);

    assert_eq!(
        journal.queue.len(),
        thread_count,
        "Journal should contain exactly {} items",
        thread_count
    );

    // Verify no duplicates and all UIDs present
    let uids: Vec<String> = journal
        .queue
        .iter()
        .map(|a| match a {
            Action::Create(t) => t.uid.clone(),
            _ => "".to_string(),
        })
        .collect();

    for i in 0..thread_count {
        assert!(
            uids.contains(&format!("uid-{}", i)),
            "Journal missing uid-{}",
            i
        );
    }
}
