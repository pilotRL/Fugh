use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectLanguage {
    Cpp,
    CSharp,
}

impl ProjectLanguage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cpp => "C++",
            Self::CSharp => "C#",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectTemplate {
    Console,
    CMakeProject,
    ClassLibrary,
    AvaloniaApp,
    QtWidgetsApp,
    VisualStudioCpp,
}

impl ProjectTemplate {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Console => "Console application",
            Self::CMakeProject => "CMake project",
            Self::ClassLibrary => "Class library",
            Self::AvaloniaApp => "Avalonia desktop app",
            Self::QtWidgetsApp => "Qt Widgets desktop app",
            Self::VisualStudioCpp => "Imported Visual Studio C++ project",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub path: PathBuf,
    pub language: ProjectLanguage,
    pub template: ProjectTemplate,
    pub entry_file: PathBuf,
    #[serde(default)]
    pub project_file: Option<PathBuf>,
    #[serde(default)]
    pub properties: ProjectProperties,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectProperties {
    pub cpp: CppProjectProperties,
    pub csharp: CSharpProjectProperties,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CppProjectProperties {
    /// Selected CMake build configuration (Debug, Release, or a custom name).
    pub build_configuration: String,
    /// Native target architecture: Host, x86, or x64.
    pub platform: String,
    /// Empty means the CMake project chooses its own C++ standard.
    pub standard: String,
    pub include_directories: Vec<String>,
    pub library_directories: Vec<String>,
    /// Library names (for example `ssl`) or full library file paths.
    pub libraries: Vec<String>,
    pub definitions: Vec<String>,
    pub compiler_options: Vec<String>,
    pub linker_options: Vec<String>,
    pub runtime_library_directories: Vec<String>,
    /// Runtime DLL/shared-object files copied beside executable targets.
    pub runtime_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CSharpProjectProperties {
    /// Selected MSBuild configuration (Debug, Release, or a custom name).
    pub build_configuration: String,
    /// Compiler target architecture (AnyCPU, x86, x64, ARM, or ARM64).
    pub platform_target: String,
    /// Empty values inherit the corresponding value from the project file.
    pub target_framework: String,
    pub runtime_identifier: String,
    pub assembly_name: String,
    pub root_namespace: String,
    pub language_version: String,
    pub nullable: String,
    pub implicit_usings: String,
    pub treat_warnings_as_errors: String,
    pub nuget_packages: Vec<NuGetPackageReference>,
    pub assembly_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NuGetPackageReference {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub path: Option<PathBuf>,
    pub title: String,
    pub text: String,
    pub dirty: bool,
    #[serde(default)]
    pub breakpoints: Vec<usize>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSymbol {
    pub name: String,
    pub kind: String,
    pub path: PathBuf,
    pub namespace: Option<String>,
    pub line: usize,
}

impl Document {
    pub fn scratch() -> Self {
        Self {
            path: None,
            title: "scratch.txt".to_owned(),
            text: String::from("// Fugh scratch editor\n"),
            dirty: false,
            breakpoints: Vec::new(),
            pinned: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub current: Option<ProjectInfo>,
    pub recent: Vec<ProjectInfo>,
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub directories: Vec<PathBuf>,
    pub documents: Vec<Document>,
    #[serde(default)]
    pub closed_documents: Vec<Document>,
    pub selected_document: usize,
    pub message: String,
    #[serde(skip)]
    pub symbols: Vec<ProjectSymbol>,
    #[serde(skip)]
    pub symbol_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewSourceKind {
    CSharpClass,
    CSharpInterface,
    CSharpEnum,
    CSharpRecord,
    CSharpStruct,
    CSharpStaticClass,
    CppSource,
    CppHeader,
    AvaloniaXaml,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            current: None,
            recent: Vec::new(),
            files: Vec::new(),
            directories: Vec::new(),
            documents: vec![Document::scratch()],
            closed_documents: Vec::new(),
            selected_document: 0,
            message: "Welcome to Fugh".to_owned(),
            symbols: Vec::new(),
            symbol_revision: 0,
        }
    }
}

impl NewSourceKind {
    pub const fn default_name(self) -> &'static str {
        match self {
            Self::CSharpClass => "NewClass",
            Self::CSharpInterface => "INewService",
            Self::CSharpEnum => "NewStatus",
            Self::CSharpRecord => "NewRecord",
            Self::CSharpStruct => "NewStruct",
            Self::CSharpStaticClass => "NewUtilities",
            Self::CppSource => "NewSource",
            Self::CppHeader => "NewHeader",
            Self::AvaloniaXaml => "NewView",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::CSharpClass => "Class",
            Self::CSharpInterface => "Interface",
            Self::CSharpEnum => "Enum",
            Self::CSharpRecord => "Record",
            Self::CSharpStruct => "Struct",
            Self::CSharpStaticClass => "Static class",
            Self::CppSource => "C++ Source",
            Self::CppHeader => "C++ Header",
            Self::AvaloniaXaml => "Avalonia View",
        }
    }

    const fn extension(self) -> &'static str {
        match self {
            Self::CSharpClass
            | Self::CSharpInterface
            | Self::CSharpEnum
            | Self::CSharpRecord
            | Self::CSharpStruct
            | Self::CSharpStaticClass => "cs",
            Self::CppSource => "cpp",
            Self::CppHeader => "hpp",
            Self::AvaloniaXaml => "axaml",
        }
    }
}

impl Workspace {
    pub fn close_project(&mut self) {
        let recent = std::mem::take(&mut self.recent);
        let symbol_revision = self.symbol_revision.wrapping_add(1);
        *self = Self {
            recent,
            ..Self::default()
        };
        self.symbol_revision = symbol_revision;
    }

    pub fn normalize_paths(&mut self) {
        if let Some(project) = &mut self.current {
            project.path = normalize_windows_path(project.path.clone());
        }
        for project in &mut self.recent {
            project.path = normalize_windows_path(project.path.clone());
        }
        for document in &mut self.documents {
            if let Some(path) = &mut document.path {
                *path = normalize_windows_path(path.clone());
            }
        }
    }

    pub fn create_project(
        &mut self,
        parent: &Path,
        name: &str,
        language: ProjectLanguage,
        template: ProjectTemplate,
        framework: &str,
    ) -> Result<ProjectInfo, String> {
        self.create_project_with_csharp_scaffold(parent, name, language, template, framework, false)
    }

    pub fn create_project_with_csharp_scaffold(
        &mut self,
        parent: &Path,
        name: &str,
        language: ProjectLanguage,
        template: ProjectTemplate,
        framework: &str,
        csharp_scaffold: bool,
    ) -> Result<ProjectInfo, String> {
        if template == ProjectTemplate::QtWidgetsApp {
            return Err("Creating new Qt Widgets projects is disabled. Choose a C++ Console or CMake Project template instead.".to_owned());
        }
        validate_project_destination(parent, name)?;
        if template == ProjectTemplate::AvaloniaApp && language != ProjectLanguage::CSharp {
            return Err("Avalonia desktop apps require C#.".to_owned());
        }
        if template == ProjectTemplate::QtWidgetsApp && language != ProjectLanguage::Cpp {
            return Err("Qt Widgets desktop apps require C++.".to_owned());
        }
        if template == ProjectTemplate::CMakeProject && language != ProjectLanguage::Cpp {
            return Err("CMake starter projects require C++.".to_owned());
        }
        if language == ProjectLanguage::CSharp
            && !matches!(framework, "net8.0" | "net9.0" | "net10.0")
        {
            return Err(format!("Unsupported .NET target framework: {framework}"));
        }
        if !parent.is_dir() {
            return Err(format!(
                "The selected project location does not exist: {}",
                parent.display()
            ));
        }

        let project_path = parent.join(name);
        fs::create_dir(&project_path).map_err(|error| {
            format!(
                "Could not create project folder '{}': {error}",
                project_path.display()
            )
        })?;

        let files = match render_template(name, language, template, framework, csharp_scaffold) {
            Ok(template_files) => template_files,
            Err(error) => {
                let _ = fs::remove_dir(&project_path);
                return Err(error);
            }
        };

        for (relative_path, contents) in files {
            let destination = project_path.join(&relative_path);
            if let Some(parent) = destination.parent()
                && let Err(error) = fs::create_dir_all(parent)
            {
                let _ = fs::remove_dir_all(&project_path);
                return Err(format!("Could not create project subfolder: {error}"));
            }
            if let Err(error) = fs::write(&destination, contents) {
                let _ = fs::remove_dir_all(&project_path);
                return Err(format!(
                    "Could not write project file '{}': {error}",
                    destination.display()
                ));
            }
        }

        let canonical_path =
            normalize_windows_path(project_path.canonicalize().map_err(|error| {
                format!(
                    "Could not resolve project folder '{}': {error}",
                    project_path.display()
                )
            })?);
        let info = ProjectInfo {
            name: name.to_owned(),
            path: canonical_path,
            language,
            template,
            entry_file: Self::entry_file(language, template),
            project_file: (language == ProjectLanguage::CSharp)
                .then(|| PathBuf::from(format!("{name}.csproj"))),
            properties: ProjectProperties::default(),
        };
        self.activate_project(info.clone())?;
        Ok(info)
    }

    fn entry_file(language: ProjectLanguage, template: ProjectTemplate) -> PathBuf {
        match (language, template) {
            (ProjectLanguage::Cpp, ProjectTemplate::Console) => PathBuf::from("src/main.cpp"),
            (ProjectLanguage::Cpp, ProjectTemplate::CMakeProject) => PathBuf::from("src/main.cpp"),
            (ProjectLanguage::CSharp, ProjectTemplate::CMakeProject) => PathBuf::from("Program.cs"),
            (ProjectLanguage::CSharp, ProjectTemplate::Console) => PathBuf::from("Program.cs"),
            (ProjectLanguage::CSharp, ProjectTemplate::ClassLibrary) => PathBuf::from("Class1.cs"),
            (ProjectLanguage::CSharp, ProjectTemplate::AvaloniaApp) => {
                PathBuf::from("MainWindow.axaml")
            }
            (ProjectLanguage::Cpp, ProjectTemplate::QtWidgetsApp) => {
                PathBuf::from("src/MainWindow.cpp")
            }
            (ProjectLanguage::Cpp, ProjectTemplate::ClassLibrary) => {
                PathBuf::from("src/library.cpp")
            }
            (ProjectLanguage::Cpp, ProjectTemplate::AvaloniaApp) => PathBuf::from("CMakeLists.txt"),
            (ProjectLanguage::CSharp, ProjectTemplate::QtWidgetsApp) => PathBuf::from("Program.cs"),
            (_, ProjectTemplate::VisualStudioCpp) => PathBuf::from("CMakeLists.txt"),
        }
    }

    pub fn open_project(&mut self, path: &Path) -> Result<(), String> {
        let mut path = normalize_windows_path(path.canonicalize().map_err(|error| {
            format!(
                "Could not open project folder '{}': {error}",
                path.display()
            )
        })?);
        if path.is_file() {
            match path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("sln" | "slnx") => {
                    let solution = fs::read_to_string(&path).map_err(|error| {
                        format!("Could not read solution '{}': {error}", path.display())
                    })?;
                    let project_file = solution_project_path(&path, &solution)?;
                    let project_path = normalize_windows_path(
                        path.parent()
                            .ok_or_else(|| "Solution folder could not be resolved.".to_owned())?
                            .join(project_file.replace('\\', std::path::MAIN_SEPARATOR_STR))
                            .canonicalize()
                            .map_err(|error| {
                                format!(
                                    "Could not locate solution project '{}': {error}",
                                    project_file
                                )
                            })?,
                    );
                    return self.open_project(&project_path);
                }
                Some("vcxproj") => {
                    let project = detect_visual_studio_cpp(&path)?;
                    return self.activate_project(project);
                }
                Some("csproj") => {
                    let project = detect_csharp_project(&path)?;
                    return self.activate_project(project);
                }
                Some("txt")
                    if path
                        .file_name()
                        .is_some_and(|name| name == "CMakeLists.txt") => {}
                _ => {
                    return Err(format!(
                        "Unsupported project file '{}'. Open a Visual Studio solution/project, CMakeLists.txt, or project folder.",
                        path.display()
                    ));
                }
            }
            path = path
                .parent()
                .ok_or_else(|| "Project folder could not be resolved.".to_owned())?
                .to_path_buf();
        }
        if !path.is_dir() {
            return Err(format!("Project path is not a folder: {}", path.display()));
        }
        let info = detect_project(&path)?;
        self.activate_project(info)
    }

    pub fn activate_project(&mut self, info: ProjectInfo) -> Result<(), String> {
        let mut info = info;
        let properties_load_error = match load_project_properties(&info.path) {
            Ok(properties) => {
                info.properties = properties;
                None
            }
            Err(error) => Some(error),
        };
        let files = source_files(&info.path)?;
        let directories = project_directories(&info.path)?;
        if files.is_empty() {
            return Err(format!(
                "No supported source files were found in '{}'.",
                info.path.display()
            ));
        }

        let entry_file = info.entry_file.clone();
        self.current = Some(info.clone());
        self.directories = directories;
        self.symbols.clear();
        self.recent.retain(|recent| recent.path != info.path);
        self.recent.insert(0, info);
        self.recent.truncate(30);
        self.files = files;
        self.documents.clear();
        self.selected_document = 0;
        self.message = properties_load_error.unwrap_or_else(|| "Project opened".to_owned());
        self.open_file(&entry_file)?;
        self.refresh_symbols()
    }

    pub fn save_current_project_properties(&mut self) -> Result<(), String> {
        let project = self
            .current
            .as_ref()
            .ok_or_else(|| "Open a project before editing project properties.".to_owned())?;
        let path = project.path.clone();
        let properties = project.properties.clone();
        let language = project.language;
        let project_file = project.project_file.as_ref().map_or_else(
            || project.path.join(format!("{}.csproj", project.name)),
            |relative| project.path.join(relative),
        );
        if language == ProjectLanguage::CSharp
            && self.documents.iter().any(|document| {
                document.dirty
                    && document.path.as_deref().is_some_and(|open_path| {
                        open_path == project_file.as_path()
                            || open_path.canonicalize().ok().as_deref()
                                == project_file.canonicalize().ok().as_deref()
                    })
            })
        {
            return Err(
                "Save or discard your unsaved .csproj edits before changing project properties."
                    .to_owned(),
            );
        }
        save_project_properties(&path, &properties)?;
        if language == ProjectLanguage::CSharp {
            write_csharp_project_properties(self.current.as_ref().expect("project checked above"))?;
            if let Ok(contents) = fs::read_to_string(&project_file) {
                for document in &mut self.documents {
                    if document.path.as_deref().is_some_and(|open_path| {
                        open_path == project_file.as_path()
                            || open_path.canonicalize().ok().as_deref()
                                == project_file.canonicalize().ok().as_deref()
                    }) {
                        document.text = contents.clone();
                        document.dirty = false;
                    }
                }
            }
        }
        for recent in &mut self.recent {
            if recent.path == path {
                recent.properties = properties.clone();
            }
        }
        self.message = "Project properties saved".to_owned();
        Ok(())
    }

    /// Persist build-toolbar settings without rewriting project files or
    /// rejecting changes because an unrelated .csproj editor tab is dirty.
    pub fn save_current_build_settings(&mut self) -> Result<(), String> {
        let project = self
            .current
            .as_ref()
            .ok_or_else(|| "Open a project before changing build settings.".to_owned())?;
        let path = project.path.clone();
        let properties = project.properties.clone();
        save_project_properties(&path, &properties)?;
        for recent in &mut self.recent {
            if recent.path == path {
                recent.properties = properties.clone();
            }
        }
        self.message = "Build settings saved".to_owned();
        Ok(())
    }

    pub fn create_folder(&mut self, directory: &Path, requested_name: &str) -> Result<(), String> {
        let project = self
            .current
            .as_ref()
            .ok_or_else(|| "Open a project before adding a folder.".to_owned())?;
        let project_root = project.path.clone();
        let parent_path = project_root.join(directory);
        let canonical_parent =
            normalize_windows_path(parent_path.canonicalize().map_err(|error| {
                format!(
                    "Could not access project folder '{}': {error}",
                    parent_path.display()
                )
            })?);
        if !canonical_parent.starts_with(&project_root) || !canonical_parent.is_dir() {
            return Err("New folders must be created inside the current project.".to_owned());
        }
        let name = validate_folder_name(requested_name)?;
        let path = canonical_parent.join(name);
        fs::create_dir(&path)
            .map_err(|error| format!("Could not create folder '{}': {error}", path.display()))?;
        self.directories = project_directories(&project_root)?;
        Ok(())
    }

    pub fn delete_project_item(&mut self, relative_path: &Path) -> Result<(), String> {
        let project_root = self
            .current
            .as_ref()
            .ok_or_else(|| "Open a project before deleting an item.".to_owned())?
            .path
            .clone();
        if relative_path.as_os_str().is_empty() {
            return Err("The project root cannot be deleted from Solution Explorer.".to_owned());
        }
        let path = project_root.join(relative_path);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("Could not inspect '{}': {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err("Symbolic links cannot be deleted from Solution Explorer.".to_owned());
        }
        let canonical_path = normalize_windows_path(
            path.canonicalize()
                .map_err(|error| format!("Could not resolve '{}': {error}", path.display()))?,
        );
        if !canonical_path.starts_with(&project_root) || canonical_path == project_root {
            return Err("Only items inside the current project can be deleted.".to_owned());
        }
        if metadata.is_dir() {
            fs::remove_dir_all(&canonical_path).map_err(|error| {
                format!(
                    "Could not delete folder '{}': {error}",
                    canonical_path.display()
                )
            })?;
        } else {
            fs::remove_file(&canonical_path).map_err(|error| {
                format!(
                    "Could not delete file '{}': {error}",
                    canonical_path.display()
                )
            })?;
        }

        let removed_before_selected = self
            .documents
            .iter()
            .take(self.selected_document)
            .filter(|document| {
                document
                    .path
                    .as_ref()
                    .is_some_and(|document_path| document_path.starts_with(&canonical_path))
            })
            .count();
        self.documents.retain(|document| {
            !document
                .path
                .as_ref()
                .is_some_and(|document_path| document_path.starts_with(&canonical_path))
        });
        if self.documents.is_empty() {
            self.selected_document = 0;
        } else {
            self.selected_document = self
                .selected_document
                .saturating_sub(removed_before_selected)
                .min(self.documents.len() - 1);
        }
        self.files = source_files(&project_root)?;
        self.directories = project_directories(&project_root)?;
        self.refresh_symbols()
    }

    pub fn open_file(&mut self, relative_path: &Path) -> Result<(), String> {
        let project = self
            .current
            .as_ref()
            .ok_or_else(|| "Open a project before opening a source file.".to_owned())?;
        let full_path = project.path.join(relative_path);
        let canonical_path = normalize_windows_path(
            full_path
                .canonicalize()
                .map_err(|error| format!("Could not open '{}': {error}", full_path.display()))?,
        );
        if !canonical_path.starts_with(&project.path) {
            return Err("The selected file is outside the current project.".to_owned());
        }
        if let Some(index) = self
            .documents
            .iter()
            .position(|document| document.path.as_deref() == Some(canonical_path.as_path()))
        {
            if !self.documents[index].dirty {
                self.documents[index].text =
                    fs::read_to_string(&canonical_path).map_err(|error| {
                        format!("Could not reload '{}': {error}", canonical_path.display())
                    })?;
            }
            self.selected_document = index;
            return self.refresh_symbols();
        }

        let text = fs::read_to_string(&canonical_path)
            .map_err(|error| format!("Could not read '{}': {error}", canonical_path.display()))?;
        let breakpoints = self.load_breakpoints(relative_path);
        let title = canonical_path
            .strip_prefix(&project.path)
            .unwrap_or(&canonical_path)
            .to_string_lossy()
            .replace('\\', "/");
        self.documents.push(Document {
            path: Some(canonical_path),
            title,
            text,
            dirty: false,
            breakpoints,
            pinned: false,
        });
        self.selected_document = self.documents.len() - 1;
        self.refresh_symbols()
    }

    pub fn persist_breakpoints(&self) -> Result<(), String> {
        let Some(project) = &self.current else {
            return Ok(());
        };
        let directory = project.path.join(".cairn");
        let path = directory.join("breakpoints.json");
        let mut saved = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, Vec<usize>>>(&bytes).ok())
            .unwrap_or_default();
        for document in &self.documents {
            if let Some(path) = &document.path
                && let Ok(relative) = path.strip_prefix(&project.path)
            {
                let key = relative.to_string_lossy().replace('\\', "/");
                if document.breakpoints.is_empty() {
                    saved.remove(&key);
                } else {
                    saved.insert(key, document.breakpoints.clone());
                }
            }
        }
        fs::create_dir_all(&directory)
            .map_err(|error| format!("Could not save breakpoint settings: {error}"))?;
        if saved.is_empty() {
            let _ = fs::remove_file(path);
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(&saved)
            .map_err(|error| format!("Could not encode breakpoint settings: {error}"))?;
        fs::write(path, bytes).map_err(|error| format!("Could not save breakpoints: {error}"))
    }

    pub fn all_breakpoints(&self) -> Vec<(PathBuf, usize)> {
        let Some(project) = &self.current else {
            return Vec::new();
        };
        let path = project.path.join(".cairn/breakpoints.json");
        let mut saved = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, Vec<usize>>>(&bytes).ok())
            .unwrap_or_default();
        for document in &self.documents {
            if let Some(path) = &document.path
                && let Ok(relative) = path.strip_prefix(&project.path)
            {
                let key = relative.to_string_lossy().replace('\\', "/");
                if document.breakpoints.is_empty() {
                    saved.remove(&key);
                } else {
                    saved.insert(key, document.breakpoints.clone());
                }
            }
        }
        let root = project
            .path
            .canonicalize()
            .unwrap_or_else(|_| project.path.clone());
        let mut breakpoints = saved
            .into_iter()
            .flat_map(|(relative, lines)| {
                let path = project.path.join(relative);
                lines
                    .into_iter()
                    .filter(|line| *line > 0)
                    .map(move |line| (path.clone(), line))
            })
            .filter_map(|(path, line)| {
                let canonical = path.canonicalize().ok()?;
                canonical.starts_with(&root).then_some((canonical, line))
            })
            .collect::<Vec<_>>();
        breakpoints.sort_unstable();
        breakpoints.dedup();
        breakpoints
    }

    fn load_breakpoints(&self, relative_path: &Path) -> Vec<usize> {
        let Some(project) = &self.current else {
            return Vec::new();
        };
        let path = project.path.join(".cairn/breakpoints.json");
        let saved = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, Vec<usize>>>(&bytes).ok());
        let key = relative_path.to_string_lossy().replace('\\', "/");
        saved
            .and_then(|breakpoints| breakpoints.get(&key).cloned())
            .unwrap_or_default()
    }

    pub fn refresh_symbols(&mut self) -> Result<(), String> {
        let Some(project) = &self.current else {
            self.symbols.clear();
            self.symbol_revision = self.symbol_revision.wrapping_add(1);
            return Ok(());
        };
        let root = project.path.clone();
        let mut symbols = Vec::new();
        for relative_path in &self.files {
            let full_path = root.join(relative_path);
            let text = if let Some(document) = self
                .documents
                .iter()
                .find(|document| document.path.as_deref() == Some(full_path.as_path()))
            {
                document.text.clone()
            } else {
                match fs::read_to_string(&full_path) {
                    Ok(text) => text,
                    Err(error) => {
                        return Err(format!(
                            "Could not index project file '{}': {error}",
                            full_path.display()
                        ));
                    }
                }
            };
            let language = match relative_path
                .extension()
                .and_then(|extension| extension.to_str())
            {
                Some("cs") => "cs",
                Some("cpp" | "cc" | "cxx" | "h" | "hpp") => "cpp",
                _ => continue,
            };
            symbols.extend(extract_project_symbols(&text, relative_path, language));
        }
        symbols.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.path.cmp(&right.path))
        });
        symbols.dedup_by(|left, right| {
            left.name == right.name && left.path == right.path && left.kind == right.kind
        });
        self.symbols = symbols;
        self.symbol_revision = self.symbol_revision.wrapping_add(1);
        Ok(())
    }

    /// Re-index only the active editor buffer after an edit. Full project
    /// indexing remains appropriate when opening a project or applying a
    /// multi-file operation, but doing file I/O across the whole workspace
    /// after each typing pause makes the editor stall on larger projects.
    pub fn refresh_active_document_symbols(&mut self) -> Result<(), String> {
        let Some(project_root) = self.current.as_ref().map(|project| project.path.clone()) else {
            return Ok(());
        };
        let Some((path, text)) = self.active_document().and_then(|document| {
            document
                .path
                .as_ref()
                .map(|path| (path.clone(), document.text.clone()))
        }) else {
            return Ok(());
        };
        let Some(relative_path) = path.strip_prefix(&project_root).ok().map(Path::to_path_buf)
        else {
            return Ok(());
        };
        let language = match relative_path
            .extension()
            .and_then(|extension| extension.to_str())
        {
            Some("cs") => "cs",
            Some("cpp" | "cc" | "cxx" | "h" | "hpp") => "cpp",
            _ => return Ok(()),
        };
        self.symbols.retain(|symbol| symbol.path != relative_path);
        self.symbols
            .extend(extract_project_symbols(&text, &relative_path, language));
        self.symbols.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.path.cmp(&right.path))
        });
        self.symbols.dedup_by(|left, right| {
            left.name == right.name && left.path == right.path && left.kind == right.kind
        });
        self.symbol_revision = self.symbol_revision.wrapping_add(1);
        Ok(())
    }

    pub fn create_source_file(
        &mut self,
        directory: &Path,
        kind: NewSourceKind,
        requested_name: &str,
    ) -> Result<(), String> {
        let project = self
            .current
            .as_ref()
            .ok_or_else(|| "Open a project before adding a file.".to_owned())?;
        match (project.language, kind) {
            (
                ProjectLanguage::CSharp,
                NewSourceKind::CSharpClass
                | NewSourceKind::CSharpInterface
                | NewSourceKind::CSharpEnum
                | NewSourceKind::CSharpRecord
                | NewSourceKind::CSharpStruct
                | NewSourceKind::CSharpStaticClass,
            )
            | (ProjectLanguage::CSharp, NewSourceKind::AvaloniaXaml)
            | (ProjectLanguage::Cpp, NewSourceKind::CppSource)
            | (ProjectLanguage::Cpp, NewSourceKind::CppHeader) => {}
            _ => {
                return Err(
                    "This item type does not match the current project language.".to_owned(),
                );
            }
        }
        if kind == NewSourceKind::AvaloniaXaml && project.template != ProjectTemplate::AvaloniaApp {
            return Err("Avalonia views can only be added to an Avalonia project.".to_owned());
        }
        let project_root = project.path.clone();
        let directory_path = project_root.join(directory);
        let canonical_directory =
            normalize_windows_path(directory_path.canonicalize().map_err(|error| {
                format!(
                    "Could not access project folder '{}': {error}",
                    directory_path.display()
                )
            })?);
        if !canonical_directory.starts_with(&project_root) || !canonical_directory.is_dir() {
            return Err("New files must be created inside the current project.".to_owned());
        }

        let name = validate_source_name(requested_name, kind)?;
        let path = canonical_directory.join(&name);
        if path.exists() {
            return Err(format!(
                "A file named '{name}' already exists in this folder."
            ));
        }
        let type_name = Path::new(&name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| "The selected C# item name is not valid.".to_owned())?;

        let contents = match kind {
            NewSourceKind::CSharpClass => {
                format!("namespace {};\n\npublic class {type_name}\n{{\n}}\n", Self::project_namespace(&project_root))
            }
            NewSourceKind::CSharpInterface => {
                format!("namespace {};\n\npublic interface {type_name}\n{{\n}}\n", Self::project_namespace(&project_root))
            }
            NewSourceKind::CSharpEnum => {
                format!("namespace {};\n\npublic enum {type_name}\n{{\n    None\n}}\n", Self::project_namespace(&project_root))
            }
            NewSourceKind::CSharpRecord => {
                format!("namespace {};\n\npublic record {type_name};\n", Self::project_namespace(&project_root))
            }
            NewSourceKind::CSharpStruct => {
                format!("namespace {};\n\npublic struct {type_name}\n{{\n}}\n", Self::project_namespace(&project_root))
            }
            NewSourceKind::CSharpStaticClass => {
                format!("namespace {};\n\npublic static class {type_name}\n{{\n}}\n", Self::project_namespace(&project_root))
            }

            NewSourceKind::CppSource => format!("// {name}\n"),
            NewSourceKind::CppHeader => format!("#pragma once\n\n// {name}\n"),
            NewSourceKind::AvaloniaXaml => {
                "<UserControl xmlns=\"https://github.com/avaloniaui\"\n             xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\">\n  <TextBlock Text=\"New view\" />\n</UserControl>\n".to_owned()
            }
        };
        fs::write(&path, contents).map_err(|error| {
            format!("Could not create source file '{}': {error}", path.display())
        })?;

        if matches!(kind, NewSourceKind::CppSource)
            && let Err(error) = register_cpp_source(&project_root, &path)
        {
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        self.files = source_files(&project_root)?;
        self.open_file(path.strip_prefix(&project_root).unwrap_or(&path))
    }

    fn project_namespace(project_root: &Path) -> String {
        project_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("FughProject")
            .replace('-', "_")
    }

    pub fn save_all_with_line_ending(
        &mut self,
        line_ending: crate::settings::LineEnding,
    ) -> Result<(), String> {
        for document in &mut self.documents {
            if document.dirty
                && let Some(path) = &document.path
            {
                fs::write(path, apply_line_ending(&document.text, line_ending))
                    .map_err(|error| format!("Could not save '{}': {error}", path.display()))?;
                document.dirty = false;
            }
        }
        Ok(())
    }

    pub fn save_selected_with_line_ending(
        &mut self,
        line_ending: crate::settings::LineEnding,
    ) -> Result<(), String> {
        self.save_document_with_line_ending(self.selected_document, line_ending)
    }

    pub fn save_document_with_line_ending(
        &mut self,
        index: usize,
        line_ending: crate::settings::LineEnding,
    ) -> Result<(), String> {
        let (path, text) = {
            let document = self
                .documents
                .get(index)
                .ok_or_else(|| "There is no active editor document.".to_owned())?;
            let Some(path) = &document.path else {
                return Err(
                    "Scratch text is not a project file. Open a source file to save it.".to_owned(),
                );
            };
            (path.clone(), document.text.clone())
        };
        fs::write(&path, apply_line_ending(&text, line_ending))
            .map_err(|error| format!("Could not save '{}': {error}", path.display()))?;
        self.documents[index].dirty = false;
        // The symbol index is derived from the in-memory buffers and is
        // refreshed when they change. Re-scanning every project file during a
        // save needlessly stalls the editor, especially in large workspaces.
        Ok(())
    }

    pub fn save_document_as_with_line_ending(
        &mut self,
        index: usize,
        path: &Path,
        line_ending: crate::settings::LineEnding,
    ) -> Result<(), String> {
        let text = self
            .documents
            .get(index)
            .ok_or_else(|| "There is no editor file to save.".to_owned())?
            .text
            .clone();
        fs::write(path, apply_line_ending(&text, line_ending))
            .map_err(|error| format!("Could not save '{}': {error}", path.display()))?;
        let document = &mut self.documents[index];
        document.path = Some(path.to_path_buf());
        document.title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_owned();
        document.dirty = false;
        Ok(())
    }

    pub fn close_document(&mut self, index: usize) -> Result<(), String> {
        if index >= self.documents.len() {
            return Err("The editor tab is no longer open.".to_owned());
        }
        let closed = self.documents.remove(index);
        self.closed_documents.push(closed);
        if self.closed_documents.len() > 20 {
            self.closed_documents.remove(0);
        }
        if self.documents.is_empty() {
            self.selected_document = 0;
        } else if self.selected_document > index {
            self.selected_document -= 1;
        } else if self.selected_document == index {
            self.selected_document = index.min(self.documents.len() - 1);
        }
        Ok(())
    }

    pub fn reopen_closed_document(&mut self) -> Result<(), String> {
        let document = self
            .closed_documents
            .pop()
            .ok_or_else(|| "There are no recently closed tabs to reopen.".to_owned())?;
        self.documents.push(document);
        self.selected_document = self.documents.len() - 1;
        Ok(())
    }

    pub fn active_document_mut(&mut self) -> Option<&mut Document> {
        self.documents.get_mut(self.selected_document)
    }

    pub fn active_document(&self) -> Option<&Document> {
        self.documents.get(self.selected_document)
    }
}

