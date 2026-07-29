#!/usr/bin/env bash
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd /tmp/WhimprFlow
cargo --version
set +e
cargo check -p whimpr-tauri --message-format=short 2>&1 | tee /tmp/wf-check.log | tail -n 120
code=${PIPESTATUS[0]}
echo "EXIT:$code"
exit "$code"
