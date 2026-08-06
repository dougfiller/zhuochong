/// Placeholder state for the future reply workflow. It intentionally holds no
/// app state, database handle, capture data, OCR text, or model client.
#[derive(Default)]
pub(crate) struct WechatReplyRuntime;

impl WechatReplyRuntime {
    /// Private OCR hand-off for a verified capture. It has no Tauri command and
    /// cannot progress to retrieval or a model after an OCR failure.
    pub(crate) fn recognize_captured_wechat<S: super::ocr::WechatOcrAuditSink>(
        &self,
        slices: &super::capture::WechatCaptureSlices,
        identity: &super::window_identity::WechatWindowIdentity,
        captured: super::types::CapturedWechat,
        state: &mut super::state_machine::StateMachine,
        stage_seq: u64,
        audit_sink: &mut S,
    ) -> Result<super::types::OcrReadyReply, super::types::ContractError> {
        let mut dispatcher = super::ocr::WechatOcrDispatcher::new(
            super::ocr::WindowsMemoryPrimary,
            super::ocr::DisabledLocalFallback,
        );
        match dispatcher.recognize(slices, identity, captured, audit_sink) {
            Ok(reply) => Ok(reply),
            Err(error @ (super::types::ContractError::WxOcrEmpty
            | super::types::ContractError::WxOcrUnavailable
            | super::types::ContractError::WxOcrFailed)) => {
                state.fail_ocr(error, stage_seq)?;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    /// Re-reads the foreground Windows window and returns a backend-only identity.
    /// This step never captures pixels, invokes OCR, retrieval, or a model.
    #[cfg(target_os = "windows")]
    pub(crate) fn validate_foreground_wechat(
        &self,
    ) -> Result<super::window_identity::WechatWindowIdentity, super::types::ContractError> {
        let catalog = super::profiles::CompatibilityCatalog::embedded()
            .map_err(|_| super::types::ContractError::WxProfileUnsupported)?;
        let evidence = crate::monitor::read_foreground_window_evidence()
            .map_err(|_| super::types::ContractError::WxNotForeground)?;
        super::window_identity::validate_foreground(&catalog, evidence)
    }

    /// Non-Windows targets cannot create a Windows compatibility identity.
    #[cfg(not(target_os = "windows"))]
    pub(crate) fn validate_foreground_wechat(
        &self,
    ) -> Result<super::window_identity::WechatWindowIdentity, super::types::ContractError> {
        Err(super::types::ContractError::WxWindowUnsupported)
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn revalidate_foreground_wechat(
        &self,
        previous: &super::window_identity::WechatWindowIdentity,
    ) -> Result<super::window_identity::WechatWindowIdentity, super::types::ContractError> {
        let catalog = super::profiles::CompatibilityCatalog::embedded()
            .map_err(|_| super::types::ContractError::WxProfileUnsupported)?;
        let evidence = crate::monitor::read_foreground_window_evidence()
            .map_err(|_| super::types::ContractError::WxNotForeground)?;
        super::window_identity::revalidate_foreground(&catalog, previous, evidence)
    }

    /// Non-Windows targets cannot re-read a Windows compatibility identity.
    #[cfg(not(target_os = "windows"))]
    pub(crate) fn revalidate_foreground_wechat(
        &self,
        _previous: &super::window_identity::WechatWindowIdentity,
    ) -> Result<super::window_identity::WechatWindowIdentity, super::types::ContractError> {
        Err(super::types::ContractError::WxWindowUnsupported)
    }
}

/// Serializes Work Review captures and the private WeChat capture transaction.
/// Callers must use `try_acquire` so background recording never queues work.
pub(crate) struct CaptureCoordinator {
    permit: std::sync::Arc<tokio::sync::Semaphore>,
    latest_capture_version: std::sync::atomic::AtomicU64,
}

impl Default for CaptureCoordinator {
    fn default() -> Self {
        Self {
            permit: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            latest_capture_version: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl CaptureCoordinator {
    pub(crate) fn try_acquire(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.permit.clone().try_acquire_owned().ok()
    }

    /// Allocate only after a complete capture has passed revalidation and ROI
    /// checks, so every returned slice has a strictly newer version.
    pub(crate) fn next_capture_version(&self) -> super::types::CaptureVersion {
        let value = self
            .latest_capture_version
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(1);
        super::types::CaptureVersion::new(value)
    }

    pub(crate) fn is_current_capture(&self, version: super::types::CaptureVersion) -> bool {
        version.value() == self.latest_capture_version.load(std::sync::atomic::Ordering::Relaxed)
    }
}

async fn cancellation_requested(cancel: &mut Option<tokio::sync::watch::Receiver<bool>>) {
    let Some(receiver) = cancel else {
        std::future::pending::<()>().await;
        return;
    };
    loop {
        if *receiver.borrow() {
            return;
        }
        if receiver.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

async fn wait_for_capture_worker<T>(
    worker: &mut tokio::task::JoinHandle<Result<T, super::types::ContractError>>,
    timeout: std::time::Duration,
    cancel: &mut Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<T, super::types::ContractError> {
    if cancel.as_ref().is_some_and(|receiver| *receiver.borrow()) {
        let _ = worker.await;
        return Err(super::types::ContractError::WxRequestCancelled);
    }

    tokio::select! {
        result = tokio::time::timeout(timeout, &mut *worker) => match result {
            Ok(Ok(Ok(frame))) => Ok(frame),
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(_)) => Err(super::types::ContractError::WxCaptureFailed),
            // spawn_blocking cannot be aborted safely. Joining before restore
            // prevents an overdue worker from capturing an already restored overlay.
            Err(_) => {
                let _ = worker.await;
                Err(super::types::ContractError::WxCaptureTimeout)
            }
        },
        _ = cancellation_requested(cancel) => {
            let _ = worker.await;
            Err(super::types::ContractError::WxRequestCancelled)
        },
    }
}

async fn run_guarded_capture<T, V, P, Before, Worker, After>(
    mut guard: super::capture::WechatCaptureGuard<P>,
    before_worker: Before,
    worker: Worker,
    after_worker: After,
    timeout: std::time::Duration,
    cancel: &mut Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<(T, V), super::types::ContractError>
where
    P: super::capture::WechatWindowPort,
    Before: FnOnce() -> Result<(), super::types::ContractError>,
    Worker: FnOnce() -> tokio::task::JoinHandle<Result<T, super::types::ContractError>>,
    After: FnOnce() -> Result<V, super::types::ContractError>,
{
    let outcome = async {
        before_worker()?;
        let mut worker = worker();
        let frame = wait_for_capture_worker(&mut worker, timeout, cancel).await?;
        Ok((frame, after_worker()?))
    }
    .await;
    guard.finish_after_worker();
    outcome
}

impl WechatReplyRuntime {
    /// Runs the private, memory-only capture transaction. There is deliberately
    /// no Tauri command for this method: a later authorized step owns its input.
    #[cfg(target_os = "windows")]
    pub(crate) async fn capture_foreground_wechat(
        &self,
        app: tauri::AppHandle,
        coordinator: &CaptureCoordinator,
        screenshot_service: crate::screenshot::ScreenshotService,
        timeout: std::time::Duration,
        mut cancel: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<super::capture::WechatCaptureSlices, super::types::ContractError> {
        let identity = self.validate_foreground_wechat()?;
        let _permit = coordinator
            .try_acquire()
            .ok_or(super::types::ContractError::WxBusy)?;
        let guard = super::capture::WechatCaptureGuard::begin(
            super::capture::TauriWechatWindowPort::new(app),
        )?;
        let (frame, current) = run_guarded_capture(
            guard,
            || self.revalidate_foreground_wechat(&identity).map(|_| ()),
            || {
                let target_window = identity.capture_target_window();
                tokio::task::spawn_blocking(move || {
                    screenshot_service
                        .capture_ephemeral_for_window(&target_window)
                        .map_err(|_| super::types::ContractError::WxCaptureFailed)
                })
            },
            || self.revalidate_foreground_wechat(&identity),
            timeout,
            &mut cancel,
        )
        .await?;
        let (chat_rgba, header_identity_rgba) = super::capture::cropped_images_for_identity(
            super::capture::EphemeralCapturedFrame::new(frame),
            &current,
        )?;
        Ok(super::capture::WechatCaptureSlices {
            request_id: super::types::RequestId::new(),
            capture_version: coordinator.next_capture_version(),
            chat_rgba,
            header_identity_rgba,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier, Mutex};

    #[derive(Clone)]
    struct FakeWindowPort {
        calls: Arc<Mutex<Vec<&'static str>>>,
        avatar_hidden: Arc<std::sync::atomic::AtomicBool>,
    }

    impl FakeWindowPort {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                avatar_hidden: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl super::super::capture::WechatWindowPort for FakeWindowPort {
        fn is_visible(
            &self,
            label: &'static str,
        ) -> Result<Option<bool>, super::super::types::ContractError> {
            Ok((label == crate::avatar_engine::AVATAR_WINDOW_LABEL).then_some(
                !self.avatar_hidden.load(std::sync::atomic::Ordering::SeqCst),
            ))
        }

        fn hide(&self, _label: &'static str) -> Result<(), super::super::types::ContractError> {
            self.calls.lock().unwrap().push("hide");
            self.avatar_hidden.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        fn show(&self, _label: &'static str) {
            self.calls.lock().unwrap().push("show");
            self.avatar_hidden.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn guard(port: FakeWindowPort) -> super::super::capture::WechatCaptureGuard<FakeWindowPort> {
        super::super::capture::WechatCaptureGuard::begin(port).unwrap()
    }

    #[test]
    fn successful_captures_receive_monotonic_versions_and_reject_old_results() {
        let coordinator = CaptureCoordinator::default();
        let first = coordinator.next_capture_version();
        let second = coordinator.next_capture_version();

        assert!(second > first);
        assert!(!coordinator.is_current_capture(first));
        assert!(coordinator.is_current_capture(second));
    }

    #[tokio::test]
    async fn stale_identity_does_not_start_a_worker_and_restores_once() {
        let port = FakeWindowPort::new();
        let worker_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started = worker_started.clone();
        let mut cancel = None;

        let result = run_guarded_capture(
            guard(port.clone()),
            || Err(super::super::types::ContractError::WxRequestStale),
            move || {
                started.store(true, std::sync::atomic::Ordering::SeqCst);
                tokio::task::spawn_blocking(|| Ok(()))
            },
            || Ok(()),
            std::time::Duration::from_millis(20),
            &mut cancel,
        )
        .await;

        assert_eq!(result, Err(super::super::types::ContractError::WxRequestStale));
        assert!(!worker_started.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(port.calls(), vec!["hide", "show"]);
    }

    #[tokio::test]
    async fn provider_failure_and_worker_panic_restore_once() {
        for worker in [
            tokio::task::spawn_blocking(|| Err(super::super::types::ContractError::WxCaptureFailed)),
            tokio::task::spawn_blocking(|| -> Result<(), super::super::types::ContractError> {
                panic!("test worker panic")
            }),
        ] {
            let port = FakeWindowPort::new();
            let mut cancel = None;
            let result = run_guarded_capture(
                guard(port.clone()),
                || Ok(()),
                || worker,
                || Ok(()),
                std::time::Duration::from_millis(20),
                &mut cancel,
            )
            .await;

            assert_eq!(result, Err(super::super::types::ContractError::WxCaptureFailed));
            assert_eq!(port.calls(), vec!["hide", "show"]);
        }
    }

    #[tokio::test]
    async fn timeout_waits_for_worker_before_restoring() {
        let port = FakeWindowPort::new();
        let release = Arc::new(Barrier::new(2));
        let worker_release = release.clone();
        let task_port = port.clone();
        let task = tokio::spawn(async move {
            let mut cancel = None;
            run_guarded_capture(
                guard(task_port),
                || Ok(()),
                move || tokio::task::spawn_blocking(move || {
                    worker_release.wait();
                    Ok(())
                }),
                || Ok(()),
                std::time::Duration::from_millis(10),
                &mut cancel,
            )
            .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert_eq!(port.calls(), vec!["hide"]);
        release.wait();
        assert_eq!(task.await.unwrap(), Err(super::super::types::ContractError::WxCaptureTimeout));
        assert_eq!(port.calls(), vec!["hide", "show"]);
    }

    #[tokio::test]
    async fn cancellation_waits_for_worker_before_restoring() {
        let port = FakeWindowPort::new();
        let release = Arc::new(Barrier::new(2));
        let worker_release = release.clone();
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let task_port = port.clone();
        let task = tokio::spawn(async move {
            let mut cancel = Some(cancel_rx);
            run_guarded_capture(
                guard(task_port),
                || Ok(()),
                move || tokio::task::spawn_blocking(move || {
                    worker_release.wait();
                    Ok(())
                }),
                || Ok(()),
                std::time::Duration::from_secs(1),
                &mut cancel,
            )
            .await
        });

        cancel_tx.send(true).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(port.calls(), vec!["hide"]);
        release.wait();
        assert_eq!(task.await.unwrap(), Err(super::super::types::ContractError::WxRequestCancelled));
        assert_eq!(port.calls(), vec!["hide", "show"]);
    }
}

#[allow(dead_code)]
pub(crate) trait WechatCapturePort {
    fn capture_is_unsupported(&self) -> &'static str;
}

#[allow(dead_code)]
pub(crate) struct UnsupportedWechatCapture;

impl WechatCapturePort for UnsupportedWechatCapture {
    fn capture_is_unsupported(&self) -> &'static str {
        "WX_NOT_READY"
    }
}

#[allow(dead_code)]
pub(crate) trait WechatReplyModelPort {
    fn reply_is_unavailable(&self) -> &'static str;
}

#[allow(dead_code)]
pub(crate) struct UnavailableWechatReplyModel;

impl WechatReplyModelPort for UnavailableWechatReplyModel {
    fn reply_is_unavailable(&self) -> &'static str {
        "WX_TEXT_MODEL_UNAVAILABLE"
    }
}