fn apply_line_ending(text: &str, line_ending: crate::settings::LineEnding) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    match line_ending {
        crate::settings::LineEnding::Lf => normalized,
        crate::settings::LineEnding::CrLf => normalized.replace('\n', "\r\n"),
    }
}

fn validate_source_name(requested: &str, kind: NewSourceKind) -> Result<String, String> {
    let requested = requested.trim();
    let file_name_component = Path::new(requested)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if requested.is_empty() || requested != file_name_component {
        return Err("Enter a file name without a folder path.".to_owned());
    }
    let extension = kind.extension();
    let file_name = if requested.ends_with(&format!(".{extension}")) {
        requested.to_owned()
    } else {
        format!("{requested}.{extension}")
    };
    let stem = Path::new(&file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    if stem.is_empty()
        || !stem
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        || !stem
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
    {
        return Err(
            "Use a name starting with a letter or underscore; only letters, numbers, and underscores are allowed."
                .to_owned(),
        );
    }
    if Path::new(&file_name)
        .extension()
        .and_then(|value| value.to_str())
        != Some(extension)
    {
        return Err(format!("Use the .{extension} file extension."));
    }
    Ok(file_name)
}

fn validate_folder_name(requested: &str) -> Result<&str, String> {
    let name = requested.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name
            .chars()
            .last()
            .is_some_and(|character| matches!(character, '.' | ' '))
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
    {
        return Err("Enter a valid folder name without a path or reserved characters.".to_owned());
    }
    if Path::new(name).components().count() != 1 {
        return Err("Enter a folder name, not a path.".to_owned());
    }
    Ok(name)
}

pub fn normalize_windows_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    if let Some(path) = path.to_str() {
        if let Some(unc_path) = path.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{unc_path}"));
        }
        if let Some(path) = path.strip_prefix(r"\\?\") {
            return PathBuf::from(path);
        }
    }
    path
}

