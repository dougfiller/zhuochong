#!/usr/bin/env python3
"""Create and verify the frozen Work Review source baseline for step 1."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath


UPSTREAM_URL = "https://github.com/wm94i/Work-Review"
UPSTREAM_GIT_URL = f"{UPSTREAM_URL}.git"
MANIFEST_SCHEMA_VERSION = 2
METADATA_FILES = {
    "work-review-source.json",
    "reference-ledger.md",
    "third-party-assets.md",
}
EXCLUDED_DIRECTORY_NAMES = {
    ".git",
    ".cache",
    "__pycache__",
    "dist",
    "node_modules",
    "runtime-data",
    "target",
}
EXCLUDED_FILE_SUFFIXES = {".db", ".dll", ".key", ".log", ".p12", ".pem", ".pfx", ".sqlite", ".sqlite3"}
REQUIRED_PATHS = {
    ".github/workflows",
    "Cargo.lock",
    "LICENSE",
    "THIRD_PARTY_NOTICES.md",
    "crates",
    "docs",
    "package-lock.json",
    "package.json",
    "src",
    "src-tauri",
}
FORBIDDEN_PATH_PARTS = {"参考", "人物角色参考", "微信聊天记录知识库"}


class UpstreamUnavailable(ValueError):
    """The official source could not be fetched for an online verification."""


def posix_sort_key(value: str) -> bytes:
    return value.encode("utf-8")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_sha256(value: object) -> str:
    encoded = json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def is_excluded(relative: PurePosixPath) -> bool:
    if any(part in EXCLUDED_DIRECTORY_NAMES for part in relative.parts[:-1]):
        return True
    name = relative.name
    return name == ".env" or name.startswith(".env.") or relative.suffix.lower() in EXCLUDED_FILE_SUFFIXES


def source_files(root: Path, omit_product_metadata: bool = False) -> list[dict[str, object]]:
    files: list[dict[str, object]] = []
    for path in root.rglob("*"):
        relative = PurePosixPath(path.relative_to(root).as_posix())
        if omit_product_metadata and relative.parts[:2] == ("docs", "baselines"):
            continue
        if is_excluded(relative):
            continue
        if path.is_symlink():
            raise ValueError(f"symbolic links are not allowed in the frozen source set: {path}")
        if not path.is_file():
            continue
        files.append({"path": str(relative), "bytes": path.stat().st_size, "sha256": sha256(path)})
    return sorted(files, key=lambda item: posix_sort_key(str(item["path"])))


def require_source(source: Path, version: str) -> None:
    if not source.is_dir():
        raise ValueError(f"reference source is not a directory: {source}")
    if (source / ".git").exists():
        raise ValueError("reference source unexpectedly has Git metadata; do not replace the audited input")
    package_version = json.loads((source / "package.json").read_text(encoding="utf-8"))["version"]
    tauri_version = json.loads((source / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"]
    if package_version != version or tauri_version != version:
        raise ValueError(f"declared version mismatch: package={package_version}, tauri={tauri_version}, expected={version}")
    missing = sorted(path for path in REQUIRED_PATHS if not (source / path).exists())
    if missing:
        raise ValueError(f"reference source misses required paths: {', '.join(missing)}")


def upstream_snapshot(source: Path, tag: str, commit: str) -> dict[str, object]:
    """Fetch the fixed tag, archive its commit, and compare it with the local input."""
    with tempfile.TemporaryDirectory(prefix=".work-review-upstream-") as temporary_name:
        temporary = Path(temporary_name)
        repository = temporary / "repository"
        archive_root = temporary / "archive"
        try:
            subprocess.run(
                ["git", "clone", "--depth", "1", "--branch", tag, UPSTREAM_GIT_URL, str(repository)],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            resolved_tag = subprocess.run(
                ["git", "-C", str(repository), "rev-parse", f"refs/tags/{tag}^{{}}"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            ).stdout.strip()
            if resolved_tag != commit:
                raise ValueError(f"upstream tag {tag} resolves to {resolved_tag}, expected {commit}")
            tree = subprocess.run(
                ["git", "-C", str(repository), "rev-parse", f"{commit}^{{tree}}"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            ).stdout.strip()
            archive = subprocess.run(
                ["git", "-C", str(repository), "archive", "--format=tar", commit],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            ).stdout
        except subprocess.CalledProcessError as error:
            detail = error.stderr.strip() if isinstance(error.stderr, str) else "git command failed"
            raise UpstreamUnavailable(f"could not fetch upstream {tag}/{commit}: {detail}") from error
        archive_root.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as bundle:
            for member in bundle.getmembers():
                relative = PurePosixPath(member.name)
                if relative.is_absolute() or ".." in relative.parts:
                    raise ValueError(f"unsafe path in upstream archive: {member.name}")
                if member.isdir():
                    continue
                if not member.isfile():
                    raise ValueError(f"non-file entry in upstream archive: {member.name}")
                target = archive_root.joinpath(*relative.parts)
                target.parent.mkdir(parents=True, exist_ok=True)
                extracted = bundle.extractfile(member)
                if extracted is None:
                    raise ValueError(f"could not read upstream archive entry: {member.name}")
                with target.open("wb") as handle:
                    shutil.copyfileobj(extracted, handle)
        official_files = source_files(archive_root)
    local_files = source_files(source)
    if local_files != official_files:
        local_by_path = {str(item["path"]): item for item in local_files}
        official_by_path = {str(item["path"]): item for item in official_files}
        changed = sorted(path for path in local_by_path.keys() & official_by_path.keys() if local_by_path[path] != official_by_path[path])
        only_local = sorted(local_by_path.keys() - official_by_path.keys())
        only_official = sorted(official_by_path.keys() - local_by_path.keys())
        details = changed[:5] + only_local[:5] + only_official[:5]
        raise ValueError(
            "reference snapshot does not match the fixed upstream commit; "
            f"changed_or_missing={', '.join(details) or 'unknown'}"
        )
    return {
        "status": "verified",
        "git_url": UPSTREAM_GIT_URL,
        "tag": tag,
        "commit": commit,
        "tree": tree,
        "acquisition_command": "git clone --depth 1 --branch <tag> <git_url>; git archive --format=tar <commit>",
        "official_snapshot": {
            "file_count": len(official_files),
            "total_bytes": sum(int(item["bytes"]) for item in official_files),
            "files_sha256": canonical_sha256(official_files),
        },
    }


def copy_snapshot(source: Path, temporary: Path, files: list[dict[str, object]]) -> None:
    for item in files:
        relative = Path(str(item["path"]))
        target = temporary / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source / relative, target)


def asset_count(source: Path, prefix: str, suffixes: set[str] | None = None) -> int:
    count = 0
    for path in source.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(source).as_posix()
        if not relative.startswith(prefix):
            continue
        if suffixes is None or path.suffix.lower() in suffixes:
            count += 1
    return count


def write_metadata(
    destination: Path,
    files: list[dict[str, object]],
    version: str,
    tag: str,
    commit: str,
    captured_at: str,
    upstream_evidence: dict[str, object],
) -> None:
    baseline_dir = destination / "docs" / "baselines"
    baseline_dir.mkdir(parents=True, exist_ok=True)
    total_bytes = sum(int(item["bytes"]) for item in files)
    manifest = {
        "schema_version": MANIFEST_SCHEMA_VERSION,
        "source": {
            "upstream_url": UPSTREAM_URL,
            "declared_version": version,
            "resolved_tag": tag,
            "resolved_commit": commit,
            "local_git_metadata": False,
            "captured_at": captured_at,
            "acquisition_method": "local reference snapshot verified against an official fixed-commit archive",
            "upstream_verification": upstream_evidence,
        },
        "snapshot": {
            "hash_algorithm": "sha256",
            "path_order": "POSIX bytewise ascending",
            "files": files,
        },
        "exclusions": [
            ".git/**",
            "node_modules/**",
            "dist/**",
            "**/target/**",
            ".cache/**",
            "**/__pycache__/**",
            ".env*",
            "runtime-data/**",
            "*.db",
            "*.sqlite*",
            "*.dll",
            "*.pem",
            "*.key",
            "*.p12",
            "*.pfx",
            "*.log",
        ],
        "verification": {"source_file_count": len(files), "source_total_bytes": total_bytes},
    }
    (baseline_dir / "work-review-source.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    (baseline_dir / "reference-ledger.md").write_text(
        (
            "# 参考与补丁台账\n\n"
            f"冻结时间：{captured_at}  \n"
            "本台账只覆盖步骤 1；`参考/Work-Review-main/` 是只读输入，后续产品改动仅在 `desktop/` 中进行。\n\n"
            "| 参考对象与来源 URL | 上游 tag/commit、原文件 | 参考行为/用途 | 复制结论与产品修改位置 | 修改声明、冲突风险、升级策略 |\n"
            "| --- | --- | --- | --- | --- |\n"
            f"| Work Review — {UPSTREAM_URL} | `{tag}` / `{commit}`；完整本地参考快照 | 正式 Tauri 2 + Rust + Svelte 产品底座 | 完整复制至 `desktop/`；本步骤仅新增 `docs/baselines/` 元数据，无业务修改 | 后续改动必须作为补丁追加并保留本 manifest；升级需新建冻结记录与显式 diff，不能覆盖本记录 |\n"
            "| VPet — 未在本步骤使用 | 不适用 | 后续能力评估参考 | 本步骤未复制 | 如后续采用，先追加来源、许可和补丁记录 |\n"
            "| Umi-OCR — 未在本步骤使用 | 不适用 | 后续 OCR 能力评估参考 | 本步骤未复制 | 如后续采用，先追加来源、许可和补丁记录 |\n"
            "| Open-LLM-VTuber — 未在本步骤使用 | 不适用 | 后续模型接入评估参考 | 本步骤未复制 | 如后续采用，先追加来源、许可和补丁记录 |\n"
        ),
        encoding="utf-8",
    )
    documentation_assets = asset_count(destination, "docs/", {".gif", ".png", ".svg"})
    product_icons = asset_count(destination, "public/", {".icns", ".ico", ".png", ".svg"}) + asset_count(
        destination, "src-tauri/icons/", {".icns", ".ico", ".png", ".svg"}
    )
    bongocat_assets = asset_count(destination, "src/lib/components/Avatar/assets/bongocat/", {".png"})
    (baseline_dir / "third-party-assets.md").write_text(
        (
            "# 第三方与资产台账\n\n"
            f"冻结时间：{captured_at}。本表记录的是复制时的权利状态，不把 `pending-verification` 视为可发布授权。\n\n"
            "| 范围 | 来源位置 | 权利人/上游 | 许可证或证据 | 商业授权证据 | 发行状态 |\n"
            "| --- | --- | --- | --- | --- |\n"
            "| Work Review 源码与文档 | `LICENSE` | wm94i | MIT，保留原文件 | MIT 允许商业使用，仍需随副本保留声明 | 已随副本保留 |\n"
            f"| BongoCat 交互与视觉资源（{bongocat_assets} 个 PNG） | `src/lib/components/Avatar/assets/bongocat/**` | ayangweb/BongoCat | `THIRD_PARTY_NOTICES.md` 声明 MIT | MIT notice 已保留；后续发行前核对上游版权声明 | pending-verification |\n"
            f"| 应用图标与供应商标识（{product_icons} 个文件） | `public/icon.png`、`public/icons/**`、`src-tauri/icons/**` | Work Review 上游或各标识权利人 | 仅有 Work Review 仓库来源，逐项权利未证明 | 无单独商业授权证据 | pending-verification |\n"
            f"| 文档截图、动图与宣传图（{documentation_assets} 个文件） | `docs/**` | Work Review 上游或各图中内容权利人 | 仓库来源；截图中的第三方内容未逐项核对 | 无单独商业授权证据 | pending-verification |\n"
            "\n发行阻断：任何 `pending-verification` 资产在后续发行前必须补充可复核的来源、许可证或授权证据；本步骤不声称这些资产已获商业授权。\n"
        ),
        encoding="utf-8",
    )


def load_manifest(destination: Path) -> dict[str, object]:
    path = destination / "docs" / "baselines" / "work-review-source.json"
    return json.loads(path.read_text(encoding="utf-8"))


def verify(destination: Path, source: Path, version: str, tag: str, commit: str) -> None:
    if not destination.is_dir():
        raise ValueError(f"product copy is not a directory: {destination}")
    manifest = load_manifest(destination)
    source_info = manifest.get("source", {})
    expected_identity = {
        "upstream_url": UPSTREAM_URL,
        "declared_version": version,
        "resolved_tag": tag,
        "resolved_commit": commit,
        "local_git_metadata": False,
    }
    for field, expected in expected_identity.items():
        if source_info.get(field) != expected:
            raise ValueError(f"manifest source.{field} is not the frozen value")
    if manifest.get("schema_version") != MANIFEST_SCHEMA_VERSION:
        raise ValueError("manifest has no content-level upstream verification evidence")
    files = manifest.get("snapshot", {}).get("files")
    if not isinstance(files, list) or not files:
        raise ValueError("manifest contains no source files")
    paths = [item.get("path") for item in files]
    if paths != sorted(paths, key=lambda value: posix_sort_key(str(value))) or len(paths) != len(set(paths)):
        raise ValueError("manifest paths are not uniquely POSIX-bytewise sorted")
    for item in files:
        if not isinstance(item.get("bytes"), int) or item["bytes"] < 0 or len(str(item.get("sha256", ""))) != 64:
            raise ValueError(f"invalid file entry: {item}")
    expected_files = source_files(source)
    if files != expected_files:
        raise ValueError("manifest does not match the read-only reference snapshot")
    actual_files = source_files(destination, omit_product_metadata=True)
    if actual_files != files:
        raise ValueError("product source file set does not match the frozen manifest")
    metadata_dir = destination / "docs" / "baselines"
    metadata_paths = {
        path.relative_to(metadata_dir).as_posix() for path in metadata_dir.rglob("*") if path.is_file()
    }
    if metadata_paths != METADATA_FILES or any(path.is_dir() for path in metadata_dir.rglob("*")):
        raise ValueError("baseline metadata directory contains missing or unexpected files")
    missing = sorted(path for path in REQUIRED_PATHS if not (destination / path).exists())
    if missing:
        raise ValueError(f"product copy misses required paths: {', '.join(missing)}")
    if "MIT License" not in (destination / "LICENSE").read_text(encoding="utf-8"):
        raise ValueError("MIT LICENSE was not preserved")
    if "BongoCat" not in (destination / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8"):
        raise ValueError("BongoCat third-party notice was not preserved")
    for path in destination.rglob("*"):
        relative = PurePosixPath(path.relative_to(destination).as_posix())
        if any(part in EXCLUDED_DIRECTORY_NAMES or part in FORBIDDEN_PATH_PARTS for part in relative.parts):
            raise ValueError(f"forbidden directory or path found: {relative}")
        if path.is_file() and is_excluded(relative):
            raise ValueError(f"forbidden file found: {relative}")
    try:
        current_evidence = upstream_snapshot(source, tag, commit)
    except UpstreamUnavailable as error:
        print(
            "verified local snapshot only: "
            f"files={len(files)} bytes={manifest['verification']['source_total_bytes']}; "
            f"upstream identity was not revalidated ({error})"
        )
        return
    frozen_evidence = source_info.get("upstream_verification")
    if not isinstance(frozen_evidence, dict) or frozen_evidence != current_evidence:
        raise ValueError("manifest upstream verification evidence does not match the official fixed-commit archive")
    print(
        "verified upstream baseline: "
        f"files={len(files)} bytes={manifest['verification']['source_total_bytes']} tag={tag} commit={commit} "
        f"tree={current_evidence['tree']}"
    )


def create(arguments: argparse.Namespace) -> None:
    source = arguments.source.resolve()
    destination = arguments.destination.resolve()
    require_source(source, arguments.version)
    if destination.exists():
        raise ValueError(f"refusing to overwrite existing product directory: {destination}")
    files = source_files(source)
    upstream_evidence = upstream_snapshot(source, arguments.tag, arguments.commit)
    captured_at = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    temporary = Path(tempfile.mkdtemp(prefix=".desktop-baseline-", dir=destination.parent))
    try:
        copy_snapshot(source, temporary, files)
        write_metadata(
            temporary,
            files,
            arguments.version,
            arguments.tag,
            arguments.commit,
            captured_at,
            upstream_evidence,
        )
        verify(temporary, source, arguments.version, arguments.tag, arguments.commit)
        os.replace(temporary, destination)
    except Exception:
        shutil.rmtree(temporary, ignore_errors=True)
        raise
    print(f"created product baseline: {destination}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("create", "verify"))
    parser.add_argument("--source", type=Path, required=True, help="read-only Work Review reference directory")
    parser.add_argument("--destination", type=Path, required=True, help="desktop product directory")
    parser.add_argument("--version", default="1.1.0")
    parser.add_argument("--tag", default="v1.1.0")
    parser.add_argument("--commit", default="500f9d2cb3027392cfcc32ad18395dfe348fb4a1")
    return parser.parse_args()


def main() -> int:
    arguments = parse_args()
    try:
        if arguments.command == "create":
            create(arguments)
        else:
            require_source(arguments.source.resolve(), arguments.version)
            verify(arguments.destination.resolve(), arguments.source.resolve(), arguments.version, arguments.tag, arguments.commit)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"baseline verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
