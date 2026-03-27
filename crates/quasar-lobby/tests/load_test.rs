//! Lobby load test — simulates multiple concurrent session operations.
//!
//! Run with: cargo test -p quasar-lobby --test load_test --release -- --nocapture

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

const NUM_CLIENTS: usize = 100;
const NUM_OPERATIONS_PER_CLIENT: usize = 10;
const MAX_CONCURRENT: usize = 20;

#[tokio::test]
async fn lobby_load_test() {
    let _ = env_logger::builder().is_test(true).try_init();

    let start = Instant::now();
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let mut handles = Vec::new();

    for client_id in 0..NUM_CLIENTS {
        let permit = semaphore.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire().await.unwrap();

            for op_id in 0..NUM_OPERATIONS_PER_CLIENT {
                let session_name = format!("test-session-{}-{}", client_id, op_id);
                let _session_id = simulate_create_session(&session_name).await;
                let _results = simulate_find_sessions().await;
                let _join_info = simulate_join_session(op_id as u64).await;
            }

            client_id
        }));
    }

    let mut completed = 0usize;
    for handle in handles {
        if let Ok(id) = handle.await {
            completed += 1;
            if completed % 10 == 0 {
                log::info!("Completed {} clients", completed);
            }
        }
    }

    let duration = start.elapsed();
    let total_ops = NUM_CLIENTS * NUM_OPERATIONS_PER_CLIENT * 3;
    let ops_per_sec = total_ops as f64 / duration.as_secs_f64();

    log::info!("Load test completed:");
    log::info!("  Total clients: {}", NUM_CLIENTS);
    log::info!("  Operations per client: {}", NUM_OPERATIONS_PER_CLIENT);
    log::info!("  Total operations: {}", total_ops);
    log::info!("  Duration: {:?}", duration);
    log::info!("  Operations/sec: {:.2}", ops_per_sec);
    log::info!(
        "  Avg latency: {:.2}ms",
        duration.as_millis() as f64 / total_ops as f64
    );

    assert_eq!(completed, NUM_CLIENTS, "All clients should complete");
    assert!(ops_per_sec > 100.0, "Should achieve at least 100 ops/sec");
}

async fn simulate_create_session(name: &str) -> u64 {
    tokio::time::sleep(Duration::from_micros(100)).await;
    let hash = blake3::hash(name.as_bytes());
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap())
}

async fn simulate_find_sessions() -> Vec<u64> {
    tokio::time::sleep(Duration::from_micros(50)).await;
    (0..5).map(|i| i as u64 * 1000).collect()
}

async fn simulate_join_session(session_id: u64) -> String {
    tokio::time::sleep(Duration::from_micros(75)).await;
    format!("token-{}-{}", session_id, uuid::Uuid::new_v4())
}

#[tokio::test]
async fn concurrent_session_creation() {
    let start = Instant::now();
    let mut handles = Vec::new();

    for i in 0..50 {
        handles.push(tokio::spawn(async move {
            simulate_create_session(&format!("concurrent-session-{}", i)).await
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    let duration = start.elapsed();
    log::info!("Created {} sessions in {:?}", results.len(), duration);
    assert_eq!(results.len(), 50);
}

#[tokio::test]
async fn session_query_performance() {
    let start = Instant::now();
    let mut total_results = 0;

    for _ in 0..100 {
        let results = simulate_find_sessions().await;
        total_results += results.len();
    }

    let duration = start.elapsed();
    let queries_per_sec = 100.0 / duration.as_secs_f64();
    log::info!(
        "Query performance: {} results in {:?} ({:.2} queries/sec)",
        total_results,
        duration,
        queries_per_sec
    );
    assert!(queries_per_sec > 50.0);
}

#[tokio::test]
async fn join_throughput() {
    let start = Instant::now();
    let mut handles = Vec::new();

    for i in 0..100 {
        handles.push(tokio::spawn(async move {
            simulate_join_session(i as u64).await
        }));
    }

    let mut tokens = Vec::new();
    for handle in handles {
        tokens.push(handle.await.unwrap());
    }

    let duration = start.elapsed();
    let joins_per_sec = 100.0 / duration.as_secs_f64();
    log::info!(
        "Join throughput: {} joins in {:?} ({:.2} joins/sec)",
        tokens.len(),
        duration,
        joins_per_sec
    );
    assert!(joins_per_sec > 50.0);
}
