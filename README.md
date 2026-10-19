# Fugh IDE

<p align="center">
  <img src="assets/fugh-logo.png" alt="Fugh IDE logo" width="120" />
</p>

<h3 align="center">A modern IDE for C++ and C# on Linux</h3>

<p align="center">
  A project-focused desktop development environment with code navigation, diagnostics, debugging, build tools, and a customizable interface.
</p>

<p align="center">
  <!-- Replace OWNER/REPO with your GitHub username and repository name. -->
  <a href="https://github.com/OWNER/REPO/releases"><img src="https://img.shields.io/github/v/release/OWNER/REPO?display_name=tag&label=release" alt="Latest release"></a>
  <a href="https://github.com/OWNER/REPO/blob/main/LICENSE"><img src="https://img.shields.io/github/license/OWNER/REPO" alt="License"></a>
  <a href="https://github.com/OWNER/REPO/stargazers"><img src="https://img.shields.io/github/stars/OWNER/REPO?style=flat" alt="GitHub stars"></a>
  <a href="https://github.com/OWNER/REPO/issues"><img src="https://img.shields.io/github/issues/OWNER/REPO" alt="Open issues"></a>
  <a href="https://github.com/OWNER/REPO/commits/main"><img src="https://img.shields.io/github/last-commit/OWNER/REPO" alt="Last commit"></a>
  <img src="https://img.shields.io/badge/platform-Linux-informational" alt="Platform">
  <img src="https://img.shields.io/badge/status-early%20development-orange" alt="Project status">
</p>

> **Development status:** Fugh is under active development. Features, supported project types, and setup instructions may change between versions.

---

## Contents