fn validate_project_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        && name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic());
    if valid {
        Ok(())
    } else {
        Err("Use a project name beginning with a letter and containing only letters, numbers, '-' or '_' (max 64 characters).".to_owned())
    }
}

pub fn validate_project_destination(parent: &Path, name: &str) -> Result<PathBuf, String> {
    validate_project_name(name)?;
    if parent.as_os_str().is_empty() || !parent.is_dir() {
        return Err(format!(
            "Choose an existing destination folder. The selected location is not a folder: {}",
            parent.display()
        ));
    }
    let destination = parent.join(name);
    match destination.try_exists() {
        Ok(true) => Err(format!(
            "A file or folder already exists at '{}'. Choose a different project name or location.",
            destination.display()
        )),
        Ok(false) => Ok(destination),
        Err(error) => Err(format!(
            "Could not check the project destination '{}': {error}",
            destination.display()
        )),
    }
}

fn solution_project_path(solution_path: &Path, contents: &str) -> Result<String, String> {
    let extension = solution_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let candidate = if extension == "slnx" {
        let document = roxmltree::Document::parse(contents)
            .map_err(|error| format!("Could not parse solution XML: {error}"))?;
        document
            .descendants()
            .filter(|node| node.has_tag_name("Project"))
            .filter_map(|node| node.attribute("Path"))
            .find(|path| supported_solution_project(path))
            .map(str::to_owned)
    } else {
        contents.lines().find_map(|line| {
            if !line.trim_start().starts_with("Project(") {
                return None;
            }
            line.split('"')
                .find(|value| supported_solution_project(value))
                .map(str::to_owned)
        })
    };
    candidate.ok_or_else(|| {
        "The selected solution contains no supported C# or Visual Studio C++ project.".to_owned()
    })
}

