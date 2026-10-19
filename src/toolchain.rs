use std::{
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use eframe::egui;

#[cfg(target_os = "linux")]
static IS_FEDORA: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

#[derive(Debug, Clone)]
pub struct ToolStatus {
    pub name: &'static str,
    pub command: &'static str,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageAction {
    InstallMissing,
    InstallSelected(Vec<String>),
    UpdateInstalled,
}

impl PackageAction {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::InstallMissing => "Install missing tools",
            Self::InstallSelected(_) => "Install selected tools",
            Self::UpdateInstalled => "Update installed tools",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PackageOperation {
    pub running: bool,
    pub output: Vec<String>,
    pub result: Option<Result<(), String>>,
}

#[derive(Clone)]
pub struct ToolchainManager {
    tools: Arc<Mutex<Vec<ToolStatus>>>,
    refreshing: Arc<AtomicBool>,
    package_operation: Arc<Mutex<PackageOperation>>,
}

impl Default for ToolchainManager {
    fn default() -> Self {
        Self {
            tools: Arc::new(Mutex::new(Vec::new())),
            refreshing: Arc::new(AtomicBool::new(false)),
            package_operation: Arc::new(Mutex::new(PackageOperation::default())),
        }
    }
}

impl ToolchainManager {
    pub fn supports_fedora_package_management() -> bool {
        #[cfg(target_os = "linux")]
        {
            *IS_FEDORA.get_or_init(|| {
                std::fs::read_to_string("/etc/os-release")
                    .map(|release| is_fedora_release(&release))
                    .unwrap_or(false)
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    pub fn tools(&self) -> Vec<ToolStatus> {
        self.tools
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn is_refreshing(&self) -> bool {
        self.refreshing.load(Ordering::Relaxed)
    }

    pub fn package_operation(&self) -> PackageOperation {
        self.package_operation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn record_package_error(&self, error: String) {
        let mut operation = self
            .package_operation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        operation.running = false;
        operation.result = Some(Err(error));
    }

    pub fn planned_packages(&self, action: &PackageAction) -> Vec<String> {
        let tools = self.tools();
        let mut packages = tools
            .iter()
            .filter(|tool| match action {
                PackageAction::InstallMissing => tool.version.is_none(),
                PackageAction::InstallSelected(names) => {
                    tool.version.is_none() && names.iter().any(|name| name == tool.name)
                }
                PackageAction::UpdateInstalled => tool.version.is_some(),
            })
            .flat_map(|tool| packages_for_tool(tool.name))
            .map(|package| (*package).to_owned())
            .collect::<Vec<_>>();
        if Self::supports_fedora_package_management() {
            let selected = |name: &str| match action {
                PackageAction::InstallMissing => true,
                PackageAction::InstallSelected(names) => names.iter().any(|item| item == name),
                PackageAction::UpdateInstalled => false,
            };
            if selected("C# debugger")
                && tools
                    .iter()
                    .any(|tool| tool.name == "C# debugger" && tool.version.is_none())
            {
                packages.extend(["curl".to_owned(), "tar".to_owned()]);
            }
            if selected("Roslyn language server")
                && tools
                    .iter()
                    .any(|tool| tool.name == "Roslyn language server" && tool.version.is_none())
                && tools
                    .iter()
                    .any(|tool| tool.name == ".NET SDK" && tool.version.is_none())
            {
                packages.push("dotnet-sdk-10.0".to_owned());
            }
        }
        packages.sort_unstable();
        packages.dedup();
        packages
    }

    pub fn tool_has_install_package(name: &str) -> bool {
        !packages_for_tool(name).is_empty()
    }

    pub fn tool_has_supported_installer(name: &str) -> bool {
        Self::tool_has_install_package(name)
            || matches!(name, "Roslyn language server" | "C# debugger")
    }

    pub fn netcoredbg_executable() -> Option<std::path::PathBuf> {
        if Command::new("netcoredbg")
            .arg("--help")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return Some(std::path::PathBuf::from("netcoredbg"));
        }
        let root = managed_tools_dir()?.join("netcoredbg/current");
        find_executable(&root, "netcoredbg")
    }

    pub fn start_tool_install_action(
        &self,
        action: PackageAction,
        repaint: egui::Context,
    ) -> Result<(), String> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (action, repaint);
            Err(
                "One-click package management is currently supported on Fedora Linux only."
                    .to_owned(),
            )
        }

        #[cfg(target_os = "linux")]
        {
            let install_roslyn = self.tools().iter().any(|tool| {
                tool.name == "Roslyn language server"
                    && tool.version.is_none()
                    && match &action {
                        PackageAction::InstallMissing => true,
                        PackageAction::InstallSelected(names) => {
                            names.iter().any(|name| name == "Roslyn language server")
                        }
                        PackageAction::UpdateInstalled => false,
                    }
            });
            let install_netcoredbg = self.tools().iter().any(|tool| {
                tool.name == "C# debugger"
                    && tool.version.is_none()
                    && match &action {
                        PackageAction::InstallMissing => true,
                        PackageAction::InstallSelected(names) => {
                            names.iter().any(|name| name == "C# debugger")
                        }
                        PackageAction::UpdateInstalled => false,
                    }
            });
            let fedora_supported = Self::supports_fedora_package_management();
            let packages = if fedora_supported {
                self.planned_packages(&action)
            } else {
                Vec::new()
            };
            if !fedora_supported && !install_roslyn && !install_netcoredbg {
                return Err(
                    "One-click package management is currently supported on Fedora Linux only."
                        .to_owned(),
                );
            }
            if packages.is_empty() && !install_roslyn && !install_netcoredbg {
                let unsupported_missing = self
                    .tools()
                    .into_iter()
                    .filter(|tool| {
                        tool.version.is_none()
                            && packages_for_tool(tool.name).is_empty()
                            && tool.name != "Roslyn language server"
                    })
                    .map(|tool| tool.name)
                    .collect::<Vec<_>>();
                return Err(match &action {
                    PackageAction::InstallMissing => {
                        if unsupported_missing.is_empty() {
                            "No missing tools are available through Fedora package management."
                                .to_owned()
                        } else {
                            format!(
                                "{} is missing but is not available through the Fedora package installer. Install NetCoreDbg from the official release page: https://github.com/Samsung/netcoredbg/releases",
                                unsupported_missing.join(", ")
                            )
                        }
                    }
                    PackageAction::InstallSelected(_) => {
                        "No selected missing tools have a supported package.".to_owned()
                    }
                    PackageAction::UpdateInstalled => {
                        "No installed supported tools to update.".to_owned()
                    }
                });
            }

            let mut operation = self
                .package_operation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if operation.running {
                return Err("A package operation is already running.".to_owned());
            }
            operation.running = true;
            operation.output.clear();
            operation.result = None;
            drop(operation);

            let state = self.package_operation.clone();
            let tools = self.tools.clone();
            let refreshing = self.refreshing.clone();
            thread::Builder::new()
                .name("cairn-tool-install-action".to_owned())
                .spawn(move || {
                    let mut lines = Vec::new();
                    let mut failure = None;
                    if !packages.is_empty() {
                        let mut command = Command::new("pkexec");
                        command.arg("dnf");
                        match &action {
                            PackageAction::InstallMissing | PackageAction::InstallSelected(_) => {
                                command.args(["install", "--assumeyes"]);
                            }
                            PackageAction::UpdateInstalled => {
                                command.args(["upgrade", "--refresh", "--assumeyes"]);
                            }
                        }
                        match command.args(&packages).output() {
                            Ok(output) => {
                                lines.extend(
                                    String::from_utf8_lossy(&output.stdout)
                                        .lines()
                                        .map(str::to_owned),
                                );
                                lines.extend(
                                    String::from_utf8_lossy(&output.stderr)
                                        .lines()
                                        .map(str::to_owned),
                                );
                                if !output.status.success() {
                                    let code = output.status.code().map_or_else(
                                        || "terminated by signal".to_owned(),
                                        |code| code.to_string(),
                                    );
                                    failure = Some(format!(
                                        "Fedora package operation failed (exit {code})."
                                    ));
                                }
                            }
                            Err(error) => {
                                failure = Some(format!("Could not start pkexec: {error}"))
                            }
                        }
                    }
                    if failure.is_none() && install_roslyn {
                        lines.push(
                            "Installing Roslyn language server as a global .NET tool…".to_owned(),
                        );
                        match Command::new("dotnet")
                            .args([
                                "tool",
                                "install",
                                "--global",
                                "roslyn-language-server",
                                "--prerelease",
                            ])
                            .output()
                        {
                            Ok(output) => {
                                lines.extend(
                                    String::from_utf8_lossy(&output.stdout)
                                        .lines()
                                        .map(str::to_owned),
                                );
                                lines.extend(
                                    String::from_utf8_lossy(&output.stderr)
                                        .lines()
                                        .map(str::to_owned),
                                );
                                if !output.status.success() {
                                    let code = output.status.code().map_or_else(
                                        || "terminated by signal".to_owned(),
                                        |code| code.to_string(),
                                    );
                                    failure = Some(format!(
                                        "Roslyn tool installation failed (exit {code})."
                                    ));
                                }
                            }
                            Err(error) => {
                                failure = Some(format!(
                                    "Could not start dotnet to install Roslyn: {error}"
                                ))
                            }
                        }
                    }
                    if failure.is_none() && install_netcoredbg {
                        lines.push("Downloading NetCoreDbg from the official Samsung/netcoredbg GitHub release…".to_owned());
                        match install_netcoredbg_release() {
                            Ok(version) => lines.push(format!("Installed NetCoreDbg {version} in Fugh's user-local tools directory.")),
                            Err(error) => failure = Some(error),
                        }
                    }
                    *tools
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = detect_tools();
                    let result = if let Some(error) = failure {
                        Err((lines, error))
                    } else {
                        Ok(lines)
                    };
                    let mut operation = state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    match result {
                        Ok(lines) => {
                            operation.output = lines;
                            operation.result = Some(Ok(()));
                        }
                        Err((lines, error)) => {
                            operation.output = lines;
                            operation.output.push(error.clone());
                            operation.result = Some(Err(error));
                        }
                    }
                    operation.running = false;
                    refreshing.store(false, Ordering::Relaxed);
                    repaint.request_repaint();
                })
                .map_err(|error| {
                    let mut operation = self
                        .package_operation
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    operation.running = false;
                    let message = format!("Could not start Fedora package worker: {error}");
                    operation.result = Some(Err(message.clone()));
                    message
                })?;
            Ok(())
        }
    }

    pub fn refresh(&self, repaint: egui::Context) {
        if self
            .refreshing
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let tools = self.tools.clone();
        let refreshing = self.refreshing.clone();
        let result = thread::Builder::new()
            .name("cairn-toolchain-detection".to_owned())
            .spawn(move || {
                let detected = detect_tools();
                *tools
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = detected;
                refreshing.store(false, Ordering::Relaxed);
                repaint.request_repaint();
            });
        if let Err(error) = result {
            self.refreshing.store(false, Ordering::Relaxed);
            *self
                .tools
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = vec![ToolStatus {
                name: "Detection",
                command: "thread",
                version: Some(format!("Could not start detection: {error}")),
            }];
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn is_fedora_release(os_release: &str) -> bool {
    os_release.lines().any(|line| {
        line.strip_prefix("ID=")
            .map(|id| id.trim_matches('"').eq_ignore_ascii_case("fedora"))
            .unwrap_or(false)
    })
}

fn packages_for_tool(name: &str) -> &'static [&'static str] {
    match name {
        "Rust compiler" => &["rust"],
        "Cargo" => &["cargo"],
        "CMake" => &["cmake"],
        "C++ compiler" => &["gcc-c++", "make"],
        ".NET SDK" => &["dotnet-sdk-10.0"],
        "clangd" => &["clang-tools-extra"],
        "clang-format" => &["clang-tools-extra"],
        "Roslyn language server" => &[],
        "Git" => &["git"],
        "Debugger" => &["gdb", "lldb"],
        _ => &[],
    }
}

fn detect_tools() -> Vec<ToolStatus> {
    vec![
        probe("Rust compiler", "rustc", &["--version"]),
        probe("Cargo", "cargo", &["--version"]),
        probe("CMake", "cmake", &["--version"]),
        probe_first(
            "C++ compiler",
            &[("g++", &["--version"]), ("clang++", &["--version"])],
        ),
        probe(".NET SDK", "dotnet", &["--version"]),
        probe_netcoredbg(),
        probe("clangd", "clangd", &["--version"]),
        probe("clang-format", "clang-format", &["--version"]),
        probe_roslyn(),
        probe("Git", "git", &["--version"]),
        probe_first(
            "Debugger",
            &[("gdb", &["--version"]), ("lldb", &["--version"])],
        ),
    ]
}

fn probe_netcoredbg() -> ToolStatus {
    let version = ToolchainManager::netcoredbg_executable().and_then(|executable| {
        Command::new(executable)
            .arg("--help")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| {
                let text = String::from_utf8_lossy(&output.stdout);
                let first_line = text.lines().find(|line| !line.trim().is_empty());
                first_line
                    .unwrap_or("NetCoreDbg available")
                    .trim()
                    .to_owned()
            })
    });
    ToolStatus {
        name: "C# debugger",
        command: "netcoredbg",
        version,
    }
}

fn managed_tools_dir() -> Option<std::path::PathBuf> {
    if let Some(xdg_data) = std::env::var_os("XDG_DATA_HOME") {
        return Some(std::path::PathBuf::from(xdg_data).join("cairn-ide/tools"));
    }
    std::env::var_os("HOME")
        .map(|home| std::path::PathBuf::from(home).join(".local/share/cairn-ide/tools"))
}

fn find_executable(root: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = entry.file_type().ok()?;
        if kind.is_file() && entry.file_name() == name {
            return Some(path);
        }
        if kind.is_dir()
            && let Some(found) = find_executable(&path, name)
        {
            return Some(found);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn install_netcoredbg_release() -> Result<String, String> {
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => {
            return Err(format!(
                "NetCoreDbg automatic download is not configured for Linux architecture '{other}'."
            ));
        }
    };
    for prerequisite in ["curl", "tar"] {
        if !Command::new(prerequisite)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return Err(format!(
                "NetCoreDbg installation requires '{prerequisite}'. On Fedora choose Install Missing Tools again so Fugh can install the prerequisites."
            ));
        }
    }
    let tools_dir = managed_tools_dir().ok_or_else(|| {
        "Could not locate the user's local data directory for tool installation.".to_owned()
    })?;
    std::fs::create_dir_all(&tools_dir)
        .map_err(|error| format!("Could not create {}: {error}", tools_dir.display()))?;
    let release = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--header",
            "Accept: application/vnd.github+json",
            "https://api.github.com/repos/Samsung/netcoredbg/releases/latest",
        ])
        .output()
        .map_err(|error| {
            format!("Could not start curl to query the official NetCoreDbg release: {error}")
        })?;
    if !release.status.success() {
        return Err(format!(
            "GitHub release lookup failed (curl exit {}). Install curl and check the network connection.",
            release.status.code().unwrap_or(-1)
        ));
    }
    let release_json: serde_json::Value = serde_json::from_slice(&release.stdout)
        .map_err(|error| format!("GitHub returned invalid NetCoreDbg release metadata: {error}"))?;
    let version = release_json
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "GitHub release metadata did not include a release tag.".to_owned())?;
    let expected_asset = format!("netcoredbg-linux-{architecture}.tar.gz");
    let download_url = release_json
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .and_then(|assets| {
            assets.iter().find(|asset| {
                asset.get("name").and_then(serde_json::Value::as_str) == Some(&expected_asset)
            })
        })
        .and_then(|asset| {
            asset
                .get("browser_download_url")
                .and_then(serde_json::Value::as_str)
        })
        .ok_or_else(|| format!("The latest NetCoreDbg release has no {expected_asset} asset."))?;
    if !download_url.starts_with("https://github.com/Samsung/netcoredbg/releases/download/") {
        return Err(
            "GitHub release metadata contained an unexpected download host; refusing the download."
                .to_owned(),
        );
    }

    let unique = format!(
        ".netcoredbg-staging-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let staging = tools_dir.join(unique);
    let payload = staging.join("payload");
    std::fs::create_dir_all(&payload)
        .map_err(|error| format!("Could not create a temporary installation directory: {error}"))?;
    let archive = staging.join("netcoredbg.tar.gz");
    let download = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--output",
        ])
        .arg(&archive)
        .arg(download_url)
        .output()
        .map_err(|error| format!("Could not start NetCoreDbg download: {error}"))?;
    if !download.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "NetCoreDbg download failed (curl exit {}).",
            download.status.code().unwrap_or(-1)
        ));
    }
    let listing = Command::new("tar")
        .args(["-tzf"])
        .arg(&archive)
        .output()
        .map_err(|error| {
            format!("Could not start tar to inspect the NetCoreDbg archive: {error}")
        })?;
    if !listing.status.success() || !archive_members_are_safe(&listing.stdout) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err("The NetCoreDbg archive failed validation; it was not extracted.".to_owned());
    }
    let extracted = Command::new("tar")
        .args(["-xzf"])
        .arg(&archive)
        .arg("-C")
        .arg(&payload)
        .args(["--no-same-owner", "--no-same-permissions"])
        .output()
        .map_err(|error| format!("Could not start tar to extract NetCoreDbg: {error}"))?;
    if !extracted.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "NetCoreDbg archive extraction failed: {}",
            String::from_utf8_lossy(&extracted.stderr).trim()
        ));
    }
    let executable = find_executable(&payload, "netcoredbg").ok_or_else(|| {
        "The downloaded NetCoreDbg archive did not contain its debugger executable.".to_owned()
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&executable)
            .map_err(|error| format!("Could not inspect NetCoreDbg executable: {error}"))?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions)
            .map_err(|error| format!("Could not make NetCoreDbg executable: {error}"))?;
    }
    let mut validation = Command::new(&executable);
    validation.arg("--help");
    set_library_path_for_binary(&mut validation, &executable);
    let validation = validation.output().map_err(|error| {
        format!("Downloaded NetCoreDbg could not be started for validation: {error}")
    })?;
    if !validation.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "Downloaded NetCoreDbg failed its startup check: {}",
            String::from_utf8_lossy(&validation.stderr).trim()
        ));
    }

    let current = tools_dir.join("netcoredbg/current");
    let current_parent = current
        .parent()
        .expect("fixed NetCoreDbg tool path has a parent");
    std::fs::create_dir_all(current_parent)
        .map_err(|error| format!("Could not prepare NetCoreDbg install directory: {error}"))?;
    let payload_install = current_parent.join(format!(".install-{}", std::process::id()));
    if payload_install.exists() {
        std::fs::remove_dir_all(&payload_install).map_err(|error| {
            format!("Could not clear previous NetCoreDbg staging folder: {error}")
        })?;
    }
    std::fs::rename(&payload, &payload_install)
        .map_err(|error| format!("Could not stage NetCoreDbg for installation: {error}"))?;
    let backup = current_parent.join("previous");
    if backup.exists() {
        std::fs::remove_dir_all(&backup)
            .map_err(|error| format!("Could not remove old NetCoreDbg backup: {error}"))?;
    }
    if current.exists() {
        std::fs::rename(&current, &backup).map_err(|error| {
            format!("Could not preserve the previous NetCoreDbg installation: {error}")
        })?;
    }
    if let Err(error) = std::fs::rename(&payload_install, &current) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, &current);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "Could not activate NetCoreDbg installation: {error}"
        ));
    }
    let _ = std::fs::remove_dir_all(&backup);
    let _ = std::fs::remove_dir_all(&staging);
    Ok(version.to_owned())
}

