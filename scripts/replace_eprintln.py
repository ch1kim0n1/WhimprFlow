#!/usr/bin/env python3
from pathlib import Path

FILES = [
    Path("src-tauri/src/win.rs"),
    Path("src-tauri/src/linux.rs"),
    Path("src-tauri/src/hotkey.rs"),
    Path("src-tauri/src/local_llm.rs"),
    Path("src-tauri/src/lib.rs"),
    Path("src-tauri/src/autolearn.rs"),
]

REPL = 'tracing::info!(target: "whimpr", '

for f in FILES:
    text = f.read_text(encoding="utf-8")
    n = text.count("eprintln!")
    text2 = text.replace("eprintln!(", REPL)
    f.write_text(text2, encoding="utf-8", newline="\n")
    print(f"{f}: {n} replaced, {text2.count('eprintln!')} remaining")
