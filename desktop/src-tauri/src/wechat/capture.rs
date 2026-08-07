use super::profiles::NormalizedRoi;
use super::types::{CaptureVersion, ContractError, RequestId};
use super::window_identity::WechatWindowIdentity;
use crate::monitor::WindowBounds;
use crate::screenshot::CapturedMonitorFrame;
use image::{imageops, RgbaImage};
use tauri::{AppHandle, Manager};

const MAIN_WINDOW_LABEL: &str = "main";

/// Private, in-memory screenshot evidence for one verified WeChat request.
/// It has no serde implementation and must never cross the Tauri boundary.
pub(crate) struct EphemeralCapturedFrame {
    frame: CapturedMonitorFrame,
}

impl EphemeralCapturedFrame {
    pub(crate) fn new(frame: CapturedMonitorFrame) -> Self {
        Self { frame }
    }

    fn crop_window_roi(
        &self,
        bounds_px: WindowBounds,
        dpi: u32,
        roi: NormalizedRoi,
    ) -> Result<RgbaImage, ContractError> {
        crop_physical_roi(self.frame.clone(), bounds_px, dpi, roi)
    }
}

pub(crate) struct WechatCaptureSlices {
    pub(crate) request_id: RequestId,
    pub(crate) capture_version: CaptureVersion,
    pub(crate) chat_rgba: RgbaImage,
    pub(crate) header_identity_rgba: RgbaImage,
}

/// Header-only observation. It deliberately has no request or CaptureVersion:
/// binding observations must not replace the chat capture generation.
pub(crate) struct HeaderObservationFrame {
    pub(crate) rgba: RgbaImage,
}

pub(crate) fn header_for_identity(
    frame: EphemeralCapturedFrame,
    identity: &WechatWindowIdentity,
) -> Result<HeaderObservationFrame, ContractError> {
    Ok(HeaderObservationFrame {
        rgba: frame.crop_window_roi(
            identity.bounds_px(),
            identity.dpi(),
            identity.header_identity_roi(),
        )?,
    })
}

pub(crate) fn slices_for_identity(
    frame: EphemeralCapturedFrame,
    identity: &WechatWindowIdentity,
    request_id: RequestId,
    capture_version: CaptureVersion,
) -> Result<WechatCaptureSlices, ContractError> {
    let (chat_rgba, header_identity_rgba) = cropped_images_for_identity(frame, identity)?;
    Ok(WechatCaptureSlices {
        request_id,
        capture_version,
        chat_rgba,
        header_identity_rgba,
    })
}

pub(crate) fn cropped_images_for_identity(
    frame: EphemeralCapturedFrame,
    identity: &WechatWindowIdentity,
) -> Result<(RgbaImage, RgbaImage), ContractError> {
    let chat_rgba =
        frame.crop_window_roi(identity.bounds_px(), identity.dpi(), identity.chat_roi())?;
    let header_identity_rgba = frame.crop_window_roi(
        identity.bounds_px(),
        identity.dpi(),
        identity.header_identity_roi(),
    )?;
    Ok((chat_rgba, header_identity_rgba))
}

fn crop_physical_roi(
    frame: CapturedMonitorFrame,
    bounds_px: WindowBounds,
    dpi: u32,
    roi: NormalizedRoi,
) -> Result<RgbaImage, ContractError> {
    if dpi == 0 || bounds_px.width == 0 || bounds_px.height == 0 {
        return Err(ContractError::WxCaptureFailed);
    }
    let (frame_width, frame_height) = frame.dimensions();
    let (origin_x, origin_y) = frame.origin_px();
    let window_left = bounds_px
        .x
        .checked_sub(origin_x)
        .ok_or(ContractError::WxCaptureFailed)?;
    let window_top = bounds_px
        .y
        .checked_sub(origin_y)
        .ok_or(ContractError::WxCaptureFailed)?;
    let window_left = u32::try_from(window_left).map_err(|_| ContractError::WxCaptureFailed)?;
    let window_top = u32::try_from(window_top).map_err(|_| ContractError::WxCaptureFailed)?;
    if window_left
        .checked_add(bounds_px.width)
        .map_or(true, |right| right > frame_width)
        || window_top
            .checked_add(bounds_px.height)
            .map_or(true, |bottom| bottom > frame_height)
    {
        return Err(ContractError::WxCaptureFailed);
    }

    let left = (roi.left * f64::from(bounds_px.width)).floor();
    let top = (roi.top * f64::from(bounds_px.height)).floor();
    let right = (roi.right * f64::from(bounds_px.width)).ceil();
    let bottom = (roi.bottom * f64::from(bounds_px.height)).ceil();
    if !left.is_finite()
        || !top.is_finite()
        || !right.is_finite()
        || !bottom.is_finite()
        || left < 0.0
        || top < 0.0
        || right > f64::from(bounds_px.width)
        || bottom > f64::from(bounds_px.height)
        || right <= left
        || bottom <= top
    {
        return Err(ContractError::WxCaptureFailed);
    }

    let image = frame.into_rgba();
    Ok(imageops::crop_imm(
        &image,
        window_left + left as u32,
        window_top + top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    )
    .to_image())
}