fn supported_solution_project(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.ends_with(".csproj") || path.ends_with(".vcxproj")
}

fn detect_visual_studio_cpp(project_file: &Path) -> Result<ProjectInfo, String> {
    let contents = fs::read_to_string(project_file).map_err(|error| {
        format!(
            "Could not read Visual Studio C++ project '{}': {error}",
            project_file.display()
        )
    })?;
    let document = roxmltree::Document::parse(&contents)
        .map_err(|error| format!("Could not parse Visual Studio C++ project XML: {error}"))?;
    let root = normalize_windows_path(
        project_file
            .parent()
            .ok_or_else(|| "Visual Studio project folder could not be resolved.".to_owned())?
            .canonicalize()
            .map_err(|error| format!("Could not resolve Visual Studio project folder: {error}"))?,
    );
    let mut sources = document
        .descendants()
        .filter(|node| node.tag_name().name() == "ClCompile")
        .filter_map(|node| node.attribute("Include"))
        .map(|include| PathBuf::from(include.replace('\\', "/")))
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .is_some_and(|extension| matches!(extension.as_str(), "c" | "cc" | "cpp" | "cxx"))
        })
        .filter_map(|relative| {
            let canonical = normalize_windows_path(root.join(&relative).canonicalize().ok()?);
            if canonical.starts_with(&root) {
                canonical.strip_prefix(&root).ok().map(Path::to_path_buf)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    sources.sort();
    sources.dedup();
    let entry_file = sources
        .iter()
        .find(|source| source.file_name().is_some_and(|name| name == "main.cpp"))
        .or_else(|| sources.first())
        .cloned()
        .ok_or_else(|| {
            "No existing C/C++ source files were found in the Visual Studio project.".to_owned()
        })?;
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Visual Studio project folder must have a valid name.".to_owned())?
        .to_owned();
    Ok(ProjectInfo {
        name,
        path: root,
        language: ProjectLanguage::Cpp,
        template: ProjectTemplate::VisualStudioCpp,
        entry_file,
        project_file: project_file.file_name().map(PathBuf::from),
        properties: ProjectProperties::default(),
    })
}

fn detect_csharp_project(project_file: &Path) -> Result<ProjectInfo, String> {
    let contents = fs::read_to_string(project_file).map_err(|error| {
        format!(
            "Could not read C# project '{}': {error}",
            project_file.display()
        )
    })?;
    let document = roxmltree::Document::parse(&contents)
        .map_err(|error| format!("The C# project file is not valid XML: {error}"))?;
    if document.root_element().tag_name().name() != "Project" {
        return Err("The selected XML file is not an MSBuild Project file.".to_owned());
    }
    let root = normalize_windows_path(
        project_file
            .parent()
            .ok_or_else(|| "C# project folder could not be resolved.".to_owned())?
            .canonicalize()
            .map_err(|error| format!("Could not resolve C# project folder: {error}"))?,
    );
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "C# project folder must have a valid name.".to_owned())?
        .to_owned();
    let template = if contents.contains("Avalonia.Desktop") {
        ProjectTemplate::AvaloniaApp
    } else if document
        .descendants()
        .find(|node| node.tag_name().name() == "OutputType")
        .and_then(|node| node.text())
        .is_none_or(|output_type| output_type.eq_ignore_ascii_case("Library"))
    {
        ProjectTemplate::ClassLibrary
    } else {
        ProjectTemplate::Console
    };
    let files = source_files(&root)?;
    let expected = match template {
        ProjectTemplate::AvaloniaApp => Some(PathBuf::from("MainWindow.axaml")),
        ProjectTemplate::Console => Some(PathBuf::from("Program.cs")),
        _ => None,
    };
    let entry_file = expected
        .filter(|file| root.join(file).is_file())
        .or_else(|| {
            files
                .iter()
                .find(|file| file.extension().is_some_and(|extension| extension == "cs"))
                .cloned()
        })
        .or_else(|| files.first().cloned())
        .ok_or_else(|| "No supported source files were found in this C# project.".to_owned())?;
    Ok(ProjectInfo {
        name,
        path: root,
        language: ProjectLanguage::CSharp,
        template,
        entry_file,
        project_file: project_file.file_name().map(PathBuf::from),
        properties: ProjectProperties::default(),
    })
}

fn detect_project(path: &Path) -> Result<ProjectInfo, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Project folder must have a valid name.".to_owned())?
        .to_owned();

    let mut projects = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| format!("Could not scan project: {error}"))? {
        let entry = entry.map_err(|error| format!("Could not read project entry: {error}"))?;
        let candidate = entry.path();
        if candidate
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("csproj"))
        {
            projects.push((ProjectLanguage::CSharp, candidate));
        } else if candidate
            .file_name()
            .is_some_and(|name| name == "CMakeLists.txt")
        {
            projects.push((ProjectLanguage::Cpp, candidate));
        } else if candidate
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("vcxproj"))
        {
            projects.push((ProjectLanguage::Cpp, candidate));
        }
    }
    if projects.is_empty() {
        return Err("No top-level CMakeLists.txt, .csproj or .vcxproj was found.".to_owned());
    }
    if projects.len() > 1 {
        return Err("This folder contains multiple project files. Open the specific project or solution file to choose one.".to_owned());
    }
    let (language, project_file) = projects.into_iter().next().expect("checked above");

    if project_file
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("vcxproj"))
    {
        return detect_visual_studio_cpp(&project_file);
    }

    if language == ProjectLanguage::CSharp {
        return detect_csharp_project(&project_file);
    }

    let contents = fs::read_to_string(&project_file)
        .map_err(|error| format!("Could not inspect CMake project: {error}"))?;
    let normalized = contents.to_ascii_lowercase();
    if ![
        "project(",
        "add_executable(",
        "add_library(",
        "add_subdirectory(",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
    {
        return Err(
            "CMakeLists.txt does not contain a CMake project, target, or subdirectory declaration."
                .to_owned(),
        );
    }
    let template = if contents.contains("Qt6") && contents.contains("Widgets") {
        ProjectTemplate::QtWidgetsApp
    } else if contents.contains("# Fugh template: cpp-class-library") {
        ProjectTemplate::ClassLibrary
    } else if contents.contains("# Fugh template: cmake-project")
        || contents.contains("# Cairn template: cmake-project")
    {
        ProjectTemplate::CMakeProject
    } else {
        ProjectTemplate::Console
    };
    let files = source_files(path)?;
    let entry_file = if template == ProjectTemplate::AvaloniaApp {
        PathBuf::from("MainWindow.axaml")
    } else if template == ProjectTemplate::QtWidgetsApp {
        PathBuf::from("src/MainWindow.cpp")
    } else {
        files
            .iter()
            .find(|file| {
                file.file_name().is_some_and(|name| {
                    name == "main.cpp" || name == "Program.cs" || name == "library.cpp"
                })
            })
            .cloned()
            .unwrap_or_else(|| PathBuf::from("CMakeLists.txt"))
    };

    Ok(ProjectInfo {
        name,
        path: path.to_path_buf(),
        language,
        template,
        entry_file,
        project_file: None,
        properties: ProjectProperties::default(),
    })
}

pub fn project_properties_file(project_root: &Path) -> PathBuf {
    project_root.join(".fugh").join("project-properties.json")
}

fn load_project_properties(project_root: &Path) -> Result<ProjectProperties, String> {
    let path = project_properties_file(project_root);
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "Could not read project properties '{}': {error}",
                path.display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(ProjectProperties::default())
        }
        Err(error) => Err(format!(
            "Could not read project properties '{}': {error}",
            path.display()
        )),
    }
}

pub fn save_project_properties(
    project_root: &Path,
    properties: &ProjectProperties,
) -> Result<(), String> {
    let path = project_properties_file(project_root);
    let directory = path
        .parent()
        .ok_or_else(|| "Project properties folder could not be resolved.".to_owned())?;
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "Could not create project properties folder '{}': {error}",
            directory.display()
        )
    })?;
    let contents = serde_json::to_vec_pretty(properties)
        .map_err(|error| format!("Could not serialize project properties: {error}"))?;
    fs::write(&path, contents).map_err(|error| {
        format!(
            "Could not save project properties '{}': {error}",
            path.display()
        )
    })
}

