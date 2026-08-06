#!/usr/bin/env python3
"""Static fail-closed gate for the private Windows WeChat OCR path."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def verify(project_root: Path) -> None:
    source_root = project_root / "desktop" / "src-tauri" / "src"
    wechat_ocr = (source_root / "wechat" / "ocr.rs").read_text(encoding="utf-8")
    implementation = wechat_ocr.split("#[cfg(test)]", maxsplit=1)[0]
    ocr_service = (source_root / "ocr.rs").read_text(encoding="utf-8")
    runtime = (source_root / "wechat" / "runtime.rs").read_text(encoding="utf-8")
    profiles = (source_root / "wechat" / "profiles.rs").read_text(encoding="utf-8")
    types = (source_root / "wechat" / "types.rs").read_text(encoding="utf-8")
    catalog = json.loads((source_root / "wechat" / "profiles" / "windows-wechat-v1.json").read_text(encoding="utf-8"))

    assert catalog["profiles"] == [], "production fallback requires a real frozen Windows probe"
    for required in [
        "chat_rgba",
        "MAX_CHAT_PIXELS",
        "NormalizedOcrText::parse",
        "fallback_is_approved",
        "WechatOcrAuditEvent",
    ]:
        assert required in implementation, f"missing OCR safety contract: {required}"
    for required in ["WxOcrEmpty", "WxOcrUnavailable", "WxOcrFailed", "NormalizedOcrText"]:
        assert required in types, f"missing stable OCR error or text gate: {required}"
    for forbidden in [
        "Path", "Command", "powershell", "Paddle", "StorageFile", "http", "tauri::command", "std::fs",
    ]:
        assert forbidden not in implementation, f"forbidden WeChat OCR capability: {forbidden}"
    assert "header_identity_rgba" not in implementation

    memory_start = ocr_service.index("pub(crate) fn extract_windows_ocr_rgba")
    memory_end = ocr_service.index("/// 创建 OCR 服务", memory_start)
    memory_entry = ocr_service[memory_start:memory_end]
    for required in ["SoftwareBitmap", "OcrEngine", "DataWriter", "TryCreateFromUserProfileLanguages"]:
        assert required in memory_entry, f"missing WinRT memory OCR API: {required}"
    for forbidden in ["Command", "powershell", "StorageFile", "temp_dir", "std::fs", "extract_text", "OcrService::new"]:
        assert forbidden not in memory_entry, f"memory OCR regressed to path/process behavior: {forbidden}"

    for required in ["recognize_captured_wechat", "state.fail_ocr", "WindowsMemoryPrimary", "DisabledLocalFallback"]:
        assert required in runtime, f"missing private OCR-to-failed routing: {required}"
    for required in [
        "ocr_fallback_audit",
        "probe_outcome",
        "probe_monitors",
        "compiled-local-memory-v1",
        "is_approved_for",
        "probe_evidence_sha256: raw.probe_evidence.evidence_sha256",
        "self.probe_evidence_sha256 == profile.probe_evidence_sha256",
        "fallback_audit_must_bind_to_one_exact_failed_or_unavailable_probe",
        "Err(CatalogError::InvalidCatalog)",
    ]:
        assert required in profiles, f"missing frozen fallback audit gate: {required}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project-root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    verify(args.project_root.resolve())
    print("wechat Windows OCR static gate: PASS (memory-only, fallback disabled in production catalog)")


if __name__ == "__main__":
    main()
