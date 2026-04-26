use std::fs::OpenOptions;
use std::sync::Arc;

fn main() {
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("test.log")
        .unwrap();

    let writer = Arc::new(log_file);

    tracing_subscriber::fmt()
        .with_writer(writer)
        .init();

    tracing::info!("Hello, world!");
}