#[cfg(target_os = "linux")]
fn archive_members_are_safe(listing: &[u8]) -> bool {
    String::from_utf8_lossy(listing).lines().all(|entry| {
        let path = std::path::Path::new(entry.trim());
        !path.as_os_str().is_empty()
            && path.components().all(|component| {
                matches!(
                    component,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            })
    })
}

#[cfg(unix)]
pub(crate) fn set_library_path_for_binary(command: &mut Command, binary: &std::path::Path) {
    let Some(parent) = binary.parent().filter(|path| !path.as_os_str().is_empty()) else {
        return;
    };
    let mut paths = vec![parent.to_path_buf()];
    if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    if let Ok(joined) = std::env::join_paths(paths) {
        command.env("LD_LIBRARY_PATH", joined);
    }
}

fn probe_roslyn() -> ToolStatus {
    let version = Command::new("dotnet")
        .args(["tool", "list", "--global"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .find(|line| {
                    line.split_whitespace()
                        .next()
                        .is_some_and(|name| name.eq_ignore_ascii_case("roslyn-language-server"))
                })
                .map(|line| {
                    line.split_whitespace().nth(1).map_or_else(
                        || "Installed global .NET tool".to_owned(),
                        |version| format!("Roslyn {version}"),
                    )
                })
        });
    ToolStatus {
        name: "Roslyn language server",
        command: "roslyn-language-server",
        version,
    }
}

fn probe(name: &'static str, command: &'static str, args: &[&str]) -> ToolStatus {
    let version = Command::new(command)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            stdout
                .lines()
                .chain(stderr.lines())
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Installed (version not reported)")
                .trim()
                .to_owned()
        });
    ToolStatus {
        name,
        command,
        version,
    }
}

