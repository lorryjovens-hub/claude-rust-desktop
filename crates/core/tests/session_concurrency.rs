use claude_core::runtime::{RuntimePaths, SessionStore};
use std::sync::{Arc, Barrier};

#[test]
fn parallel_cli_sessions_and_messages_do_not_upgrade_stale_wal_snapshots() {
    let temp = tempfile::tempdir().unwrap();
    let paths = RuntimePaths::new(temp.path().join("data")).unwrap();
    let db = paths.open_database().unwrap();
    let barrier = Arc::new(Barrier::new(6));
    let threads = (0..6)
        .map(|_| {
            let db = db.clone();
            let workspace = temp.path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = SessionStore::new(db);
                barrier.wait();
                for _ in 0..10 {
                    let id = store.create(&workspace, "fixture", "fixture").unwrap();
                    store.append(&id, "user", "hello").unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn separate_hosts_can_open_and_initialize_the_same_database() {
    let temp = tempfile::tempdir().unwrap();
    let barrier = Arc::new(Barrier::new(6));
    let threads = (0..6).map(|_| {
        let data = temp.path().join("data");
        let workspace = temp.path().to_path_buf();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            for _ in 0..3 {
                let db = RuntimePaths::new(data.clone()).unwrap().open_database().unwrap();
                let store = SessionStore::new(db);
                let id = store.create(&workspace,"fixture","fixture").unwrap();
                store.append(&id,"user","hello").unwrap();
            }
        })
    }).collect::<Vec<_>>();
    for thread in threads {thread.join().unwrap();}
}
