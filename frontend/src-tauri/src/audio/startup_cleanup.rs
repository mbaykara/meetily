//! Release every state-owned sender before waiting for pipeline termination.
pub async fn after_storage_failure<E, Release, Stop, Stopped, Output>(
    original: E,
    release_state: Release,
    stop_pipeline: Stop,
) -> E
where
    Release: FnOnce(),
    Stop: FnOnce() -> Stopped,
    Stopped: std::future::Future<Output = Output>,
{
    release_state();
    let _ = stop_pipeline().await;
    original
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::sync::{mpsc, Mutex as AsyncMutex};
    #[tokio::test]
    async fn failed_storage_closes_state_sender_and_releases_lifecycle_for_next_start() {
        let lifecycle = AsyncMutex::new(());
        let start_lock = lifecycle.lock().await;
        let (pipeline_sender, mut receiver) = mpsc::unbounded_channel::<()>();
        let state_sender = Arc::new(Mutex::new(Some(pipeline_sender.clone())));
        let pipeline = tokio::spawn(async move { while receiver.recv().await.is_some() {} });
        let failure = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            after_storage_failure(
                "storage initialization failed",
                || {
                    state_sender.lock().unwrap().take();
                },
                || async {
                    drop(pipeline_sender);
                    pipeline.await.unwrap();
                },
            ),
        )
        .await
        .expect("startup cleanup must close the pipeline receiver");
        assert_eq!(failure, "storage initialization failed");
        drop(start_lock);
        let next_start =
            tokio::time::timeout(std::time::Duration::from_secs(1), lifecycle.lock()).await;
        assert!(
            next_start.is_ok(),
            "a subsequent recording start must acquire lifecycle"
        );
    }
}
