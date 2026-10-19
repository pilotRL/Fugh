#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="$project_root/target/release/fugh-ide"
bin_dir="$HOME/.local/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
applications_dir="$data_dir/applications"
icons_dir="$data_dir/icons/hicolor/256x256/apps"
mime_packages_dir="$data_dir/mime/packages"

if [[ ! -x "$binary" ]]; then
    printf 'Release binary not found at %s. Run cargo build --release first.\n' "$binary" >&2
    exit 1
fi

install -D -m 755 "$binary" "$bin_dir/fugh-ide"
install -d -m 755 "$applications_dir"
install -D -m 644 "$project_root/assets/devicon/fugh-icon.png" "$icons_dir/fugh-logo.png"
install -D -m 644 "$project_root/assets/devicon/fugh-icon.png" \
    "$project_root/target/release/fugh-icon.png"
install -D -m 644 "$project_root/packaging/linux/application-x-fugh-project.xml" \
    "$mime_packages_dir/application-x-fugh-project.xml"
icon_path="$icons_dir/fugh-logo.png"

escaped_exec="$bin_dir/fugh-ide"
escaped_exec="${escaped_exec//\\/\\\\}"
escaped_exec="${escaped_exec//\"/\\\"}"

desktop_file="$applications_dir/io.github.pilotrl.fugh.desktop"
splash_desktop_file="$applications_dir/io.github.pilotrl.fughsplash.desktop"
for legacy_desktop_file in \
    "$applications_dir/Fugh.desktop" \
    "$applications_dir/fugh.desktop" \
    "$applications_dir/fugh-ide.desktop" \
    "$applications_dir/io.github.pilotrl.Fugh.desktop"; do
    if [[ -f "$legacy_desktop_file" ]] \
        && grep -Fqx 'Comment=Desktop IDE for C++ and C# projects' "$legacy_desktop_file" \
        && grep -Fqx 'MimeType=application/x-fugh-project;' "$legacy_desktop_file"; then
        rm -f -- "$legacy_desktop_file"
    fi
done
legacy_splash_desktop="$applications_dir/io.github.pilotrl.FughSplash.desktop"
if [[ -f "$legacy_splash_desktop" ]] \
    && grep -Fqx 'Comment=Fugh startup splash window' "$legacy_splash_desktop"; then
    rm -f -- "$legacy_splash_desktop"
fi
cat > "$desktop_file" <<EOF
[Desktop Entry]
Type=Application
Name=Fugh
Comment=Desktop IDE for C++ and C# projects
Exec="$escaped_exec" --open-project %f
Icon=$icon_path
MimeType=application/x-fugh-project;
Terminal=false
Categories=Development;IDE;
Keywords=code;editor;C++;C#;development;
StartupNotify=true
StartupWMClass=io.github.pilotrl.fugh
EOF
cat > "$splash_desktop_file" <<EOF
[Desktop Entry]
Type=Application
Name=Fugh Startup Splash
Comment=Fugh startup splash window
Exec="$escaped_exec" --fugh-startup-splash
Icon=$icon_path
Terminal=false
NoDisplay=true
StartupNotify=false
StartupWMClass=io.github.pilotrl.fughsplash
EOF

# ELF executables do not carry portable desktop icons. Add a normal launcher
# beside the release binary so the build output has a branded clickable item.
output_launcher="$project_root/target/release/Fugh IDE.desktop"
output_binary="$project_root/target/release/fugh-ide"
escaped_output_exec="$output_binary"
escaped_output_exec="${escaped_output_exec//\\/\\\\}"
escaped_output_exec="${escaped_output_exec//\"/\\\"}"
cat > "$output_launcher" <<EOF
[Desktop Entry]
Type=Application
Name=Fugh IDE
Comment=Desktop IDE for C++ and C# projects
Exec="$escaped_output_exec"
Icon=$project_root/target/release/fugh-icon.png
Terminal=false
Categories=Development;IDE;
StartupNotify=true
StartupWMClass=io.github.pilotrl.fugh
EOF
chmod 755 "$output_launcher"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$applications_dir"
fi
mime_database_ready=0
if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$data_dir/mime"
    mime_database_ready=1
else
    printf 'update-mime-database is unavailable; install shared-mime-info and run the installer again to register .fugh files.\n' >&2
fi
if [[ "$mime_database_ready" == 1 ]] && command -v xdg-mime >/dev/null 2>&1; then
    xdg-mime default io.github.pilotrl.fugh.desktop application/x-fugh-project
    printf 'Registered application/x-fugh-project to open with Fugh.\n'
elif [[ "$mime_database_ready" == 0 ]]; then
    printf 'The .fugh association was not set because the MIME database could not be updated.\n' >&2
else
    printf 'xdg-mime is unavailable; install xdg-utils and run the installer again to set .fugh as a double-click project file.\n' >&2
fi

printf 'Installed Fugh for this user. Launch it from the application menu or double-click a .fugh project file.\n'
