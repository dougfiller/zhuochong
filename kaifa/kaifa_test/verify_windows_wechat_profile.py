#!/usr/bin/env python3
"""Static fail-closed gate for the Windows WeChat compatibility profile skeleton."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def verify(project_root: Path) -> None:
    tauri_root = project_root / "desktop" / "src-tauri" / "src"
    catalog_path = tauri_root / "wechat" / "profiles" / "windows-wechat-v1.json"
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))

    assert catalog["schema_version"] == 1
    assert catalog["catalog_version"] == "windows-wechat-v1"
    assert catalog["profiles"] == [], "no Windows probe evidence exists, so no profile may be enabled"

    profiles_source = (tauri_root / "wechat" / "profiles.rs").read_text(encoding="utf-8")
    identity_source = (tauri_root / "wechat" / "window_identity.rs").read_text(encoding="utf-8")
    runtime_source = (tauri_root / "wechat" / "runtime.rs").read_text(encoding="utf-8")

    for required in [
        "include_str!",
        "deny_unknown_fields",
        "is_sha256",
        "is_valid",
        "SUPPORTED_CATALOG_VERSION",
        "SUPPORTED_PROFILE_VERSION",
        "filter(|profile| profile.enabled)",
    ]:
        assert required in profiles_source, f"missing catalog fail-closed check: {required}"
    for required in ["WxProfileUnsupported", "WxRequestStale", "sanitize_title_hint", "profile_matches"]:
        assert required in identity_source, f"missing identity gate: {required}"
    monitor_source = (tauri_root / "monitor.rs").read_text(encoding="utf-8")
    for required in ["DwmGetWindowAttribute", "foreground_window_theme(hwnd)", "theme: Some(theme)"]:
        assert required in monitor_source, f"missing direct theme observation: {required}"
    assert "read_foreground_window_evidence()" in runtime_source
    assert "evidence: super::window_identity::ForegroundWindowEvidence" not in runtime_source
    assert "never captures pixels, invokes OCR, retrieval, or a model" in runtime_source


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project-root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    verify(args.project_root.resolve())
    print("windows-wechat compatibility profile static gate: PASS (no enabled profile)")


if __name__ == "__main__":
    main()