- [Overview](#overview)
- [Features](#features)
- [Screenshots](#screenshots)
- [Supported projects and tools](#supported-projects-and-tools)
- [Requirements](#requirements)
- [Build from source](#build-from-source)
- [Install for your user](#install-for-your-user)
- [Project structure and configuration](#project-structure-and-configuration)
- [Known limitations](#known-limitations)
- [Roadmap](#roadmap)
- [Contributing and bug reports](#contributing-and-bug-reports)
- [License](#license)

## Overview

**Fugh** is a desktop IDE built with **Rust, eframe, and egui**. It is designed around projects rather than isolated files, bringing source editing, build tools, diagnostics, navigation, and debugging into one application.

Fugh currently supports workflows for **C++ and C#** projects using the toolchains installed on the user's system. It includes a project welcome screen, a solution-explorer-style file tree, configurable themes, language-server integration, and integrated build and debugging controls.

Fugh is intended to be Linux-first, with Fedora-specific support for selected system package operations. The project is evolving, and not every feature is available for every language or project format.

## Features

### Project workflow

- Welcome screen with recent projects and actions to create, open, or import projects.
- Project templates for C++ console applications, C++ static libraries, C# console applications, and C# class libraries.
- Import support for CMake project files, .NET project files, and .NET solutions.
- Project properties for CMake-based C++ and .NET C# projects.
- Solution Explorer-style project tree with file icons, context menus, and file/folder creation.
- Persistent project settings stored in the project's `.fugh` directory.

### Editor

- Syntax highlighting for C++, C#, Rust, JSON, and TOML.
- Document tabs, save/discard prompts for modified documents, and `Ctrl+S` saving.
- Code completion for common language keywords and APIs, plus project symbols.
- Paired brackets, braces, and quotes, with indentation carried to new lines.
- Find in Files across saved files and open unsaved buffers.
- Brace-based code folding, a minimap, Alt+click multi-cursor carets, and adjustable editor zoom.
- Unsaved-buffer recovery snapshots after an unclean exit.

### Language intelligence

- C++ language-server integration through **clangd**.
- C# language-server integration through **Roslyn**, when installed and configured.
- Diagnostics from language servers and build output, with editor underlines, gutter markers, overview markers, and hover messages.
- Go to Definition using `F12` or `Ctrl+click`, with project-symbol fallback where available.
- Hover information, document outline, and Quick Actions through `Ctrl+.`.
- C++ and C# formatting through `clang-format` and `dotnet format`.
- Optional format-on-save, with project `.clang-format` and `.editorconfig` files taking precedence where supported.

Language intelligence depends on the relevant tools being installed. The local keyword/API completion is a lightweight fallback and is not a replacement for semantic language-server features.

### Build, run, and debug

- Build, Release, Rebuild, Clean, Analyze, Publish, Create Package, Run, and Debug commands.
- Background CMake and .NET builds with captured output.
- C++ debugging through GDB/MI and C# debugging through NetCoreDbg/MI.
- Debugger controls for continuing, stepping, stopping, stack frames, variables, and persistent breakpoints.
- C++ static-library templates with a public header.
- C# project settings for target framework, runtime identifier, assembly name, namespace, language version, nullable options, analyzers, NuGet packages, and assembly references.
- C++ project settings for language standard, include/library directories, linker options, definitions, and runtime library files.

### Customization

- Built-in themes: Fugh Dark, Graphite, Fugh Light, Dracula, Nord, Solarized Dark, Monokai, and System.
- Custom themes that can be imported and exported as JSON.
- Configurable accent color, UI scale, typography, indentation, line numbers, word wrap, panel visibility, and keyboard shortcuts.
- Resizable and repositionable editor panels, with saved layouts.
- Formatter preferences for C++ and C#.

### Visual UI designer

Fugh includes an early, lightweight Avalonia designer for a subset of C# desktop projects. It supports placing and editing selected controls such as buttons, labels, and text boxes, with a control outline and generated click-handler stubs.

This is **not yet a full live Avalonia preview**. Unsupported XAML can still be edited as source, but may not be editable on the visual canvas.

## Screenshots

Add screenshots of the current application to an `assets/` directory and update the paths below. Screenshots help visitors understand the project quickly.

<!-- Replace these placeholders with real screenshots from the repository. -->
| Welcome screen | Code editor |
|---|---|
| ![Fugh welcome screen](assets/screenshots/welcome.png) | ![Fugh code editor](assets/screenshots/editor.png) |

| Diagnostics and project explorer | Settings and themes |
|---|---|
| ![Fugh diagnostics](assets/screenshots/diagnostics.png) | ![Fugh settings](assets/screenshots/settings.png) |

If you do not have screenshots yet, remove this section until the images are committed. Do not leave broken image links in a published README.

## Supported projects and tools

| Task | Project type | Main tools |
|---|---|---|
| Build and run | CMake-based C++ | CMake and a C++ compiler |
| Build and run | .NET C# | .NET SDK matching the project's target framework |
| C++ diagnostics and navigation | C++ source in a configured workspace | clangd and a valid compilation database |
| C# diagnostics and navigation | .NET C# project | Roslyn language server and compatible .NET runtime |
| C++ debugging | Built C++ executable | GDB |
| C# debugging | .NET application | NetCoreDbg |
| C++ formatting | C++ source | clang-format |
| C# formatting | .NET project | .NET SDK and `dotnet format` |

Imported Visual Studio `.vcxproj` projects can currently be browsed and edited, but the current build runner does not build them. Use a CMake-based project for the supported C++ build workflow.

## Requirements

### Build requirements

- Linux desktop environment.
- Rust toolchain installed through `rustup`.
- Rust **1.95 or newer**, as required by the current project.
- Native development packages required by `eframe`, `egui`, and the windowing system.

### Fedora setup

Install the native build tools and libraries:

```bash
sudo dnf group install "Development Tools"
sudo dnf install \
  pkgconf-pkg-config \
  openssl-devel \
  libxcb-devel \
  libxkbcommon-devel \
  wayland-devel \
  mesa-libGL-devel
```

Install Rust with `rustup` if it is not already installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup toolchain install stable
```

For C++ projects, install CMake and a compiler. For example, on Fedora:

```bash
sudo dnf install cmake gcc-c++ make
```

For C# projects, install a .NET SDK matching the project's target framework, such as .NET 8, .NET 9, or .NET 10 where applicable.

For interactive debugging, install GDB for C++ or NetCoreDbg for C#. Additional tools such as clangd and clang-format are needed for the corresponding language features.

Building an existing Qt Widgets project also requires Qt 6 development files. On Fedora:

```bash
sudo dnf install qt6-qtbase-devel
```

> Package names can vary by Fedora version. Other Linux distributions may require equivalent development packages installed through their package manager.

## Build from source

Clone the repository and enter its directory:

```bash
git clone https://github.com/OWNER/REPO.git
cd REPO
```

Replace the repository URL and directory with the actual GitHub repository details.

Build and run the application:

```bash
cargo build --release
cargo run --release
```

Run the test suite:

```bash
cargo test
```

If a command fails, check that the native development packages and Rust toolchain are installed. C++ and C# project functionality also depends on the relevant compilers, SDKs, and language tools being available on the system.

## Install for your user

On Linux, the repository includes a user-local installation script:

```bash
bash packaging/linux/install-user.sh
```

The script installs Fugh under the user's `~/.local` directory and adds an application-menu entry. This installation method is intended to work without administrator access.

## Project structure and configuration

Fugh stores application preferences and workspace state using eframe persistence in the platform's application-data location.

Project-specific Fugh settings are stored in the project's `.fugh` directory. Formatter settings can also use standard project files such as:

- `.clang-format` for C++ formatting.
- `.editorconfig` for supported editor and formatter settings.

These files can be committed to source control to share project conventions with other contributors.

## Known limitations

Fugh is under active development. Important current limitations include:

- Language-server features depend on external tools being installed and correctly configured.
- C++ language-server features require clangd and a valid compilation database.
- C# language-server features require the Roslyn language server and a compatible .NET runtime.
- Local completion is lightweight and may not resolve all overloads, visibility rules, or imports.
- Imported Visual Studio `.vcxproj` projects are currently browse/edit only for build purposes; CMake projects are the supported C++ build workflow.
- The visual Avalonia designer supports only a subset of XAML and is not a live application preview.
- One-click package installation and updates are currently Fedora-only.
- Rust/Cargo can be detected as toolchains, but Rust project templates and Rust project workflows are not yet supported.
- Floating/undocked editor panes are not implemented.

See the issue tracker for known bugs, feature requests, and current development priorities.

## Roadmap

The roadmap may change as development progresses. Possible areas of continued work include:

- Improve editor completion and semantic language intelligence.
- Expand project and toolchain support across Linux distributions.
- Improve the visual designer and its supported XAML controls.
- Refine performance, diagnostics, and debugging workflows.
- Continue improving the interface, accessibility, and customization options.

For planned work, see the repository's [Issues](https://github.com/OWNER/REPO/issues) and [Projects](https://github.com/OWNER/REPO/projects) pages if enabled.

## Contributing and bug reports

Contributions, reproducible bug reports, and constructive suggestions are welcome.

Before opening an issue:

1. Check whether a similar issue already exists.
2. Include your Linux distribution and version.
3. Include the Fugh version or commit you are using.
4. Describe the steps needed to reproduce the problem.
5. Include relevant logs or terminal output, removing personal information first.

When submitting code changes, keep changes focused, follow the existing style, and test the affected functionality where possible. Do not include personal paths, credentials, tokens, or private project data in reports.

If the repository has contribution guidelines, follow `CONTRIBUTING.md`.

## Versioning and releases

Published versions should be available through the repository's [Releases](https://github.com/OWNER/REPO/releases) page. The release badge at the top of this README updates automatically when GitHub has a published release.

Until a first release is published, the project should be considered pre-release software. Check the release notes before upgrading or relying on a specific feature.

## License

The license for Fugh must be defined by the repository's `LICENSE` file. **Add the license you have chosen before publishing or distributing the project**, and update this section to name it.

If the repository does not contain a `LICENSE` file, visitors should not assume that the project is open source or that they have permission to redistribute or modify it.

The license badge at the top reads the license metadata from GitHub once a recognized `LICENSE` file is present. It does not choose a license for you.

---

<p align="center">
  Made with Rust and egui.
</p>