fn write_csharp_project_properties(project: &ProjectInfo) -> Result<(), String> {
    let project_file = project.project_file.as_ref().map_or_else(
        || project.path.join(format!("{}.csproj", project.name)),
        |relative| project.path.join(relative),
    );
    let project_contents = fs::read_to_string(&project_file).map_err(|error| {
        format!(
            "Could not read C# project file '{}': {error}",
            project_file.display()
        )
    })?;
    let document = roxmltree::Document::parse(&project_contents)
        .map_err(|error| format!("Could not parse C# project file: {error}"))?;
    let has_properties_import = document
        .descendants()
        .filter(|node| node.tag_name().name() == "Import")
        .filter_map(|node| node.attribute("Project"))
        .any(|import| import == ".fugh/project-properties.targets");
    let properties = &project.properties.csharp;
    let has_configured_items = properties
        .nuget_packages
        .iter()
        .any(|package| !package.id.trim().is_empty())
        || properties
            .assembly_references
            .iter()
            .any(|reference| !reference.trim().is_empty());
    // Build properties are passed to MSBuild directly. Avoid touching a
    // project's .csproj until it actually needs the generated reference items.
    if !has_properties_import && !has_configured_items {
        return Ok(());
    }

    let properties_file = project
        .path
        .join(".fugh")
        .join("project-properties.targets");
    let directory = properties_file
        .parent()
        .ok_or_else(|| "C# project properties folder could not be resolved.".to_owned())?;
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "Could not create C# project properties folder '{}': {error}",
            directory.display()
        )
    })?;
    let mut contents = String::from("<Project>\n  <ItemGroup>\n");
    for package in &project.properties.csharp.nuget_packages {
        let id = package.id.trim();
        if id.is_empty() {
            continue;
        }
        let already_referenced = document.descendants().any(|node| {
            node.tag_name().name() == "PackageReference"
                && node
                    .attribute("Include")
                    .or_else(|| node.attribute("Update"))
                    .is_some_and(|include| include.eq_ignore_ascii_case(id))
        });
        let operation = if already_referenced {
            "Update"
        } else {
            "Include"
        };
        contents.push_str(&format!(
            "    <PackageReference {operation}=\"{}\"",
            escape_xml(id)
        ));
        if !package.version.trim().is_empty() {
            contents.push_str(&format!(
                " Version=\"{}\"",
                escape_xml(package.version.trim())
            ));
        }
        contents.push_str(" />\n");
    }
    for reference in &project.properties.csharp.assembly_references {
        let reference = reference.trim();
        if reference.is_empty() {
            continue;
        }
        let reference_path = PathBuf::from(reference);
        let reference_path_text = if reference_path.is_absolute() {
            reference_path.display().to_string()
        } else {
            reference.replace('\\', "/")
        };
        let name = reference_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("AssemblyReference");
        let already_referenced = document.descendants().any(|node| {
            node.tag_name().name() == "Reference"
                && node
                    .attribute("Include")
                    .or_else(|| node.attribute("Update"))
                    .is_some_and(|include| {
                        include
                            .split(',')
                            .next()
                            .is_some_and(|include| include.trim().eq_ignore_ascii_case(name))
                    })
        });
        let operation = if already_referenced {
            "Update"
        } else {
            "Include"
        };
        contents.push_str(&format!(
            "    <Reference {operation}=\"{}\"><HintPath>{}</HintPath><Private>true</Private></Reference>\n",
            escape_xml(name),
            escape_xml(&reference_path_text)
        ));
    }
    contents.push_str("  </ItemGroup>\n</Project>\n");
    fs::write(&properties_file, contents).map_err(|error| {
        format!(
            "Could not write C# project references '{}': {error}",
            properties_file.display()
        )
    })?;

    if has_properties_import {
        return Ok(());
    }
    let close = project_contents
        .rfind("</Project>")
        .ok_or_else(|| "C# project file does not have a closing Project element.".to_owned())?;
    let updated = format!(
        "{}  <Import Project=\".fugh/project-properties.targets\" Condition=\"Exists('.fugh/project-properties.targets')\" />\n{}",
        &project_contents[..close],
        &project_contents[close..]
    );
    fs::write(&project_file, updated).map_err(|error| {
        format!(
            "Could not update C# project file '{}': {error}",
            project_file.display()
        )
    })
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn source_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("Could not list '{}': {error}", directory.display()))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("Could not read directory entry: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Could not inspect project item: {error}"))?;
            if file_type.is_dir() {
                let name = entry.file_name();
                if !matches!(
                    name.to_str(),
                    Some("build" | "bin" | "obj" | ".git" | ".cairn" | ".fugh" | "target" | ".vs")
                ) {
                    pending.push(entry.path());
                }
            } else if file_type.is_file() {
                let path = entry.path();
                if path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(str::to_ascii_lowercase)
                    .is_some_and(|extension| {
                        matches!(
                            extension.as_str(),
                            "c" | "cc"
                                | "cpp"
                                | "cxx"
                                | "h"
                                | "hh"
                                | "hpp"
                                | "hxx"
                                | "cs"
                                | "cmake"
                                | "axaml"
                                | "xaml"
                        )
                    })
                    || path
                        .file_name()
                        .is_some_and(|name| name == "CMakeLists.txt")
                {
                    files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

fn project_directories(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut directories = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("Could not list '{}': {error}", directory.display()))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("Could not read directory entry: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Could not inspect project item: {error}"))?;
            if file_type.is_dir() {
                let path = entry.path();
                let name = entry.file_name();
                if !matches!(
                    name.to_str(),
                    Some("build" | "bin" | "obj" | ".git" | ".cairn" | ".fugh" | "target" | ".vs")
                ) {
                    directories.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
                    pending.push(path);
                }
            }
        }
    }
    directories.sort();
    Ok(directories)
}

pub(crate) fn extract_project_symbols(
    text: &str,
    path: &Path,
    language: &str,
) -> Vec<ProjectSymbol> {
    let mut symbols = Vec::new();
    let namespace = text.lines().find_map(parse_namespace);
    let type_keywords = ["class", "struct", "interface", "enum", "record"];
    let ignored_functions = [
        "if",
        "for",
        "while",
        "switch",
        "catch",
        "return",
        "throw",
        "using",
        "namespace",
        "sizeof",
        "typeof",
        "nameof",
        "new",
        "lock",
    ];
    for (line_index, line) in text.lines().enumerate() {
        let line = line.split_once("//").map_or(line, |(code, _)| code).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for keyword in type_keywords {
            let Some(position) = find_word(line, keyword) else {
                continue;
            };
            if let Some(name) = next_identifier(&line[position + keyword.len()..]) {
                symbols.push(ProjectSymbol {
                    name,
                    kind: keyword.to_owned(),
                    path: path.to_path_buf(),
                    namespace: namespace.clone(),
                    line: line_index + 1,
                });
            }
        }

        let Some(open_paren) = line.find('(') else {
            continue;
        };
        let before_paren = line[..open_paren].trim_end();
        let name_start = before_paren
            .char_indices()
            .rev()
            .find(|(_, character)| !character.is_ascii_alphanumeric() && *character != '_')
            .map_or(0, |(index, character)| index + character.len_utf8());
        let name = &before_paren[name_start..];
        if !is_identifier(name) || ignored_functions.contains(&name) {
            continue;
        }
        let declaration_prefix = before_paren[..name_start].trim_end();
        if declaration_prefix.is_empty()
            || declaration_prefix.ends_with('.')
            || declaration_prefix.ends_with("->")
            || declaration_prefix.starts_with("return ")
            || declaration_prefix.starts_with("throw ")
        {
            continue;
        }
        let Some(close_paren) = line[open_paren + 1..].find(')') else {
            continue;
        };
        let after_parameters = line[open_paren + 1 + close_paren + 1..].trim_start();
        if !after_parameters.is_empty()
            && !after_parameters.starts_with('{')
            && !after_parameters.starts_with(';')
            && !after_parameters.starts_with("=>")
            && !after_parameters.starts_with("const")
            && !after_parameters.starts_with("noexcept")
        {
            continue;
        }
        let kind = if language == "cs" {
            "C# function"
        } else {
            "C++ function"
        };
        symbols.push(ProjectSymbol {
            name: name.to_owned(),
            kind: kind.to_owned(),
            path: path.to_path_buf(),
            namespace: namespace.clone(),
            line: line_index + 1,
        });
    }
    symbols
}

