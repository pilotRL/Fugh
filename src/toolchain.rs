use std::{
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
enum LinuxPackageManager {
    Dnf,
    Apt,
    Pacman,
    Zypper,
    Apk,
}

impl LinuxPackageManager {
    const fn label(self) -> &'static str {
        match self {
            Self::Dnf => "DNF (Fedora/RHEL family)",
            Self::Apt => "APT (Debian/Ubuntu family)",
            Self::Pacman => "pacman (Arch family)",
            Self::Zypper => "Zypper (openSUSE/SUSE family)",
            Self::Apk => "apk (Alpine Linux)",
        }
    }
}

#[cfg(target_os = "linux")]
static LINUX_PACKAGE_MANAGER: std::sync::OnceLock<Option<LinuxPackageManager>> =
    std::sync::OnceLock::new();
static COMMAND_AVAILABILITY: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, bool>>,
> = std::sync::OnceLock::new();

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
            Self::UpdateInstalled => "Update system packages",
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
    fn linux_package_manager() -> Option<LinuxPackageManager> {
        #[cfg(target_os = "linux")]
        {
            *LINUX_PACKAGE_MANAGER.get_or_init(|| {
                std::fs::read_to_string("/etc/os-release")
                    .ok()
                    .and_then(|release| package_manager_for_os_release(&release))
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    pub fn supports_linux_package_management() -> bool {
        Self::linux_package_manager().is_some()
    }

    pub fn supports_fedora_release() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/etc/os-release")
                .is_ok_and(|release| is_fedora_release(&release))
        }

        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    pub fn package_manager_label() -> String {
        Self::linux_package_manager().map_or_else(
            || "unsupported Linux distribution".to_owned(),
            |manager| manager.label().to_owned(),
        )
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
            .flat_map(|tool| {
                Self::linux_package_manager()
                    .map(|manager| packages_for_tool(manager, tool.name))
                    .unwrap_or_default()
            })
            .map(|package| (*package).to_owned())
            .collect::<Vec<_>>();
        if let Some(manager) = Self::linux_package_manager() {
            let selected = |name: &str| match action {
                PackageAction::InstallMissing => true,
                PackageAction::InstallSelected(names) => names.iter().any(|item| item == name),
                PackageAction::UpdateInstalled => false,
            };
            let missing = |name: &str| {
                tools
                    .iter()
                    .any(|tool| tool.name == name && tool.version.is_none())
            };
            let needs_dotnet = (selected(".NET SDK") && missing(".NET SDK"))
                || (selected("Roslyn language server")
                    && missing("Roslyn language server")
                    && missing(".NET SDK"));
            let needs_rust = (selected("Rust compiler") || selected("Cargo"))
                && (missing("Rust compiler") || missing("Cargo"));
            let needs_netcoredbg = selected("C# debugger") && missing("C# debugger");
            if (needs_dotnet || needs_rust)
                && !command_available("curl")
                && !command_available("wget")
            {
                packages.extend(
                    packages_for_tool(manager, "Download tools")
                        .iter()
                        .map(|package| (*package).to_owned()),
                );
            }
            if needs_netcoredbg && !command_available("curl") {
                packages.extend(
                    packages_for_tool(manager, "Download tools")
                        .iter()
                        .map(|package| (*package).to_owned()),
                );
            }
            if (needs_netcoredbg || needs_dotnet) && !command_available("tar") {
                packages.extend(
                    packages_for_tool(manager, "Archive tools")
                        .iter()
                        .map(|package| (*package).to_owned()),
                );
            }
            if (needs_dotnet || needs_rust) && !command_available("bash") {
                packages.extend(
                    packages_for_tool(manager, "Shell")
                        .iter()
                        .map(|package| (*package).to_owned()),
                );
            }
        }
        packages.sort_unstable();
        packages.dedup();
        packages
    }

    pub fn tool_has_install_package(name: &str) -> bool {
        Self::linux_package_manager()
            .is_some_and(|manager| !packages_for_tool(manager, name).is_empty())
    }

    pub fn tool_has_supported_installer(name: &str) -> bool {
        Self::tool_has_install_package(name)
            || (cfg!(target_os = "linux")
                && matches!(
                    name,
                    "Rust compiler"
                        | "Cargo"
                        | ".NET SDK"
                        | "Roslyn language server"
                        | "C# debugger"
                ))
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
            Err("In-app tool installation is currently supported on Linux only.".to_owned())
        }

        #[cfg(target_os = "linux")]
        {
            let package_manager = Self::linux_package_manager();
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
            let install_rust_toolchain = self.tools().iter().any(|tool| {
                matches!(tool.name, "Rust compiler" | "Cargo")
                    && tool.version.is_none()
                    && match &action {
                        PackageAction::InstallMissing => true,
                        PackageAction::InstallSelected(names) => names
                            .iter()
                            .any(|name| matches!(name.as_str(), "Rust compiler" | "Cargo")),
                        PackageAction::UpdateInstalled => false,
                    }
            });
            let install_dotnet_sdk = self.tools().iter().any(|tool| {
                tool.name == ".NET SDK"
                    && tool.version.is_none()
                    && match &action {
                        PackageAction::InstallMissing => true,
                        PackageAction::InstallSelected(names) => {
                            names.iter().any(|name| name == ".NET SDK")
                        }
                        PackageAction::UpdateInstalled => false,
                    }
            }) || (install_roslyn
                && self
                    .tools()
                    .iter()
                    .any(|tool| tool.name == ".NET SDK" && tool.version.is_none()));
            let unsupported_tools = if package_manager.is_none() {
                self.tools()
                    .into_iter()
                    .filter(|tool| {
                        let requested = match &action {
                            PackageAction::InstallMissing => true,
                            PackageAction::InstallSelected(names) => {
                                names.iter().any(|name| name == tool.name)
                            }
                            PackageAction::UpdateInstalled => false,
                        };
                        requested
                            && tool.version.is_none()
                            && !matches!(
                                tool.name,
                                "Rust compiler"
                                    | "Cargo"
                                    | ".NET SDK"
                                    | "Roslyn language server"
                                    | "C# debugger"
                            )
                    })
                    .map(|tool| tool.name)
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let packages =
                package_manager.map_or_else(Vec::new, |_| self.planned_packages(&action));
            if package_manager.is_none()
                && !install_roslyn
                && !install_netcoredbg
                && !install_dotnet_sdk
                && !install_rust_toolchain
            {
                return Err("This Linux distribution is not recognized for automatic package installation. Install the required tools with its package manager; Fugh can still install .NET, Roslyn, and NetCoreDbg when their download prerequisites are present.".to_owned());
            }
            if packages.is_empty()
                && !install_roslyn
                && !install_netcoredbg
                && !install_dotnet_sdk
                && !install_rust_toolchain
            {
                let unsupported_missing = self
                    .tools()
                    .into_iter()
                    .filter(|tool| {
                        tool.version.is_none()
                            && package_manager.is_none_or(|manager| {
                                packages_for_tool(manager, tool.name).is_empty()
                            })
                            && tool.name != "Roslyn language server"
                            && tool.name != ".NET SDK"
                    })
                    .map(|tool| tool.name)
                    .collect::<Vec<_>>();
                return Err(match &action {
                    PackageAction::InstallMissing => {
                        if unsupported_missing.is_empty() {
                            "No missing tools are available through this package manager."
                                .to_owned()
                        } else {
                            format!(
                                "{} is missing but has no automatic installer for this distribution. Install it with the distribution's package manager or from its official project site.",
                                unsupported_missing.join(", ")
                            )
                        }
                    }
                    PackageAction::InstallSelected(_) => {
                        "No selected missing tools have a supported package.".to_owned()
                    }
                    PackageAction::UpdateInstalled => {
                        "No installed system packages to update.".to_owned()
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
                    if !packages.is_empty()
                        && let Some(manager) = package_manager
                    {
                        failure = run_package_commands(manager, &action, &packages, &mut lines);
                        if failure.is_none() {
                            reset_command_availability_cache();
                        }
                    }
                    if failure.is_none() && install_dotnet_sdk {
                        lines.push("Installing the latest .NET LTS SDK to ~/.dotnet using Microsoft's official installer…".to_owned());
                        match install_dotnet_sdk_user_local() {
                            Ok(output) => lines.extend(output),
                            Err(error) => failure = Some(error),
                        }
                    }
                    if failure.is_none() && install_rust_toolchain {
                        lines.push("Installing the stable Rust toolchain with rustup in ~/.cargo and ~/.rustup…".to_owned());
                        match install_rust_toolchain_user_local() {
                            Ok(output) => lines.extend(output),
                            Err(error) => failure = Some(error),
                        }
                    }
                    if failure.is_none() && install_roslyn {
                        if let Some(version) = probe_roslyn().version {
                            lines.push(format!("Roslyn is already installed ({version}); keeping the existing global tool."));
                        } else {
                            lines.push(
                                "Installing Roslyn language server as a global .NET tool…"
                                    .to_owned(),
                            );
                            let mut command = Command::new(dotnet_executable());
                            command.envs(dotnet_environment());
                            match command
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
                    }
                    if failure.is_none() && install_netcoredbg {
                        lines.push("Downloading NetCoreDbg from the official Samsung/netcoredbg GitHub release…".to_owned());
                        match install_netcoredbg_release() {
                            Ok(version) => lines.push(format!("Installed NetCoreDbg {version} in Fugh's user-local tools directory.")),
                            Err(error) => failure = Some(error),
                        }
                    }
                    if failure.is_none() && !unsupported_tools.is_empty() {
                        failure = Some(format!(
                            "Installed the user-local tools, but these system tools need manual installation on this distribution: {}.",
                            unsupported_tools.join(", ")
                        ));
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
                    let message = format!("Could not start tool installation worker: {error}");
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
fn package_manager_for_os_release(os_release: &str) -> Option<LinuxPackageManager> {
    let mut id = "";
    let mut id_like = "";
    for line in os_release.lines() {
        if let Some(value) = line.strip_prefix("ID=") {
            id = value.trim_matches(['"', '\'']).trim();
        } else if let Some(value) = line.strip_prefix("ID_LIKE=") {
            id_like = value.trim_matches(['"', '\'']).trim();
        }
    }
    let ids = std::iter::once(id)
        .chain(id_like.split_whitespace())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();

    if ids.iter().any(|id| {
        matches!(
            id.as_str(),
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "ol" | "nobara"
        )
    }) {
        Some(LinuxPackageManager::Dnf)
    } else if ids.iter().any(|id| {
        matches!(
            id.as_str(),
            "debian" | "ubuntu" | "linuxmint" | "pop" | "elementary" | "zorin" | "kali"
        )
    }) {
        Some(LinuxPackageManager::Apt)
    } else if ids.iter().any(|id| {
        matches!(
            id.as_str(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" | "artix"
        )
    }) {
        Some(LinuxPackageManager::Pacman)
    } else if ids
        .iter()
        .any(|id| id.starts_with("opensuse") || matches!(id.as_str(), "suse" | "sles"))
    {
        Some(LinuxPackageManager::Zypper)
    } else if ids.iter().any(|id| id == "alpine") {
        Some(LinuxPackageManager::Apk)
    } else {
        None
    }
}

#[cfg(any(target_os = "linux", test))]
fn is_fedora_release(os_release: &str) -> bool {
    os_release.lines().any(|line| {
        line.strip_prefix("ID=")
            .is_some_and(|id| id.trim_matches(['"', '\'']).eq_ignore_ascii_case("fedora"))
    })
}

fn packages_for_tool(manager: LinuxPackageManager, name: &str) -> &'static [&'static str] {
    match (manager, name) {
        (_, "Rust compiler" | "Cargo") => &[],
        (_, "CMake") => &["cmake"],
        (LinuxPackageManager::Dnf | LinuxPackageManager::Zypper, "C++ compiler") => {
            &["gcc-c++", "make"]
        }
        (LinuxPackageManager::Apt, "C++ compiler") => &["g++", "make"],
        (LinuxPackageManager::Pacman, "C++ compiler") => &["gcc", "make"],
        (LinuxPackageManager::Apk, "C++ compiler") => &["g++", "make"],
        (LinuxPackageManager::Dnf, "clangd" | "clang-format") => &["clang-tools-extra"],
        (LinuxPackageManager::Apt, "clangd") => &["clangd"],
        (LinuxPackageManager::Apt, "clang-format") => &["clang-format"],
        (LinuxPackageManager::Pacman, "clangd" | "clang-format") => &["clang"],
        (LinuxPackageManager::Zypper, "clangd" | "clang-format") => &["clang-tools"],
        (LinuxPackageManager::Apk, "clangd" | "clang-format") => &["clang-extra-tools"],
        (_, "Git") => &["git"],
        (_, "Debugger") => &["gdb", "lldb"],
        (LinuxPackageManager::Dnf, "Download tools") => &["curl"],
        (LinuxPackageManager::Apt, "Download tools") => &["curl"],
        (LinuxPackageManager::Pacman, "Download tools") => &["curl"],
        (LinuxPackageManager::Zypper, "Download tools") => &["curl"],
        (LinuxPackageManager::Apk, "Download tools") => &["curl"],
        (_, "Archive tools") => &["tar"],
        (_, "Shell") => &["bash"],
        _ => &[],
    }
}

fn command_available(command: &str) -> bool {
    let cache = COMMAND_AVAILABILITY.get_or_init(Default::default);
    if let Some(available) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(command)
        .copied()
    {
        return available;
    }
    let available = Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(command.to_owned(), available);
    available
}

#[cfg(target_os = "linux")]
fn reset_command_availability_cache() {
    if let Some(cache) = COMMAND_AVAILABILITY.get() {
        cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

#[cfg(target_os = "linux")]
fn run_package_commands(
    manager: LinuxPackageManager,
    action: &PackageAction,
    packages: &[String],
    output_lines: &mut Vec<String>,
) -> Option<String> {
    let install = !matches!(action, PackageAction::UpdateInstalled);
    let mut commands: Vec<Vec<String>> = Vec::new();
    match manager {
        LinuxPackageManager::Dnf => {
            let mut args = if install {
                vec![
                    "dnf".to_owned(),
                    "install".to_owned(),
                    "--assumeyes".to_owned(),
                ]
            } else {
                vec![
                    "dnf".to_owned(),
                    "upgrade".to_owned(),
                    "--refresh".to_owned(),
                    "--assumeyes".to_owned(),
                ]
            };
            args.extend(packages.iter().cloned());
            commands.push(args);
        }
        LinuxPackageManager::Apt => {
            commands.push(vec!["apt-get".to_owned(), "update".to_owned()]);
            let mut args = vec!["apt-get".to_owned(), "install".to_owned()];
            if !install {
                args.push("--only-upgrade".to_owned());
            }
            args.push("--yes".to_owned());
            args.extend(packages.iter().cloned());
            commands.push(args);
        }
        LinuxPackageManager::Pacman => {
            let mut args = if install {
                vec![
                    "pacman".to_owned(),
                    "-S".to_owned(),
                    "--needed".to_owned(),
                    "--noconfirm".to_owned(),
                ]
            } else {
                vec![
                    "pacman".to_owned(),
                    "-Syu".to_owned(),
                    "--noconfirm".to_owned(),
                ]
            };
            args.extend(packages.iter().cloned());
            commands.push(args);
        }
        LinuxPackageManager::Zypper => {
            let mut args = vec!["zypper".to_owned(), "--non-interactive".to_owned()];
            args.push(if install { "install" } else { "update" }.to_owned());
            if install {
                args.push("--no-recommends".to_owned());
            }
            args.extend(packages.iter().cloned());
            commands.push(args);
        }
        LinuxPackageManager::Apk => {
            if !install {
                commands.push(vec!["apk".to_owned(), "update".to_owned()]);
                let mut args = vec!["apk".to_owned(), "upgrade".to_owned()];
                args.extend(packages.iter().cloned());
                commands.push(args);
            } else {
                let mut args = vec!["apk".to_owned(), "add".to_owned()];
                args.extend(packages.iter().cloned());
                commands.push(args);
            }
        }
    }

    for args in commands {
        let display = format!("pkexec {}", args.join(" "));
        output_lines.push(format!("Running {display}"));
        let output = match Command::new("pkexec").args(&args).output() {
            Ok(output) => output,
            Err(error) => {
                return Some(format!(
                    "Could not start pkexec. Install PolicyKit or run this command manually: {display} ({error})"
                ));
            }
        };
        output_lines.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::to_owned),
        );
        output_lines.extend(
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .map(str::to_owned),
        );
        if !output.status.success() {
            let code = output.status.code().map_or_else(
                || "terminated by signal".to_owned(),
                |code| code.to_string(),
            );
            return Some(format!(
                "{} package operation failed (exit {code}).",
                manager.label()
            ));
        }
    }
    None
}

fn dotnet_root() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| std::path::PathBuf::from(home).join(".dotnet"))
}

fn dotnet_executable() -> std::path::PathBuf {
    if let Some(local) = dotnet_root().map(|root| root.join("dotnet"))
        && local.is_file()
    {
        return local;
    }
    "dotnet".into()
}

fn dotnet_environment() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    let Some(root) = dotnet_root() else {
        return Vec::new();
    };
    let mut paths = vec![root.join("tools"), root.clone()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let mut variables = Vec::new();
    if root.join("dotnet").is_file() {
        variables.push(("DOTNET_ROOT".into(), root.clone().into_os_string()));
    }
    if let Ok(path) = std::env::join_paths(paths) {
        variables.push(("PATH".into(), path));
    }
    variables
}

pub(crate) fn toolchain_environment() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home = std::path::PathBuf::from(home);
        paths.push(home.join(".cargo/bin"));
        let dotnet_root = home.join(".dotnet");
        paths.push(dotnet_root.join("tools"));
        paths.push(dotnet_root);
    }
    #[cfg(target_os = "linux")]
    if let Ok(entries) = std::fs::read_dir("/usr/lib") {
        paths.extend(entries.flatten().filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let bin = entry.path().join("bin");
            (name.starts_with("llvm") && bin.is_dir()).then_some(bin)
        }));
    }
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let mut variables = Vec::new();
    if let Ok(path) = std::env::join_paths(paths) {
        variables.push(("PATH".into(), path));
    }
    if let Some(root) = dotnet_root().filter(|root| root.join("dotnet").is_file()) {
        variables.push(("DOTNET_ROOT".into(), root.into_os_string()));
    }
    variables
}

#[cfg(target_os = "linux")]
fn install_rust_toolchain_user_local() -> Result<Vec<String>, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            "Could not locate the user's home directory for installing Rust.".to_owned()
        })?;
    if !command_available("bash") {
        return Err("Installing Rust requires Bash. Install bash with your distribution's package manager, then retry.".to_owned());
    }
    if !command_available("curl") && !command_available("wget") {
        return Err("Installing Rust requires curl or wget. Install one with your distribution's package manager, then retry.".to_owned());
    }
    let script =
        std::env::temp_dir().join(format!("fugh-rustup-install-{}.sh", std::process::id()));
    download_https("https://sh.rustup.rs", &script)?;
    let result = Command::new("bash")
        .arg(&script)
        .args(["-y", "--profile", "minimal"])
        .envs(toolchain_environment())
        .output();
    let _ = std::fs::remove_file(&script);
    let output = result
        .map_err(|error| format!("Could not start the official rustup installer: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let lines = stdout
        .lines()
        .map(str::to_owned)
        .chain(stderr.lines().map(str::to_owned))
        .collect::<Vec<_>>();
    if !output.status.success() {
        return Err(format!(
            "The rustup installer failed (exit {}).",
            output.status.code().unwrap_or(-1)
        ));
    }
    if !home.join(".cargo/bin/rustc").is_file() || !home.join(".cargo/bin/cargo").is_file() {
        return Err(
            "The rustup installer finished but rustc or cargo was not found under ~/.cargo/bin."
                .to_owned(),
        );
    }
    Ok(lines)
}

#[cfg(target_os = "linux")]
fn install_dotnet_sdk_user_local() -> Result<Vec<String>, String> {
    let root = dotnet_root().ok_or_else(|| {
        "Could not locate the user's home directory for installing .NET.".to_owned()
    })?;
    if !command_available("bash") {
        return Err("Installing .NET requires Bash. Install bash with your distribution's package manager, then retry.".to_owned());
    }
    if !command_available("tar") {
        return Err("Installing .NET requires tar. Install tar with your distribution's package manager, then retry.".to_owned());
    }
    if !command_available("curl") && !command_available("wget") {
        return Err("Installing .NET requires curl or wget. Install one with your distribution's package manager, then retry.".to_owned());
    }
    let script =
        std::env::temp_dir().join(format!("fugh-dotnet-install-{}.sh", std::process::id()));
    download_https("https://dot.net/v1/dotnet-install.sh", &script)?;
    let result = Command::new("bash")
        .arg(&script)
        .args(["--channel", "LTS", "--install-dir"])
        .arg(&root)
        .arg("--no-path")
        .envs(dotnet_environment())
        .output();
    let _ = std::fs::remove_file(&script);
    let output =
        result.map_err(|error| format!("Could not start Microsoft's .NET installer: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let lines = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if !output.status.success() {
        return Err(format!(
            "Microsoft's .NET SDK installer failed (exit {}).",
            output.status.code().unwrap_or(-1)
        ));
    }
    let installed_dotnet = root.join("dotnet");
    if !installed_dotnet.is_file() {
        return Err("The .NET installer finished but ~/.dotnet/dotnet was not created.".to_owned());
    }
    Ok(lines)
}

#[cfg(target_os = "linux")]
fn download_https(url: &str, destination: &std::path::Path) -> Result<(), String> {
    let result = if command_available("curl") {
        Command::new("curl")
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
            .arg(destination)
            .arg(url)
            .output()
    } else if command_available("wget") {
        Command::new("wget")
            .args(["--https-only", "--output-document"])
            .arg(destination)
            .arg(url)
            .output()
    } else {
        return Err("Downloading this tool requires curl or wget.".to_owned());
    }
    .map_err(|error| format!("Could not start the HTTPS download: {error}"))?;
    if !result.status.success() {
        let _ = std::fs::remove_file(destination);
        return Err(format!(
            "HTTPS download failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    Ok(())
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
        probe_dotnet(),
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
        if !command_available(prerequisite) {
            return Err(format!(
                "NetCoreDbg installation requires '{prerequisite}'. Choose Install Missing Tools again so Fugh can install the prerequisite, or add it with your distribution's package manager."
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
    let mut command = Command::new(dotnet_executable());
    command.envs(dotnet_environment());
    let version = command
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

fn probe_dotnet() -> ToolStatus {
    let mut command = Command::new(dotnet_executable());
    command.envs(dotnet_environment());
    let version = command
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    ToolStatus {
        name: ".NET SDK",
        command: "dotnet",
        version,
    }
}

fn probe(name: &'static str, command: &'static str, args: &[&str]) -> ToolStatus {
    let mut probe = Command::new(command);
    probe.envs(toolchain_environment());
    let version = probe
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
    use super::{
        LinuxPackageManager, ToolStatus, package_manager_for_os_release, packages_for_tool,
        probe_first,
    };

    #[test]
    fn detects_common_linux_package_manager_families() {
        assert_eq!(
            package_manager_for_os_release("NAME=Fedora Linux\nID=fedora\n"),
            Some(LinuxPackageManager::Dnf)
        );
        assert_eq!(
            package_manager_for_os_release("NAME=Ubuntu\nID=ubuntu\nID_LIKE=debian\n"),
            Some(LinuxPackageManager::Apt)
        );
        assert_eq!(
            package_manager_for_os_release("NAME=Debian GNU/Linux\nID=debian\n"),
            Some(LinuxPackageManager::Apt)
        );
        assert_eq!(
            package_manager_for_os_release("NAME=Arch Linux\nID=arch\n"),
            Some(LinuxPackageManager::Pacman)
        );
        assert_eq!(
            package_manager_for_os_release("NAME=Fedora derivative\nID=example\n"),
            None
        );
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
    fn tool_packages_match_supported_linux_families() {
        assert_eq!(
            packages_for_tool(LinuxPackageManager::Dnf, "C++ compiler"),
            ["gcc-c++", "make"]
        );
        assert_eq!(
            packages_for_tool(LinuxPackageManager::Apt, "C++ compiler"),
            ["g++", "make"]
        );
        assert_eq!(
            packages_for_tool(LinuxPackageManager::Apt, "clangd"),
            ["clangd"]
        );
        assert_eq!(
            packages_for_tool(LinuxPackageManager::Dnf, "clang-format"),
            ["clang-tools-extra"]
        );
        assert!(packages_for_tool(LinuxPackageManager::Apt, "unknown tool").is_empty());
        assert!(packages_for_tool(LinuxPackageManager::Apt, ".NET SDK").is_empty());
    }
}