fn probe_first(name: &'static str, choices: &[(&'static str, &[&str])]) -> ToolStatus {
    for (command, args) in choices {
        let found = probe(name, command, args);
        if found.version.is_some() {
            return found;
        }
    }
    ToolStatus {
        name,
        command: choices
            .first()
            .map_or("unknown", |(command, _args)| command),
        version: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{ToolStatus, ToolchainManager, is_fedora_release, packages_for_tool, probe_first};

    #[test]
    fn enables_package_management_only_for_fedora_os_release() {
        assert!(is_fedora_release("NAME=Fedora Linux\nID=fedora\n"));
        assert!(!is_fedora_release(
            "NAME=Ubuntu\nID=ubuntu\nID_LIKE=fedora\n"
        ));
        assert!(!is_fedora_release("NAME=Fedora derivative\nID=example\n"));
    }

    #[test]
    fn tool_status_records_missing_tools_without_faking_a_version() {
        let status = probe_first("missing", &[("cairn-no-such-tool", &["--version"])]);
        assert_eq!(status.name, "missing");
        assert!(status.version.is_none());
    }

    #[test]
    fn detected_versions_are_single_display_lines() {
        let status = ToolStatus {
            name: "test",
            command: "test-command",
            version: Some("tool version 1.0".to_owned()),
        };
        assert!(!status.version.unwrap().contains('\n'));
    }

    #[test]
    fn supported_toolchains_map_only_to_known_fedora_packages() {
        assert_eq!(packages_for_tool("C++ compiler"), ["gcc-c++", "make"]);
        assert_eq!(packages_for_tool(".NET SDK"), ["dotnet-sdk-10.0"]);
        assert!(packages_for_tool("unknown tool").is_empty());
        assert!(ToolchainManager::tool_has_supported_installer(
            "Roslyn language server"
        ));
        assert!(!ToolchainManager::tool_has_install_package(
            "Roslyn language server"
        ));
    }
}