fn parse_namespace(line: &str) -> Option<String> {
    let line = line.trim();
    let declaration = line.strip_prefix("namespace ")?;
    let name = declaration
        .split([';', '{', ' '])
        .next()
        .unwrap_or_default()
        .trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn find_word(text: &str, word: &str) -> Option<usize> {
    text.match_indices(word).find_map(|(index, _)| {
        let before_is_boundary = text[..index]
            .chars()
            .next_back()
            .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_');
        let end = index + word.len();
        let after_is_boundary = text[end..]
            .chars()
            .next()
            .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_');
        (before_is_boundary && after_is_boundary).then_some(index)
    })
}

fn next_identifier(text: &str) -> Option<String> {
    let mut chars = text.char_indices();
    let (start, first) =
        chars.find(|(_, character)| character.is_ascii_alphabetic() || *character == '_')?;
    let end = text[start..]
        .char_indices()
        .find(|(_, character)| !character.is_ascii_alphanumeric() && *character != '_')
        .map_or(text.len(), |(offset, _)| start + offset);
    (first.is_ascii_alphabetic() || first == '_').then(|| text[start..end].to_owned())
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn register_cpp_source(project_root: &Path, source: &Path) -> Result<(), String> {
    let cmake_path = project_root.join("CMakeLists.txt");
    let mut cmake = fs::read_to_string(&cmake_path)
        .map_err(|error| format!("Could not update '{}': {error}", cmake_path.display()))?;
    let command_start = cmake.find("add_executable(").ok_or_else(|| {
        format!(
            "Could not find an add_executable target in '{}'; add the source manually.",
            cmake_path.display()
        )
    })?;
    let line_end = cmake[command_start..]
        .find('\n')
        .map_or(cmake.len(), |offset| command_start + offset);
    let command_line = &cmake[command_start..line_end];
    if !command_line.contains(')') {
        return Err(format!(
            "Could not parse the executable target in '{}'; add the source manually.",
            cmake_path.display()
        ));
    }
    let target = command_line
        .strip_prefix("add_executable(")
        .and_then(|arguments| arguments.split_whitespace().next())
        .map(|target| target.trim_end_matches(')'))
        .filter(|target| !target.is_empty())
        .ok_or_else(|| {
            format!(
                "Could not resolve the executable target in '{}'.",
                cmake_path.display()
            )
        })?;
    let relative_source = source
        .strip_prefix(project_root)
        .unwrap_or(source)
        .to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\\\"");
    let target_sources = format!("target_sources({target} PRIVATE \"{relative_source}\")");
    if cmake.contains(&target_sources) {
        return Ok(());
    }
    let insertion = if line_end < cmake.len() {
        line_end + 1
    } else {
        line_end
    };
    cmake.insert_str(insertion, &format!("{target_sources}\n"));
    fs::write(&cmake_path, cmake)
        .map_err(|error| format!("Could not update '{}': {error}", cmake_path.display()))
}

fn render_template(
    name: &str,
    language: ProjectLanguage,
    template: ProjectTemplate,
    framework: &str,
    csharp_scaffold: bool,
) -> Result<Vec<(PathBuf, String)>, String> {
    let files = match (language, template) {
        (ProjectLanguage::Cpp, ProjectTemplate::Console) => vec![
            (
                PathBuf::from("CMakeLists.txt"),
                format!(
                    "cmake_minimum_required(VERSION 3.16)\nproject({name} LANGUAGES CXX)\n\n\
                     set(CMAKE_CXX_STANDARD 14)\nset(CMAKE_CXX_STANDARD_REQUIRED ON)\n\n\
                     add_executable({name} src/main.cpp)\n"
                ),
            ),
            (
                PathBuf::from("src/main.cpp"),
                format!(
                    "#include <iostream>\n\nint main() {{\n    std::cout << \"Hello from {name}!\\n\";\n    return 0;\n}}\n"
                ),
            ),
        ],
        (ProjectLanguage::Cpp, ProjectTemplate::CMakeProject) => vec![
            (
                PathBuf::from("CMakeLists.txt"),
                format!(
                    "# Fugh template: cmake-project\ncmake_minimum_required(VERSION 3.16)\nproject({name} LANGUAGES CXX)\n\nset(CMAKE_CXX_STANDARD 20)\nset(CMAKE_CXX_STANDARD_REQUIRED ON)\nset(CMAKE_EXPORT_COMPILE_COMMANDS ON)\n\nadd_executable({name} src/main.cpp)\n"
                ),
            ),
            (
                PathBuf::from("src/main.cpp"),
                format!(
                    "#include <iostream>\n\nint main() {{\n    std::cout << \"Hello from {name}!\\n\";\n    return 0;\n}}\n"
                ),
            ),
        ],
        (ProjectLanguage::Cpp, ProjectTemplate::ClassLibrary) => {
            let namespace = name.replace('-', "_");
            vec![
                (
                    PathBuf::from("CMakeLists.txt"),
                    format!(
                        "# Fugh template: cpp-class-library\ncmake_minimum_required(VERSION 3.16)\nproject({name} LANGUAGES CXX)\n\nset(CMAKE_CXX_STANDARD 14)\nset(CMAKE_CXX_STANDARD_REQUIRED ON)\nset(CMAKE_EXPORT_COMPILE_COMMANDS ON)\n\nadd_library({name} src/library.cpp)\ntarget_include_directories({name} PUBLIC \"${{CMAKE_CURRENT_SOURCE_DIR}}/include\")\n"
                    ),
                ),
                (
                    PathBuf::from(format!("include/{name}/library.hpp")),
                    format!(
                        "#pragma once\n\n#include <string>\n\nnamespace {namespace} {{\n\nclass Greeter {{\npublic:\n    std::string greeting() const;\n}};\n\n}} // namespace {namespace}\n"
                    ),
                ),
                (
                    PathBuf::from("src/library.cpp"),
                    format!(
                        "#include \"{name}/library.hpp\"\n\nnamespace {namespace} {{\n\nstd::string Greeter::greeting() const {{\n    return \"Hello from {name}!\";\n}}\n\n}} // namespace {namespace}\n"
                    ),
                ),
            ]
        }
        (ProjectLanguage::Cpp, ProjectTemplate::QtWidgetsApp) => vec![
            (
                PathBuf::from("CMakeLists.txt"),
                format!(
                    "cmake_minimum_required(VERSION 3.16)\nproject({name} LANGUAGES CXX)\n\n\
                     set(CMAKE_CXX_STANDARD 17)\nset(CMAKE_CXX_STANDARD_REQUIRED ON)\n\
                     set(CMAKE_AUTOMOC ON)\n\nfind_package(Qt6 REQUIRED COMPONENTS Widgets)\n\n\
                     add_executable({name} src/main.cpp src/MainWindow.cpp)\n\
                     target_link_libraries({name} PRIVATE Qt6::Widgets)\n"
                ),
            ),
            (
                PathBuf::from("src/main.cpp"),
                "#include \"MainWindow.h\"\n#include <QApplication>\n\nint main(int argc, char* argv[]) {\n    QApplication app(argc, argv);\n    MainWindow window;\n    window.show();\n    return app.exec();\n}\n".to_owned(),
            ),
            (
                PathBuf::from("src/MainWindow.h"),
                "#pragma once\n\n#include <QLabel>\n#include <QLineEdit>\n#include <QPushButton>\n#include <QWidget>\n\nclass MainWindow : public QWidget {\n    Q_OBJECT\n\npublic:\n    explicit MainWindow(QWidget* parent = nullptr);\n\n    // <cairn-generated-members>\n    // </cairn-generated-members>\n};\n".to_owned(),
            ),
            (
                PathBuf::from("src/MainWindow.cpp"),
                format!(
                    "#include \"MainWindow.h\"\n#include <QLabel>\n#include <QLineEdit>\n#include <QPushButton>\n\nMainWindow::MainWindow(QWidget* parent) : QWidget(parent) {{\n    // <cairn-generated-ui>\n    setWindowTitle(\"{name}\");\n    resize(900, 600);\n    // </cairn-generated-ui>\n}}\n"
                ),
            ),
            (
                PathBuf::from("MainWindow.cairn-ui"),
                format!("window\t{name}\t900\t600\n"),
            ),
        ],
        (ProjectLanguage::CSharp, ProjectTemplate::Console) => vec![
            (
                PathBuf::from(format!("{name}.csproj")),
                format!(
                    "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    \
                     <OutputType>Exe</OutputType>\n    <TargetFramework>{framework}</TargetFramework>\n    \
                     <ImplicitUsings>enable</ImplicitUsings>\n    <Nullable>enable</Nullable>\n  \
                     </PropertyGroup>\n</Project>\n"
                ),
            ),
            (
                PathBuf::from("Program.cs"),
                if csharp_scaffold {
                    format!(
                        "namespace {};\n\ninternal static class Program\n{{\n    private static void Main(string[] args)\n    {{\n        Console.WriteLine(\"Hello from {name}!\");\n    }}\n}}\n",
                        name.replace('-', "_")
                    )
                } else {
                    format!("Console.WriteLine(\"Hello from {name}!\");\n")
                },
            ),
        ],
        (ProjectLanguage::CSharp, ProjectTemplate::ClassLibrary) => vec![
            (
                PathBuf::from(format!("{name}.csproj")),
                format!(
                    "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    \
                     <TargetFramework>{framework}</TargetFramework>\n    \
                     <ImplicitUsings>enable</ImplicitUsings>\n    <Nullable>enable</Nullable>\n  \
                     </PropertyGroup>\n</Project>\n"
                ),
            ),
            (
                PathBuf::from("Class1.cs"),
                format!(
                    "namespace {};\n\npublic class Class1\n{{\n    public string Name => \"{name}\";\n}}\n",
                    name.replace('-', "_")
                ),
            ),
        ],
        (ProjectLanguage::CSharp, ProjectTemplate::AvaloniaApp) => vec![
            (
                PathBuf::from(format!("{name}.csproj")),
                format!(
                    "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    \
                     <OutputType>WinExe</OutputType>\n    <TargetFramework>{framework}</TargetFramework>\n    \
                     <RollForward>Major</RollForward>\n    \
                     <Nullable>enable</Nullable>\n    <AvaloniaUseCompiledBindingsByDefault>true</AvaloniaUseCompiledBindingsByDefault>\n  \
                     </PropertyGroup>\n  <ItemGroup>\n    \
                     <PackageReference Include=\"Avalonia\" Version=\"11.3.9\" />\n    \
                     <PackageReference Include=\"Avalonia.Desktop\" Version=\"11.3.9\" />\n    \
                     <PackageReference Include=\"Avalonia.Themes.Fluent\" Version=\"11.3.9\" />\n  \
                     </ItemGroup>\n</Project>\n"
                ),
            ),
            (
                PathBuf::from("Program.cs"),
                "using System;\nusing Avalonia;\n\nnamespace CairnApp;\n\ninternal static class Program\n{\n    [STAThread]\n    public static void Main(string[] args) => BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);\n\n    public static AppBuilder BuildAvaloniaApp() => AppBuilder.Configure<App>()\n        .UsePlatformDetect()\n        .LogToTrace();\n}\n".to_owned(),
            ),
            (
                PathBuf::from("App.axaml"),
                "<Application xmlns=\"https://github.com/avaloniaui\"\n             xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\"\n             xmlns:Fluent=\"using:Avalonia.Themes.Fluent\"\n             x:Class=\"CairnApp.App\"\n             RequestedThemeVariant=\"Dark\">\n  <Application.Styles>\n    <Fluent:FluentTheme />\n  </Application.Styles>\n</Application>\n".to_owned(),
            ),
            (
                PathBuf::from("App.axaml.cs"),
                "using Avalonia;\nusing Avalonia.Controls.ApplicationLifetimes;\nusing Avalonia.Markup.Xaml;\n\nnamespace CairnApp;\n\npublic partial class App : Application\n{\n    public override void Initialize() => AvaloniaXamlLoader.Load(this);\n\n    public override void OnFrameworkInitializationCompleted()\n    {\n        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)\n            desktop.MainWindow = new MainWindow();\n        base.OnFrameworkInitializationCompleted();\n    }\n}\n".to_owned(),
            ),
            (
                PathBuf::from("MainWindow.axaml"),
                format!(
                    "<Window xmlns=\"https://github.com/avaloniaui\"\n        xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\"\n        x:Class=\"CairnApp.MainWindow\"\n        Title=\"{name}\" Width=\"1100\" Height=\"760\" MinWidth=\"480\" MinHeight=\"360\">\n  <Viewbox Stretch=\"Uniform\">\n    <Canvas Width=\"900\" Height=\"564\" Background=\"#FF1E1E1E\">\n    <TextBlock x:Name=\"WelcomeText\" Text=\"Welcome to {name}!\" Width=\"500\" FontSize=\"24\" Foreground=\"White\" Canvas.Left=\"36\" Canvas.Top=\"32\" />\n    </Canvas>\n  </Viewbox>\n</Window>\n"
                ),
            ),
            (
                PathBuf::from("MainWindow.axaml.cs"),
                "using System;\nusing Avalonia.Controls;\nusing Avalonia.Markup.Xaml;\n\nnamespace CairnApp;\n\npublic partial class MainWindow : Window\n{\n    public MainWindow()\n    {\n        InitializeComponent();\n    }\n}\n".to_owned(),
            ),
        ],
        (ProjectLanguage::Cpp, ProjectTemplate::AvaloniaApp) => {
            return Err("Avalonia desktop apps require C#.".to_owned());
        }
        (ProjectLanguage::CSharp, ProjectTemplate::QtWidgetsApp) => {
            return Err("Qt Widgets desktop apps require C++.".to_owned());
        }
        (ProjectLanguage::CSharp, ProjectTemplate::CMakeProject) => {
            return Err("CMake starter projects require C++.".to_owned());
        }
        (_, ProjectTemplate::VisualStudioCpp) => {
            return Err("Visual Studio C++ projects are imported from existing .vcxproj files.".to_owned());
        }
    };
    Ok(files)
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::normalize_windows_path;
    use super::{
        Document, NewSourceKind, NuGetPackageReference, ProjectLanguage, ProjectTemplate,
        Workspace, render_template, validate_project_name, validate_source_name,
    };
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    fn temp_directory() -> PathBuf {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("cairn-test-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("temporary directory created");
        path
    }

    #[test]
    fn project_properties_survive_reopening_and_stay_out_of_solution_explorer() {
        let parent = temp_directory();
        let mut workspace = Workspace::default();
        let project = workspace
            .create_project(
                &parent,
                "PropertiesDemo",
                ProjectLanguage::Cpp,
                ProjectTemplate::Console,
                "net10.0",
            )
            .expect("project created");
        let current = workspace.current.as_mut().expect("project is open");
        current.properties.cpp.standard = "17".to_owned();
        current
            .properties
            .cpp
            .include_directories
            .push("vendor/include".to_owned());
        workspace
            .save_current_project_properties()
            .expect("project properties saved");

        let mut reopened = Workspace::default();
        reopened
            .open_project(&project.path)
            .expect("project reopened");
        let loaded = &reopened
            .current
            .as_ref()
            .expect("project is open")
            .properties;
        assert_eq!(loaded.cpp.standard, "17");
        assert_eq!(loaded.cpp.include_directories, ["vendor/include"]);
        assert!(!reopened.directories.contains(&PathBuf::from(".fugh")));
        fs::remove_dir_all(parent).expect("temporary project removed");
    }

    #[test]
    fn csharp_project_properties_add_portable_package_and_assembly_references() {
        let parent = temp_directory();
        let mut workspace = Workspace::default();
        let project = workspace
            .create_project(
                &parent,
                "DotnetProperties",
                ProjectLanguage::CSharp,
                ProjectTemplate::Console,
                "net10.0",
            )
            .expect("C# project created");
        let assembly = project.path.join("vendor/Managed.Helper.dll");
        fs::create_dir_all(assembly.parent().expect("assembly parent"))
            .expect("assembly directory created");
        fs::write(&assembly, b"test placeholder").expect("placeholder assembly written");
        let current = workspace.current.as_mut().expect("project is open");
        current
            .properties
            .csharp
            .nuget_packages
            .push(NuGetPackageReference {
                id: "Example.Package".to_owned(),
                version: "2.4.1".to_owned(),
            });
        current
            .properties
            .csharp
            .assembly_references
            .push("vendor/Managed.Helper.dll".to_owned());
        workspace
            .save_current_project_properties()
            .expect("C# project properties saved");

        let project_file = project.path.join("DotnetProperties.csproj");
        let project_xml = fs::read_to_string(&project_file).expect("project XML read");
        assert!(project_xml.contains(".fugh/project-properties.targets"));
        let targets = fs::read_to_string(project.path.join(".fugh/project-properties.targets"))
            .expect("generated MSBuild targets read");
        assert!(targets.contains("Include=\"Example.Package\" Version=\"2.4.1\""));
        assert!(targets.contains("vendor/Managed.Helper.dll"));

        let mut reopened = Workspace::default();
        reopened
            .open_project(&project.path)
            .expect("project reopened");
        assert_eq!(
            reopened
                .current
                .as_ref()
                .unwrap()
                .properties
                .csharp
                .nuget_packages[0]
                .id,
            "Example.Package"
        );
        fs::remove_dir_all(parent).expect("temporary project removed");
    }

    #[test]
    fn validates_folder_names_before_creation() {
        assert!(validate_project_name("Demo_2").is_ok());
        assert!(validate_project_name("2Demo").is_err());
        assert!(validate_project_name("../Demo").is_err());
        assert!(validate_project_name("").is_err());
    }

    #[test]
    fn saving_a_scratch_file_as_assigns_its_real_file_path() {
        let directory = temp_directory();
        let destination = directory.join("notes.cs");
        let mut workspace = Workspace::default();
        let mut scratch = Document::scratch();
        scratch.text = "class Notes {}\n".to_owned();
        scratch.dirty = true;
        workspace.documents = vec![scratch];

        workspace
            .save_document_as_with_line_ending(0, &destination, crate::settings::LineEnding::Lf)
            .expect("scratch file saved as a normal file");

        assert_eq!(
            workspace.documents[0].path.as_deref(),
            Some(destination.as_path())
        );
        assert_eq!(workspace.documents[0].title, "notes.cs");
        assert!(!workspace.documents[0].dirty);
        assert_eq!(
            fs::read_to_string(&destination).unwrap(),
            "class Notes {}\n"
        );
        fs::remove_dir_all(directory).expect("temporary directory removed");
    }

    #[test]
    fn validates_new_item_names_and_adds_the_expected_extension() {
        assert_eq!(
            validate_source_name("CustomerService", NewSourceKind::CSharpClass).unwrap(),
            "CustomerService.cs"
        );
        assert_eq!(
            validate_source_name("CustomerService.cs", NewSourceKind::CSharpClass).unwrap(),
            "CustomerService.cs"
        );
        assert!(validate_source_name("../outside", NewSourceKind::CSharpClass).is_err());
        assert!(validate_source_name("bad-name", NewSourceKind::CSharpClass).is_err());
        assert!(validate_source_name("File.cs", NewSourceKind::CppSource).is_err());
    }

    #[test]
    fn validates_folder_names_without_rejecting_spaces() {
        assert_eq!(
            super::validate_folder_name("UI Components").unwrap(),
            "UI Components"
        );
        assert!(super::validate_folder_name("../outside").is_err());
        assert!(super::validate_folder_name("bad/name").is_err());
        assert!(super::validate_folder_name("..").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn normalizes_extended_windows_paths_for_toolchains() {
        assert_eq!(
            normalize_windows_path(PathBuf::from(r"\\?\C:\work\project")),
            PathBuf::from(r"C:\work\project")
        );
        assert_eq!(
            normalize_windows_path(PathBuf::from(r"\\?\UNC\server\share\project")),
            PathBuf::from(r"\\server\share\project")
        );
    }

    #[test]
    fn generates_buildable_cpp_console_template() {
        let files = render_template(
            "Starter",
            ProjectLanguage::Cpp,
            ProjectTemplate::Console,
            "net10.0",
            false,
        )
        .expect("C++ console template");
        assert!(files.iter().any(|(path, _)| path == "CMakeLists.txt"));
        assert!(files.iter().any(|(path, code)| {
            path == &PathBuf::from("src/main.cpp") && code.contains("int main()")
        }));
    }

    #[test]
    fn generates_and_reopens_a_distinct_cpp_cmake_starter() {
        let root = temp_directory();
        let mut workspace = Workspace::default();
        workspace
            .create_project(
                &root,
                "CMakeStarter",
                ProjectLanguage::Cpp,
                ProjectTemplate::CMakeProject,
                "net8.0",
            )
            .expect("CMake starter created");
        let cmake = fs::read_to_string(root.join("CMakeStarter/CMakeLists.txt"))
            .expect("CMake configuration written");
        assert!(cmake.contains("# Fugh template: cmake-project"));
        assert!(cmake.contains("CMAKE_EXPORT_COMPILE_COMMANDS ON"));
        workspace
            .open_project(&root.join("CMakeStarter"))
            .expect("CMake starter reopened");
        assert_eq!(
            workspace.current.as_ref().map(|project| project.template),
            Some(ProjectTemplate::CMakeProject)
        );
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn generates_and_reopens_cpp_class_library() {
        let root = temp_directory();
        let mut workspace = Workspace::default();
        let project = workspace
            .create_project(
                &root,
                "UtilityLibrary",
                ProjectLanguage::Cpp,
                ProjectTemplate::ClassLibrary,
                "net10.0",
            )
            .expect("C++ class library created");
        assert_eq!(project.entry_file, PathBuf::from("src/library.cpp"));
        let cmake = fs::read_to_string(root.join("UtilityLibrary/CMakeLists.txt"))
            .expect("CMake configuration written");
        assert!(cmake.contains("add_library(UtilityLibrary src/library.cpp)"));
        assert!(
            root.join("UtilityLibrary/include/UtilityLibrary/library.hpp")
                .is_file()
        );
        workspace
            .open_project(&root.join("UtilityLibrary"))
            .expect("C++ class library reopened");
        assert_eq!(
            workspace.current.as_ref().map(|project| project.template),
            Some(ProjectTemplate::ClassLibrary)
        );
        fs::remove_dir_all(root).expect("temporary library removed");
    }

    #[test]
    fn new_qt_widgets_projects_are_disabled() {
        let root = temp_directory();
        let mut workspace = Workspace::default();
        let result = workspace.create_project(
            &root,
            "QtStarter",
            ProjectLanguage::Cpp,
            ProjectTemplate::QtWidgetsApp,
            "net10.0",
        );
        assert!(result.is_err());
        assert!(!root.join("QtStarter").exists());
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn generates_csharp_console_and_library_templates() {
        for template in [ProjectTemplate::Console, ProjectTemplate::ClassLibrary] {
            let files = render_template(
                "Starter",
                ProjectLanguage::CSharp,
                template,
                "net10.0",
                false,
            )
            .expect("C# template");
            assert!(
                files.iter().any(
                    |(path, contents)| path == "Starter.csproj" && contents.contains("net10.0")
                )
            );
            if template == ProjectTemplate::Console {
                assert!(files.iter().any(|(path, contents)| {
                    path == "Starter.csproj" && contents.contains("<OutputType>Exe</OutputType>")
                }));
            } else {
                assert!(files.iter().any(|(path, contents)| {
                    path == "Starter.csproj" && !contents.contains("<OutputType>")
                }));
            }
        }
    }

    #[test]
    fn reopening_class_library_preserves_its_project_type() {
        let root = temp_directory();
        let mut workspace = Workspace::default();
        workspace
            .create_project(
                &root,
                "LibraryStarter",
                ProjectLanguage::CSharp,
                ProjectTemplate::ClassLibrary,
                "net9.0",
            )
            .expect("class library created");
        workspace
            .open_project(&root.join("LibraryStarter"))
            .expect("class library reopened");
        assert_eq!(
            workspace.current.as_ref().map(|project| project.template),
            Some(ProjectTemplate::ClassLibrary)
        );
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn reports_invalid_project_files_clearly() {
        let root = temp_directory();
        let bad_project = root.join("BadProject.csproj");
        fs::write(&bad_project, "<Project><PropertyGroup>")
            .expect("invalid project fixture written");
        let error = Workspace::default()
            .open_project(&bad_project)
            .expect_err("malformed project rejected");
        assert!(error.contains("not valid XML"));
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn console_template_can_start_minimal_or_with_a_class_and_main() {
        let minimal = render_template(
            "Starter",
            ProjectLanguage::CSharp,
            ProjectTemplate::Console,
            "net10.0",
            false,
        )
        .expect("minimal C# template");
        let scaffold = render_template(
            "Starter",
            ProjectLanguage::CSharp,
            ProjectTemplate::Console,
            "net10.0",
            true,
        )
        .expect("scaffolded C# template");
        assert!(
            minimal
                .iter()
                .any(|(path, code)| path == "Program.cs" && code.starts_with("Console.WriteLine"))
        );
        assert!(scaffold.iter().any(|(path, code)| {
            path == "Program.cs" && code.contains("class Program") && code.contains("Main")
        }));
    }

    #[test]
    fn creates_and_opens_new_source_items_inside_the_project() {
        let root = temp_directory();
        let mut workspace = Workspace::default();
        workspace
            .create_project(
                &root,
                "Starter",
                ProjectLanguage::CSharp,
                ProjectTemplate::Console,
                "net10.0",
            )
            .expect("C# project created");
        workspace
            .create_source_file(
                PathBuf::new().as_path(),
                super::NewSourceKind::CSharpClass,
                "Customer",
            )
            .expect("new C# file created");
        assert!(root.join("Starter/Customer.cs").is_file());
        assert_eq!(
            workspace
                .active_document()
                .map(|document| document.title.as_str()),
            Some("Customer.cs")
        );
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn indexes_project_types_and_functions_including_unsaved_edits() {
        let root = temp_directory();
        let mut workspace = Workspace::default();
        workspace
            .create_project(
                &root,
                "Starter",
                ProjectLanguage::CSharp,
                ProjectTemplate::Console,
                "net10.0",
            )
            .expect("C# project created");
        workspace
            .create_source_file(
                PathBuf::new().as_path(),
                NewSourceKind::CSharpClass,
                "CustomerService",
            )
            .expect("C# class created");
        assert!(workspace.symbols.iter().any(|symbol| {
            symbol.name == "CustomerService"
                && symbol.kind == "class"
                && symbol.namespace.as_deref() == Some("Starter")
        }));
        let document = workspace
            .active_document_mut()
            .expect("created class is open");
        document
            .text
            .push_str("    public void SaveCustomer() { }\n");
        workspace.refresh_symbols().expect("unsaved source indexed");
        assert!(
            workspace
                .symbols
                .iter()
                .any(|symbol| { symbol.name == "SaveCustomer" && symbol.kind == "C# function" })
        );
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn extracts_cpp_symbols_declared_in_headers() {
        let symbols = super::extract_project_symbols(
            "class Widget {};\nstruct Point {};\nvoid draw_widget();\n",
            PathBuf::from("include/widgets.hpp").as_path(),
            "cpp",
        );
        assert!(symbols.iter().any(|symbol| symbol.name == "Widget"));
        assert!(
            symbols
                .iter()
                .any(|symbol| symbol.name == "draw_widget" && symbol.line == 3)
        );
        assert!(symbols.iter().any(|symbol| symbol.name == "Point"));
        assert!(
            symbols
                .iter()
                .any(|symbol| { symbol.name == "draw_widget" && symbol.kind == "C++ function" })
        );
    }

    #[test]
    fn creates_folders_and_deletes_project_items_and_open_documents() {
        let root = temp_directory();
        let mut workspace = Workspace::default();
        workspace
            .create_project(
                &root,
                "Starter",
                ProjectLanguage::CSharp,
                ProjectTemplate::Console,
                "net10.0",
            )
            .expect("C# project created");
        let outside_file = root.join("outside.txt");
        fs::write(&outside_file, "keep").expect("outside file created");
        assert!(
            workspace
                .delete_project_item(PathBuf::from("../outside.txt").as_path())
                .is_err()
        );
        assert!(outside_file.is_file());
        workspace
            .create_folder(PathBuf::new().as_path(), "Models")
            .expect("folder created");
        assert!(root.join("Starter/Models").is_dir());
        assert!(workspace.directories.contains(&PathBuf::from("Models")));

        workspace
            .create_source_file(
                PathBuf::from("Models").as_path(),
                NewSourceKind::CSharpClass,
                "Customer",
            )
            .expect("file created in the new folder");
        assert!(root.join("Starter/Models/Customer.cs").is_file());
        assert!(
            workspace
                .active_document()
                .is_some_and(|document| document.title == "Models/Customer.cs")
        );
        workspace
            .delete_project_item(PathBuf::from("Models").as_path())
            .expect("folder and its content deleted");
        assert!(!root.join("Starter/Models").exists());
        assert!(
            !workspace
                .files
                .contains(&PathBuf::from("Models/Customer.cs"))
        );
        assert!(
            workspace
                .documents
                .iter()
                .all(|document| document.title != "Models/Customer.cs")
        );
        assert!(workspace.directories.is_empty());
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn creates_common_csharp_items_with_matching_type_declarations() {
        let root = temp_directory();
        let mut workspace = Workspace::default();
        workspace
            .create_project(
                &root,
                "Sample_App",
                ProjectLanguage::CSharp,
                ProjectTemplate::Console,
                "net10.0",
            )
            .expect("C# project created");

        for (kind, file, declaration) in [
            (
                super::NewSourceKind::CSharpClass,
                "NewClass.cs",
                "public class NewClass",
            ),
            (
                super::NewSourceKind::CSharpInterface,
                "INewService.cs",
                "public interface INewService",
            ),
            (
                super::NewSourceKind::CSharpEnum,
                "NewStatus.cs",
                "public enum NewStatus",
            ),
            (
                super::NewSourceKind::CSharpRecord,
                "NewRecord.cs",
                "public record NewRecord",
            ),
            (
                super::NewSourceKind::CSharpStruct,
                "NewStruct.cs",
                "public struct NewStruct",
            ),
            (
                super::NewSourceKind::CSharpStaticClass,
                "NewUtilities.cs",
                "public static class NewUtilities",
            ),
        ] {
            workspace
                .create_source_file(PathBuf::new().as_path(), kind, file.trim_end_matches(".cs"))
                .expect("C# item created");
            let source = fs::read_to_string(root.join("Sample_App").join(file))
                .expect("generated C# item can be read");
            assert!(source.contains(declaration));
            assert!(source.contains("namespace Sample_App;"));
        }
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn closing_a_document_keeps_the_selected_tab_valid() {
        let mut workspace = Workspace {
            documents: vec![
                super::Document {
                    path: None,
                    title: "one.cs".to_owned(),
                    text: String::new(),
                    dirty: false,
                    breakpoints: Vec::new(),
                    pinned: false,
                },
                super::Document {
                    path: None,
                    title: "two.cs".to_owned(),
                    text: String::new(),
                    dirty: false,
                    breakpoints: Vec::new(),
                    pinned: false,
                },
                super::Document {
                    path: None,
                    title: "three.cs".to_owned(),
                    text: String::new(),
                    dirty: false,
                    breakpoints: Vec::new(),
                    pinned: false,
                },
            ],
            selected_document: 2,
            ..Workspace::default()
        };
        workspace.close_document(0).expect("first tab closed");
        assert_eq!(workspace.selected_document, 1);
        assert_eq!(
            workspace
                .active_document()
                .map(|document| document.title.as_str()),
            Some("three.cs")
        );
        workspace.close_document(1).expect("selected tab closed");
        assert_eq!(workspace.selected_document, 0);
        assert_eq!(
            workspace
                .active_document()
                .map(|document| document.title.as_str()),
            Some("two.cs")
        );
        workspace.close_document(0).expect("last tab closed");
        assert!(workspace.documents.is_empty());
        assert_eq!(workspace.selected_document, 0);
    }

    #[test]
    fn recently_closed_document_can_be_reopened_with_its_buffer() {
        let mut workspace = Workspace::default();
        workspace.documents[0].title = "View.axaml".to_owned();
        workspace.documents[0].text = "<Window />".to_owned();
        workspace.close_document(0).expect("document closed");
        assert!(workspace.documents.is_empty());
        workspace
            .reopen_closed_document()
            .expect("closed document reopened");
        assert_eq!(workspace.active_document().unwrap().title, "View.axaml");
        assert_eq!(workspace.active_document().unwrap().text, "<Window />");
    }

    #[test]
    fn new_cpp_source_is_registered_with_the_cmake_target() {
        let root = temp_directory();
        let mut workspace = Workspace::default();
        workspace
            .create_project(
                &root,
                "Starter",
                ProjectLanguage::Cpp,
                ProjectTemplate::Console,
                "net10.0",
            )
            .expect("C++ project created");
        workspace
            .create_source_file(
                PathBuf::new().as_path(),
                super::NewSourceKind::CppSource,
                "ExtraSource",
            )
            .expect("new C++ source created");
        let cmake = fs::read_to_string(root.join("Starter/CMakeLists.txt"))
            .expect("CMake configuration is readable");
        assert!(cmake.contains("target_sources(Starter PRIVATE \"ExtraSource.cpp\")"));
        fs::remove_dir_all(root).expect("temporary directory removed");
    }

    #[test]
    fn generates_cross_platform_avalonia_project_with_valid_xml_and_code_behind() {
        let files = render_template(
            "Starter",
            ProjectLanguage::CSharp,
            ProjectTemplate::AvaloniaApp,
            "net10.0",
            false,
        )
        .expect("Avalonia template");
        assert!(files.iter().any(|(path, contents)| {
            path == "Starter.csproj"
                && contents.contains("Avalonia.Desktop")
                && contents.contains("net10.0")
                && contents.contains("<RollForward>Major</RollForward>")
        }));
        assert!(
            files
                .iter()
                .any(|(path, contents)| path == "MainWindow.axaml" && contents.contains("<Canvas"))
        );
        assert!(
            files.iter().any(
                |(path, contents)| path == "MainWindow.axaml.cs" && contents.contains("Window")
            )
        );
        for (path, contents) in &files {
            if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("csproj" | "axaml")
            ) {
                roxmltree::Document::parse(contents)
                    .unwrap_or_else(|error| panic!("invalid XML in {}: {error}", path.display()));
            }
        }
    }

    #[test]
    fn opens_an_avalonia_project_with_its_window_as_the_entry_document() {
        let root = temp_directory();
        let project_path = root.join("Starter");
        fs::create_dir_all(&project_path).expect("project directory created");
        for (relative, contents) in render_template(
            "Starter",
            ProjectLanguage::CSharp,
            ProjectTemplate::AvaloniaApp,
            "net10.0",
            false,
        )
        .expect("Avalonia template")
        {
            let path = project_path.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("template subfolder created");
            }
            fs::write(path, contents).expect("template file written");
        }
        let mut workspace = Workspace::default();
        workspace
            .open_project(&project_path)
            .expect("Avalonia project opened");
        assert_eq!(
            workspace.current.as_ref().map(|project| project.template),
            Some(ProjectTemplate::AvaloniaApp)
        );
        assert_eq!(
            workspace
                .active_document()
                .and_then(|document| document.path.as_deref())
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str()),
            Some("MainWindow.axaml")
        );
        fs::remove_dir_all(root).expect("temporary project removed");
    }

    #[test]
    fn creates_a_project_and_rejects_existing_destination() {
        let root = temp_directory();
        let mut workspace = Workspace::default();

        let project = workspace
            .create_project(
                &root,
                "Starter",
                ProjectLanguage::Cpp,
                ProjectTemplate::Console,
                "net10.0",
            )
            .expect("project created");
        assert!(project.path.join("CMakeLists.txt").is_file());
        assert!(project.path.join("src/main.cpp").is_file());
        assert_eq!(workspace.current.as_ref(), Some(&project));
        assert!(
            workspace
                .create_project(
                    &root,
                    "Starter",
                    ProjectLanguage::Cpp,
                    ProjectTemplate::Console,
                    "net10.0",
                )
                .is_err()
        );

        fs::remove_dir_all(root).expect("test directory cleaned");
    }

    #[test]
    fn opens_a_dotnet_solution_file() {
        let root = temp_directory();
        let project_dir = root.join("Starter");
        fs::create_dir_all(&project_dir).expect("solution project folder created");
        fs::write(
            project_dir.join("Starter.csproj"),
            "<Project><PropertyGroup><OutputType>Exe</OutputType></PropertyGroup></Project>",
        )
        .expect("project file written");
        fs::write(
            project_dir.join("Program.cs"),
            "Console.WriteLine(\"Hello\");\n",
        )
        .expect("source file written");
        fs::write(
            root.join("Starter.sln"),
            "Project(\"{GUID}\") = \"Starter\", \"Starter\\Starter.csproj\", \"{GUID}\"\nEndProject\n",
        )
        .expect("solution file written");

        let mut workspace = Workspace::default();
        workspace
            .open_project(&root.join("Starter.sln"))
            .expect("solution imported");
        assert_eq!(
            workspace.current.as_ref().map(|project| &project.name),
            Some(&"Starter".to_owned())
        );
        assert_eq!(workspace.recent.len(), 1);

        fs::remove_dir_all(root).expect("test directory cleaned");
    }

    #[test]
    fn imports_visual_studio_cpp_projects_and_solutions_for_editing() {
        let root = temp_directory();
        let project_dir = root.join("NativeApp");
        fs::create_dir_all(project_dir.join("src")).expect("project source folder created");
        fs::write(
            project_dir.join("NativeApp.vcxproj"),
            "<Project xmlns=\"http://schemas.microsoft.com/developer/msbuild/2003\"><ItemGroup><ClCompile Include=\"src\\main.cpp\" /></ItemGroup></Project>",
        )
        .expect("Visual Studio project written");
        fs::write(
            project_dir.join("src/main.cpp"),
            "int main() { return 0; }\n",
        )
        .expect("C++ source written");
        fs::write(
            root.join("NativeApp.sln"),
            "Project(\"{GUID}\") = \"NativeApp\", \"NativeApp\\NativeApp.vcxproj\", \"{GUID}\"\nEndProject\n",
        )
        .expect("Visual Studio solution written");

        let mut workspace = Workspace::default();
        workspace
            .open_project(&root.join("NativeApp.sln"))
            .expect("Visual Studio solution imported");
        let project = workspace.current.as_ref().expect("imported project");
        assert_eq!(project.template, ProjectTemplate::VisualStudioCpp);
        assert_eq!(project.entry_file, PathBuf::from("src/main.cpp"));
        assert_eq!(workspace.files, vec![PathBuf::from("src/main.cpp")]);

        fs::remove_dir_all(root).expect("test directory cleaned");
    }

    #[test]
    fn validates_project_destination_and_warns_on_existing_folder() {
        let root = temp_directory();
        assert!(super::validate_project_destination(&root, "NewProject").is_ok());
        fs::create_dir(root.join("Existing")).expect("existing folder created");
        let error = super::validate_project_destination(&root, "Existing")
            .expect_err("existing destination rejected");
        assert!(error.contains("already exists"));
        assert!(super::validate_project_destination(&root.join("missing"), "Valid").is_err());

        fs::remove_dir_all(root).expect("test directory cleaned");
    }
}
