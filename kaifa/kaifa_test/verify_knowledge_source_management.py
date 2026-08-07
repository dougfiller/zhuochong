#!/usr/bin/env python3
"""Static contract gate for the step-19 redacted source-management surface."""
from pathlib import Path
import sys

root = Path(__file__).resolve().parents[2]
store = (root / "desktop/src-tauri/src/knowledge/store.rs").read_text()
commands = (root / "desktop/src-tauri/src/knowledge/commands.rs").read_text()
ui = (root / "desktop/src/routes/settings/components/SettingsKnowledge.svelte").read_text()
required = [
    "fn list_sources", "fn start_source_import", "fn retire_source", "fn deny_source",
    "fn start_rebuild", "maintenance_status", "list_knowledge_sources",
    "start_knowledge_source_import", "retire_knowledge_source", "deny_knowledge_source",
    "start_knowledge_rebuild", "openDialog({ directory: true, multiple: false })",
    "openDialog({ directory: true, multiple: true })", "source.sourceId.slice(0, 14)",
]
missing = [item for item in required if item not in "\n".join([store, commands, ui])]
if missing or "source.path" in ui:
    print("KNOWLEDGE_SOURCE_MANAGEMENT_GATE: fail " + ", ".join(missing or ["sensitive UI field"]))
    sys.exit(1)
print("KNOWLEDGE_SOURCE_MANAGEMENT_GATE: pass")
