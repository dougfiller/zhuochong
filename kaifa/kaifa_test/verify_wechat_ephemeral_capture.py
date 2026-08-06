#!/usr/bin/env python3
"""Static gate for step 7's private, no-file WeChat capture boundary."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
TAURI = ROOT / "desktop" / "src-tauri" / "src"


def require(path: Path, fragments: list[str]) -> list[str]:
    source = path.read_text(encoding="utf-8")
    return [f"{path.relative_to(ROOT)} missing: {fragment}" for fragment in fragments if fragment not in source]


def main() -> int:
    failures: list[str] = []
    capture = TAURI / "wechat" / "capture.rs"
    runtime = TAURI / "wechat" / "runtime.rs"
    screenshot = TAURI / "screenshot.rs"
    main_rs = TAURI / "main.rs"
    failures += require(capture, ["struct EphemeralCapturedFrame", "trait WechatWindowPort", "struct WechatCaptureGuard", "crop_physical_roi", "checked_sub(origin_x)", "checked_sub(origin_y)", "finish_after_worker"])
    failures += require(runtime, ["struct CaptureCoordinator", "next_capture_version", "is_current_capture", "try_acquire_owned", "capture_foreground_wechat", "run_guarded_capture", "spawn_blocking", "wait_for_capture_worker", "WxRequestCancelled", "worker.await"])
    failures += require(screenshot, ["struct CapturedMonitorFrame", "capture_ephemeral_for_window", "origin_px"])
    failures += require(main_rs, ["CaptureCoordinator>().try_acquire()", "跳过本轮 Work Review 截图"])
    source = capture.read_text(encoding="utf-8")
    forbidden = ["set_focus(", "unminimize(", "reveal_main_window(", "save_as_image("]
    failures += [f"{capture.relative_to(ROOT)} must not contain: {item}" for item in forbidden if item in source]
    if failures:
        print("FAIL")
        print("\n".join(failures))
        return 1
    print("PASS: WeChat ephemeral capture static boundary")
    return 0


if __name__ == "__main__":
    sys.exit(main())
