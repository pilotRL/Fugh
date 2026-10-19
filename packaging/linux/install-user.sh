#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="$project_root/target/release/fugh-ide"
bin_dir="$HOME/.local/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
applications_dir="$data_dir/applications"

if [[ ! -x "$binary" ]]; then
    printf 'Release binary not found at %s. Run cargo build --release first.\n' "$binary" >&2
    exit 1
fi

install -D -m 755 "$binary" "$bin_dir/fugh-ide"
install -d -m 755 "$applications_dir"

escaped_exec="$bin_dir/fugh-ide"
escaped_exec="${escaped_exec//\\/\\\\}"
escaped_exec="${escaped_exec//\"/\\\"}"

desktop_file="$applications_dir/fugh-ide.desktop"
cat > "$desktop_file" <<EOF
[Desktop Entry]
Type=Application
Name=Fugh
Comment=Desktop IDE for C++ and C# projects
Exec="$escaped_exec"
Terminal=false
Categories=Development;IDE;
Keywords=code;editor;C++;C#;development;
StartupNotify=true
StartupWMClass=Fugh
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$applications_dir" || true
fi

printf 'Installed Fugh for this user. Launch it from the application menu or run %s/fugh-ide.\n' "$bin_dir"