struct ProductWindowSnapshot {
    label: &'static str,
    was_visible: bool,
}

/// Minimal, backend-only window surface for the capture guard. Keeping focus
/// operations out of this port makes the no-activation contract testable.
pub(crate) trait WechatWindowPort {
    fn is_visible(&self, label: &'static str) -> Result<Option<bool>, ContractError>;
    fn hide(&self, label: &'static str) -> Result<(), ContractError>;
    fn show(&self, label: &'static str);
}

pub(crate) struct TauriWechatWindowPort {
    app: AppHandle,
}

impl TauriWechatWindowPort {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl WechatWindowPort for TauriWechatWindowPort {
    fn is_visible(&self, label: &'static str) -> Result<Option<bool>, ContractError> {
        self.app
            .get_webview_window(label)
            .map(|window| {
                window
                    .is_visible()
                    .map_err(|_| ContractError::WxCaptureFailed)
            })
            .transpose()
    }

    fn hide(&self, label: &'static str) -> Result<(), ContractError> {
        self.app
            .get_webview_window(label)
            .ok_or(ContractError::WxCaptureFailed)?
            .hide()
            .map_err(|_| ContractError::WxCaptureFailed)
    }

    fn show(&self, label: &'static str) {
        if let Some(window) = self.app.get_webview_window(label) {
            let _ = window.show();
        }
    }
}

/// Hides only existing visible product windows and restores them once, without
/// requesting focus or activation. The normal path calls `finish_after_worker`;
/// Drop exists solely as a panic-safe fallback.
pub(crate) struct WechatCaptureGuard<P: WechatWindowPort> {
    port: P,
    windows: Vec<ProductWindowSnapshot>,
    restored: bool,
}

impl<P: WechatWindowPort> WechatCaptureGuard<P> {
    pub(crate) fn begin(port: P) -> Result<Self, ContractError> {
        let mut guard = Self {
            port,
            windows: Vec::new(),
            restored: false,
        };
        for label in [crate::avatar_engine::AVATAR_WINDOW_LABEL, MAIN_WINDOW_LABEL] {
            let Some(was_visible) = guard.port.is_visible(label)? else {
                continue;
            };
            guard
                .windows
                .push(ProductWindowSnapshot { label, was_visible });
            if was_visible {
                guard.port.hide(label)?;
                if guard.port.is_visible(label)? != Some(false) {
                    guard.restore_once();
                    return Err(ContractError::WxCaptureFailed);
                }
            }
        }
        Ok(guard)
    }

    pub(crate) fn finish_after_worker(&mut self) {
        self.restore_once();
    }

    fn restore_once(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        for snapshot in &self.windows {
            if snapshot.was_visible {
                self.port.show(snapshot.label);
            }
        }
    }
}

impl<P: WechatWindowPort> Drop for WechatCaptureGuard<P> {
    fn drop(&mut self) {
        self.restore_once();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeWindowPort {
        visible: Arc<Mutex<BTreeMap<&'static str, bool>>>,
        calls: Arc<Mutex<Vec<&'static str>>>,
        fail_hide: Option<&'static str>,
    }

    impl FakeWindowPort {
        fn new(avatar_visible: bool, main_visible: bool) -> Self {
            Self {
                visible: Arc::new(Mutex::new(BTreeMap::from([
                    (crate::avatar_engine::AVATAR_WINDOW_LABEL, avatar_visible),
                    (MAIN_WINDOW_LABEL, main_visible),
                ]))),
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_hide: None,
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl WechatWindowPort for FakeWindowPort {
        fn is_visible(&self, label: &'static str) -> Result<Option<bool>, ContractError> {
            Ok(self.visible.lock().unwrap().get(label).copied())
        }

        fn hide(&self, label: &'static str) -> Result<(), ContractError> {
            self.calls.lock().unwrap().push(match label {
                crate::avatar_engine::AVATAR_WINDOW_LABEL => "hide-avatar",
                MAIN_WINDOW_LABEL => "hide-main",
                _ => unreachable!(),
            });
            if self.fail_hide == Some(label) {
                return Err(ContractError::WxCaptureFailed);
            }
            self.visible.lock().unwrap().insert(label, false);
            Ok(())
        }

        fn show(&self, label: &'static str) {
            self.calls.lock().unwrap().push(match label {
                crate::avatar_engine::AVATAR_WINDOW_LABEL => "show-avatar",
                MAIN_WINDOW_LABEL => "show-main",
                _ => unreachable!(),
            });
            self.visible.lock().unwrap().insert(label, true);
        }
    }

    fn frame(origin_px: (i32, i32)) -> CapturedMonitorFrame {
        CapturedMonitorFrame::new(vec![255; 4 * 100 * 80], 100, 80, origin_px).unwrap()
    }

    fn roi() -> NormalizedRoi {
        NormalizedRoi {
            left: 0.1,
            top: 0.25,
            right: 0.6,
            bottom: 0.75,
        }
    }

    #[test]
    fn physical_negative_monitor_origin_is_translated_before_crop() {
        let image = crop_physical_roi(
            frame((-1920, 0)),
            WindowBounds {
                x: -1900,
                y: 10,
                width: 80,
                height: 60,
            },
            96,
            roi(),
        )
        .unwrap();
        assert_eq!(image.dimensions(), (40, 30));
    }

    #[test]
    fn out_of_frame_window_and_invalid_roi_fail_closed() {
        assert_eq!(
            crop_physical_roi(
                frame((0, 0)),
                WindowBounds {
                    x: -1,
                    y: 0,
                    width: 80,
                    height: 60
                },
                96,
                roi(),
            ),
            Err(ContractError::WxCaptureFailed)
        );
        assert_eq!(
            crop_physical_roi(
                frame((0, 0)),
                WindowBounds {
                    x: 0,
                    y: 0,
                    width: 101,
                    height: 60
                },
                96,
                roi(),
            ),
            Err(ContractError::WxCaptureFailed)
        );
        assert_eq!(
            crop_physical_roi(
                frame((0, 0)),
                WindowBounds {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 60
                },
                96,
                NormalizedRoi {
                    left: 0.8,
                    top: 0.0,
                    right: 0.2,
                    bottom: 1.0
                },
            ),
            Err(ContractError::WxCaptureFailed)
        );
    }

    #[test]
    fn guard_restores_only_originally_visible_windows_once_without_activation() {
        let port = FakeWindowPort::new(true, false);
        let mut guard = WechatCaptureGuard::begin(port.clone()).unwrap();
        guard.finish_after_worker();
        guard.finish_after_worker();
        drop(guard);

        assert_eq!(port.calls(), vec!["hide-avatar", "show-avatar"]);
        assert_eq!(
            port.is_visible(crate::avatar_engine::AVATAR_WINDOW_LABEL),
            Ok(Some(true))
        );
        assert_eq!(port.is_visible(MAIN_WINDOW_LABEL), Ok(Some(false)));
    }

    #[test]
    fn guard_restores_snapshot_when_hiding_a_visible_window_fails() {
        let mut port = FakeWindowPort::new(true, false);
        port.fail_hide = Some(crate::avatar_engine::AVATAR_WINDOW_LABEL);

        assert_eq!(
            WechatCaptureGuard::begin(port.clone()).err(),
            Some(ContractError::WxCaptureFailed)
        );
        assert_eq!(port.calls(), vec!["hide-avatar", "show-avatar"]);
        assert_eq!(
            port.is_visible(crate::avatar_engine::AVATAR_WINDOW_LABEL),
            Ok(Some(true))
        );
    }
}
