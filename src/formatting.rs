//! Formatter adapters. Language intelligence belongs to language servers;
//! formatting is delegated to clang-format and dotnet format.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::settings::FormattingSettings;

pub fn format_document(
    path: &Path,
    project_root: &Path,
    text: &str,
    settings: &FormattingSettings,
) -> Result<String, String> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("cpp" | "cc" | "cxx" | "h" | "hpp" | "hh" | "hxx") => {
            format_cpp(path, project_root, text, settings)
        }
        Some("cs") => format_csharp(path, project_root, text, settings),
        Some(extension) => Err(format!(
            "No formatter is configured for .{extension} files."
        )),
        None => Err("The active file has no extension, so its formatter is unknown.".to_owned()),
    }
}

fn format_cpp(
    path: &Path,
    project_root: &Path,
    text: &str,
    settings: &FormattingSettings,
) -> Result<String, String> {
    let project_config = ancestor_config(path, project_root, &[".clang-format", "_clang-format"]);
    let style = if project_config.is_some() && settings.respect_project_config {
        "file".to_owned()
    } else {
        let tabs = if settings.use_tabs {
            "ForIndentation"
        } else {
            "Never"
        };
        format!(
            "{{BasedOnStyle: {}, IndentWidth: {}, UseTab: {}, ColumnLimit: {}, BreakBeforeBraces: {}}}",
            settings.cpp_style.label(),
            settings.indent_width,
            tabs,
            settings.line_length,
            settings.cpp_braces.clang_value()
        )
    };
    let mut formatter = Command::new("clang-format");
    formatter.envs(crate::toolchain::toolchain_environment());
    let mut child = formatter
        .current_dir(project_root)
        .arg(format!("--style={style}"))
        .arg(format!("--assume-filename={}", path.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!("Could not start clang-format. Install it or configure PATH. ({error})")
        })?;
    child
        .stdin
        .take()
        .ok_or_else(|| "Could not open clang-format input.".to_owned())?
        .write_all(text.as_bytes())
        .map_err(|error| format!("Could not send source to clang-format: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Could not read clang-format output: {error}"))?;
    if !output.status.success() {
        return Err(format_process_error(
            "clang-format",
            &output.stderr,
            output.status.code(),
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("clang-format returned invalid UTF-8: {error}"))
}

fn format_csharp(
    path: &Path,
    project_root: &Path,
    text: &str,
    settings: &FormattingSettings,
) -> Result<String, String> {
    let project = [".sln", ".slnx", ".csproj"]
        .iter()
        .flat_map(|extension| find_project_file(project_root, extension))
        .next()
        .ok_or_else(|| "Could not find a .NET project or solution to format.".to_owned())?;
    let relative = path
        .strip_prefix(project_root)
        .map_err(|_| "The C# file is outside the current project.".to_owned())?;
    let project_relative = project
        .strip_prefix(project_root)
        .map_err(|_| "The .NET project is outside the workspace.".to_owned())?;
    static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temporary_root = std::env::temp_dir().join(format!(
        "fugh-dotnet-format-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&temporary_root)
        .map_err(|error| format!("Could not create temporary formatting workspace: {error}"))?;
    let result = (|| {
        copy_format_workspace(project_root, &temporary_root)?;
        let temporary_file = temporary_root.join(relative);
        fs::write(&temporary_file, text)
            .map_err(|error| format!("Could not stage the C# buffer for formatting: {error}"))?;
        if !settings.respect_project_config
            || ancestor_config(path, project_root, &[".editorconfig"]).is_none()
        {
            let config = temporary_file
                .parent()
                .ok_or_else(|| "The C# file has no parent directory.".to_owned())?
                .join(".editorconfig");
            fs::write(config, editorconfig_for(settings)).map_err(|error| {
                format!("Could not create temporary formatter settings: {error}")
            })?;
        }
        let temporary_project = temporary_root.join(project_relative);
        let mut dotnet = Command::new("dotnet");
        dotnet.envs(crate::toolchain::toolchain_environment());
        let output = dotnet
            .current_dir(&temporary_root)
            .args(["format", "whitespace"])
            .arg(temporary_project)
            .arg("--include")
            .arg(relative)
            .arg("--no-restore")
            .output()
            .map_err(|error| {
                format!("Could not start dotnet format. Install the .NET SDK. ({error})")
            })?;
        if !output.status.success() {
            return Err(format_process_error(
                "dotnet format",
                &output.stderr,
                output.status.code(),
            ));
        }
        fs::read_to_string(temporary_file)
            .map_err(|error| format!("Could not read formatted C# source: {error}"))
    })();
    let _ = fs::remove_dir_all(&temporary_root);
    result
}

fn copy_format_workspace(source: &Path, destination: &Path) -> Result<(), String> {
    let entries = fs::read_dir(source)
        .map_err(|error| format!("Could not read project for formatting: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not read project entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not inspect {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        if matches!(
            name.to_str(),
            Some(".git" | ".vs" | "bin" | "target" | "node_modules")
        ) {
            continue;
        }
        let target = destination.join(&name);
        if file_type.is_dir() {
            fs::create_dir(&target)
                .map_err(|error| format!("Could not copy project directory: {error}"))?;
            copy_format_workspace(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("Could not copy project file: {error}"))?;
        }
    }
    Ok(())
}

fn editorconfig_for(settings: &FormattingSettings) -> String {
    let indent_style = if settings.use_tabs { "tab" } else { "space" };
    let brace = match settings.csharp_braces {
        crate::settings::CSharpBraceStyle::SameLine => "none",
        crate::settings::CSharpBraceStyle::Allman => "all",
    };
    format!(
        "root = true\n\n[*.cs]\nindent_style = {indent_style}\nindent_size = {}\ntab_width = {}\nend_of_line = lf\ninsert_final_newline = true\ncsharp_new_line_before_open_brace = {brace}\nmax_line_length = {}\n",
        settings.indent_width, settings.indent_width, settings.line_length
    )
}

fn find_project_file(root: &Path, extension: &str) -> Option<PathBuf> {
    fn visit(dir: &Path, extension: &str, depth: usize) -> Option<PathBuf> {
        if depth > 3 {
            return None;
        }
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| {
                    matches!(name.to_str(), Some(".git" | "bin" | "obj" | "target"))
                }) {
                    continue;
                }
                if let Some(found) = visit(&path, extension, depth + 1) {
                    return Some(found);
                }
            } else if path.extension().and_then(|value| value.to_str()) == Some(&extension[1..]) {
                return Some(path);
            }
        }
        None
    }
    visit(root, extension, 0)
}

fn ancestor_config(path: &Path, root: &Path, names: &[&str]) -> Option<PathBuf> {
    let mut directory = path.parent()?;
    loop {
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if directory == root {
            return None;
        }
        directory = directory.parent()?;
        if !directory.starts_with(root) {
            return None;
        }
    }
}

fn format_process_error(tool: &str, stderr: &[u8], code: Option<i32>) -> String {
    let detail = String::from_utf8_lossy(stderr).trim().to_owned();
    if detail.is_empty() {
        format!(
            "{tool} failed with exit code {}.",
            code.map_or_else(|| "unknown".to_owned(), |code| code.to_string())
        )
    } else {
        format!("{tool} failed: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::{ancestor_config, editorconfig_for};
    use crate::settings::{CSharpBraceStyle, FormattingSettings};

    #[test]
    fn csharp_style_generates_real_editorconfig_rules() {
        let settings = FormattingSettings {
            csharp_braces: CSharpBraceStyle::Allman,
            use_tabs: true,
            indent_width: 2,
            line_length: 88,
            ..FormattingSettings::default()
        };
        let config = editorconfig_for(&settings);
        assert!(config.contains("indent_style = tab"));
        assert!(config.contains("indent_size = 2"));
        assert!(config.contains("csharp_new_line_before_open_brace = all"));
        assert!(config.contains("max_line_length = 88"));
    }

    #[test]
    fn project_formatter_config_is_found_only_inside_project_root() {
        let root = std::env::temp_dir().join("cairn-format-config-test");
        let nested = root.join("src");
        std::fs::create_dir_all(&nested).unwrap();
        let config = root.join(".clang-format");
        std::fs::write(&config, "BasedOnStyle: LLVM\n").unwrap();
        assert_eq!(
            ancestor_config(&nested.join("main.cpp"), &root, &[".clang-format"]),
            Some(config.clone())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
