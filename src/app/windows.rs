use std::time::{Duration, Instant};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
};

use eframe::egui::{self, Color32, RichText, TextStyle};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};

use crate::{
    app::layout::{DockItem, DockSide, EditorLayout, SidebarTab},
    process::{ProcessAction, ProcessHandle},
    project::{
        CSharpProjectProperties, CppProjectProperties, NewSourceKind, NuGetPackageReference,
        ProjectInfo, ProjectLanguage, ProjectProperties, ProjectTemplate, Workspace,
    },
    theme,
    toolchain::{PackageAction, ToolchainManager},
};

use super::designer::WindowDesigner;

pub(super) struct WindowState {
    toolchains_open: Arc<AtomicBool>,
    wizard_open: Arc<AtomicBool>,
    project_form: Arc<Mutex<ProjectForm>>,
    workspace: Arc<Mutex<Workspace>>,
    editor_layout: Arc<Mutex<EditorLayout>>,
    designer: Mutex<WindowDesigner>,
    package_confirmation: Arc<Mutex<Option<PackageAction>>>,
    toolchains: ToolchainManager,
    language_server: Mutex<crate::language_server::LanguageServerClient>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectPropertiesTab {
    Cpp,
    CSharp,
}

#[derive(Clone)]
struct ProjectPropertiesDraft {
    project_path: std::path::PathBuf,
    properties: ProjectProperties,
    tab: ProjectPropertiesTab,
    open: bool,
    error: Option<String>,
}

struct FormatJob {
    path: std::path::PathBuf,
    root: std::path::PathBuf,
    text: String,
    settings: crate::settings::FormattingSettings,
    line_ending: crate::settings::LineEnding,
    save_after_format: bool,
}

struct FormatResult {
    path: std::path::PathBuf,
    text: String,
    line_ending: crate::settings::LineEnding,
    save_after_format: bool,
    result: Result<String, String>,
}

#[derive(Clone)]
struct HoverDebounce {
    path: std::path::PathBuf,
    line: usize,
    token_start: usize,
    since: Instant,
    requested: bool,
}

#[derive(Clone)]
struct ReferenceLensEntry {
    path: std::path::PathBuf,
    line: usize,
    column: usize,
    protocol_column: usize,
    name: String,
    count: usize,
}

#[derive(Clone)]
struct PendingReferenceLens {
    id: u64,
    path: std::path::PathBuf,
    line: usize,
    column: usize,
    protocol_column: usize,
    name: String,
}

#[derive(Clone, Default)]
struct ReferenceLensState {
    root: Option<std::path::PathBuf>,
    document_path: Option<std::path::PathBuf>,
    source_stamp: u64,
    checked_at: Option<Instant>,
    changed_at: Option<Instant>,
    needs_refresh: bool,
    requested: Vec<(std::path::PathBuf, usize, String)>,
    retried_empty: Vec<(std::path::PathBuf, usize, String)>,
    pending: Vec<PendingReferenceLens>,
    entries: Vec<ReferenceLensEntry>,
}

#[derive(Clone)]
struct ReferenceLensEditorMapping {
    path: Option<std::path::PathBuf>,
    source_text: String,
    removed_display_indices: Vec<usize>,
    display_to_source: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FoldRange {
    start_line: usize,
    end_line: usize,
}

struct EditorDisplayBuffer {
    text: String,
    display_to_source: Vec<usize>,
}

#[derive(Clone)]
struct DocumentSymbolsRequestTracker {
    path: std::path::PathBuf,
    text: String,
    since: Instant,
    requested: bool,
}

#[derive(Clone)]
struct DocumentSymbolsCache {
    path: std::path::PathBuf,
    text: String,
    symbols: Vec<crate::language_server::DocumentSymbolEntry>,
}

const REFERENCE_LENS_ROW_MARKER: char = '\u{200B}';
const FOLD_ROW_MARKER: char = '\u{E000}';

fn editor_display_buffer(
    source: &str,
    entries: &[ReferenceLensEntry],
    path: Option<&std::path::Path>,
    folds: &[FoldRange],
) -> EditorDisplayBuffer {
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    let mut text = String::with_capacity(source.len() + entries.len() * 2 + folds.len() * 24);
    let mut display_to_source = vec![0];
    let mut source_index = 0usize;
    let mut line_index = 1usize;
    while line_index <= lines.len() {
        let line = lines[line_index - 1];
        if path.is_some_and(|path| {
            entries
                .iter()
                .any(|entry| entry.path == path && entry.line == line_index)
        }) {
            text.push(REFERENCE_LENS_ROW_MARKER);
            display_to_source.push(source_index);
            text.push('\n');
            display_to_source.push(source_index);
        }
        for character in line.chars() {
            text.push(character);
            source_index += 1;
            display_to_source.push(source_index);
        }
        if let Some(fold) = folds.iter().find(|fold| fold.start_line == line_index)
            && fold.end_line > fold.start_line
            && fold.end_line <= lines.len()
        {
            let hidden_lines = fold.end_line - fold.start_line;
            let hidden_start = source_index;
            let hidden_characters = lines[fold.start_line..fold.end_line]
                .iter()
                .map(|line| line.chars().count())
                .sum::<usize>();
            let hidden_end = hidden_start.saturating_add(hidden_characters);
            let placeholder = format!("{FOLD_ROW_MARKER}… {hidden_lines} lines folded …\n");
            let placeholder_length = placeholder.chars().count();
            for (index, character) in placeholder.chars().enumerate() {
                text.push(character);
                display_to_source.push(if index + 1 == placeholder_length {
                    hidden_end
                } else {
                    hidden_start
                });
            }
            source_index = hidden_end;
            line_index = fold.end_line;
        }
        line_index += 1;
    }
    EditorDisplayBuffer {
        text,
        display_to_source,
    }
}

fn source_index_from_display(index: usize, display_to_source: &[usize]) -> usize {
    display_to_source
        .get(index.min(display_to_source.len().saturating_sub(1)))
        .copied()
        .unwrap_or_default()
}

fn display_index_from_source(index: usize, display_to_source: &[usize]) -> usize {
    display_to_source
        .iter()
        .position(|source_index| *source_index >= index)
        .unwrap_or(display_to_source.len().saturating_sub(1))
}

fn apply_editor_display_edit(
    source: &str,
    old_display: &str,
    new_display: &str,
    display_to_source: &[usize],
) -> String {
    let old = old_display.chars().collect::<Vec<_>>();
    let new = new_display.chars().collect::<Vec<_>>();
    let mut prefix = 0usize;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old.len().saturating_sub(prefix)
        && suffix < new.len().saturating_sub(prefix)
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let source_start = source_index_from_display(prefix, display_to_source);
    let source_end = source_index_from_display(old.len().saturating_sub(suffix), display_to_source)
        .max(source_start);
    let source_chars = source.chars().collect::<Vec<_>>();
    let byte_at = |index: usize| {
        source_chars
            .iter()
            .take(index.min(source_chars.len()))
            .map(|character| character.len_utf8())
            .sum::<usize>()
    };
    let start_byte = byte_at(source_start);
    let end_byte = byte_at(source_end);
    let inserted = new[prefix..new.len().saturating_sub(suffix)]
        .iter()
        .collect::<String>();
    let mut updated = String::with_capacity(source.len() + inserted.len());
    updated.push_str(&source[..start_byte]);
    updated.push_str(&inserted);
    updated.push_str(&source[end_byte..]);
    updated
}

fn editor_mapping_after_edit(
    old_display: &str,
    new_display: &str,
    old_mapping: &[usize],
) -> Vec<usize> {
    let old = old_display.chars().collect::<Vec<_>>();
    let new = new_display.chars().collect::<Vec<_>>();
    let mut prefix = 0usize;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old.len().saturating_sub(prefix)
        && suffix < new.len().saturating_sub(prefix)
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let old_end = old.len().saturating_sub(suffix);
    let new_end = new.len().saturating_sub(suffix);
    let source_start = source_index_from_display(prefix, old_mapping);
    let source_end = source_index_from_display(old_end, old_mapping).max(source_start);
    let source_delta = new_end.saturating_sub(prefix) as isize
        - (source_end.saturating_sub(source_start) as isize);
    let display_delta = new_end as isize - old_end as isize;
    let mut mapping = Vec::with_capacity(new.len() + 1);
    for index in 0..=new.len() {
        let source_index = if index <= prefix {
            source_index_from_display(index, old_mapping)
        } else if index <= new_end {
            let inserted_offset = index - prefix;
            if source_end == source_start {
                source_start.saturating_add(inserted_offset)
            } else {
                source_start
                    .saturating_add(inserted_offset.min(source_end.saturating_sub(source_start)))
            }
        } else {
            let old_index = (index as isize - display_delta).max(0) as usize;
            let mapped = source_index_from_display(old_index, old_mapping) as isize + source_delta;
            mapped.max(0) as usize
        };
        mapping.push(source_index);
    }
    mapping
}

fn apply_multi_cursor_edit(
    old_source: &str,
    source_after_primary: &str,
    cursors: &[usize],
    primary_cursor_before: Option<usize>,
    primary_cursor_after: usize,
) -> (String, Vec<usize>, usize) {
    if old_source == source_after_primary || cursors.is_empty() {
        return (
            source_after_primary.to_owned(),
            cursors.to_vec(),
            primary_cursor_after,
        );
    }
    let old = old_source.chars().collect::<Vec<_>>();
    let new = source_after_primary.chars().collect::<Vec<_>>();
    let mut prefix = 0usize;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old.len().saturating_sub(prefix)
        && suffix < new.len().saturating_sub(prefix)
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let old_end = old.len().saturating_sub(suffix);
    let new_end = new.len().saturating_sub(suffix);
    let removed = old_end.saturating_sub(prefix);
    let inserted = new[prefix..new_end].to_vec();
    let delta = inserted.len() as isize - removed as isize;
    let forward_delete =
        removed > 0 && inserted.is_empty() && primary_cursor_before == Some(prefix);
    let mut adjusted = cursors
        .iter()
        .copied()
        .filter(|cursor| *cursor < prefix || *cursor > old_end)
        .map(|cursor| {
            if cursor > old_end {
                (cursor as isize + delta).max(0) as usize
            } else {
                cursor.min(new.len())
            }
        })
        .collect::<Vec<_>>();
    adjusted.sort_unstable();
    adjusted.dedup();
    let mut positions = adjusted
        .into_iter()
        .filter(|cursor| *cursor <= new.len())
        .collect::<Vec<_>>();
    let mut edits = positions
        .iter()
        .copied()
        .map(|cursor| {
            let (start, end) = if forward_delete {
                (cursor, cursor.saturating_add(removed).min(new.len()))
            } else {
                (cursor.saturating_sub(removed), cursor)
            };
            (start, end, inserted.clone())
        })
        .collect::<Vec<_>>();
    edits.sort_by_key(|(start, end, _)| std::cmp::Reverse((*start, *end)));
    let mut updated = source_after_primary.to_owned();
    for (start, end, replacement) in &edits {
        let characters = updated.chars().collect::<Vec<_>>();
        let byte_at = |index: usize| {
            characters
                .iter()
                .take(index.min(characters.len()))
                .map(|character| character.len_utf8())
                .sum::<usize>()
        };
        let start_byte = byte_at(*start);
        let end_byte = byte_at(*end);
        let mut replacement_text = String::new();
        replacement_text.extend(replacement);
        updated.replace_range(start_byte..end_byte, &replacement_text);
    }
    let insert_count = inserted.len();
    for cursor in &mut positions {
        let own_start = if forward_delete {
            *cursor
        } else {
            cursor.saturating_sub(removed)
        };
        let mut new_cursor = own_start.saturating_add(insert_count);
        for (start, end, replacement) in &edits {
            if *start < own_start {
                new_cursor = (new_cursor as isize + replacement.len() as isize
                    - end.saturating_sub(*start) as isize)
                    .max(0) as usize;
            }
        }
        *cursor = new_cursor;
    }
    let mut updated_primary = primary_cursor_after;
    for (start, end, replacement) in &edits {
        if *start < primary_cursor_after {
            updated_primary = (updated_primary as isize + replacement.len() as isize
                - end.saturating_sub(*start) as isize)
                .max(0) as usize;
        }
    }
    (updated, positions, updated_primary)
}

fn foldable_ranges(source: &str) -> Vec<FoldRange> {
    let characters = source.chars().collect::<Vec<_>>();
    let mut ranges = Vec::new();
    let mut openings = Vec::<usize>::new();
    let mut line = 1usize;
    let mut index = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut block_comment = false;
    while index < characters.len() {
        let character = characters[index];
        let next = characters.get(index + 1).copied();
        if character == '\n' {
            line += 1;
            if quote.is_some() && !escaped {
                quote = None;
            }
            escaped = false;
            index += 1;
            continue;
        }
        if block_comment {
            if character == '*' && next == Some('/') {
                block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
            index += 1;
            continue;
        }
        if character == '/' && next == Some('/') {
            while index < characters.len() && characters[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if character == '/' && next == Some('*') {
            block_comment = true;
            index += 2;
            continue;
        }
        if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if character == '{' {
            openings.push(line);
        } else if character == '}'
            && let Some(start_line) = openings.pop()
            && line > start_line
        {
            ranges.push(FoldRange {
                start_line,
                end_line: line,
            });
        }
        index += 1;
    }
    ranges.sort_by_key(|range| (range.start_line, std::cmp::Reverse(range.end_line)));
    ranges.dedup();
    ranges
}

fn adjust_fold_ranges_after_edit(old: &str, new: &str, folds: &[FoldRange]) -> Vec<FoldRange> {
    if old == new {
        return folds.to_vec();
    }
    let old_chars = old.chars().collect::<Vec<_>>();
    let new_chars = new.chars().collect::<Vec<_>>();
    let mut prefix = 0usize;
    while prefix < old_chars.len()
        && prefix < new_chars.len()
        && old_chars[prefix] == new_chars[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old_chars.len().saturating_sub(prefix)
        && suffix < new_chars.len().saturating_sub(prefix)
        && old_chars[old_chars.len() - 1 - suffix] == new_chars[new_chars.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let old_end = old_chars.len().saturating_sub(suffix);
    let new_end = new_chars.len().saturating_sub(suffix);
    let start_line = old_chars[..prefix].iter().filter(|ch| **ch == '\n').count() + 1;
    let end_line = old_chars[..old_end]
        .iter()
        .filter(|ch| **ch == '\n')
        .count()
        + 1;
    let removed_lines = old_chars[prefix..old_end]
        .iter()
        .filter(|ch| **ch == '\n')
        .count() as isize;
    let inserted_lines = new_chars[prefix..new_end]
        .iter()
        .filter(|ch| **ch == '\n')
        .count() as isize;
    let line_delta = inserted_lines - removed_lines;
    folds
        .iter()
        .filter_map(|fold| {
            if fold.end_line < start_line {
                Some(*fold)
            } else if fold.start_line > end_line
                || (old_end == prefix && fold.start_line >= start_line)
            {
                let start = fold.start_line as isize + line_delta;
                let end = fold.end_line as isize + line_delta;
                (start > 0 && end >= start).then_some(FoldRange {
                    start_line: start as usize,
                    end_line: end as usize,
                })
            } else {
                None
            }
        })
        .collect()
}

fn toggle_editor_fold(ctx: &egui::Context, path: &std::path::Path, range: FoldRange) {
    let id = egui::Id::unique("cairn.editor-fold-ranges");
    ctx.data_mut(|data| {
        let mut by_path = data
            .get_temp::<HashMap<std::path::PathBuf, Vec<FoldRange>>>(id)
            .unwrap_or_default();
        let folds = by_path.entry(path.to_path_buf()).or_default();
        if let Some(index) = folds.iter().position(|fold| *fold == range) {
            folds.remove(index);
        } else {
            folds.push(range);
            folds.sort_by_key(|fold| (fold.start_line, std::cmp::Reverse(fold.end_line)));
        }
        data.insert_temp(id, by_path);
    });
}

/// Removes the synthetic CodeLens rows from the editor's display buffer.
/// Returns character indexes that were removed so the editor cursor can be
/// mapped back to the actual source text.
fn source_text_without_reference_lens_rows(display: &str) -> (String, Vec<usize>) {
    let mut source = String::with_capacity(display.len());
    let mut removed = Vec::new();
    let mut character_index = 0;
    for line in display.split_inclusive('\n') {
        let content = line
            .strip_suffix('\n')
            .unwrap_or(line)
            .strip_suffix('\r')
            .unwrap_or_else(|| line.strip_suffix('\n').unwrap_or(line));
        let virtual_blank = content.chars().eq([REFERENCE_LENS_ROW_MARKER]);
        for (offset, character) in line.char_indices() {
            let index = character_index + line[..offset].chars().count();
            if character == REFERENCE_LENS_ROW_MARKER
                || (virtual_blank && matches!(character, '\r' | '\n'))
            {
                removed.push(index);
            } else {
                source.push(character);
            }
        }
        character_index += line.chars().count();
    }
    (source, removed)
}

fn source_cursor_index(display_index: usize, removed: &[usize]) -> usize {
    display_index.saturating_sub(removed.partition_point(|index| *index < display_index))
}

fn source_cursor_before_reference_lens(
    display: &str,
    source: &str,
    display_cursor: usize,
    entries: &[ReferenceLensEntry],
) -> Option<usize> {
    let display_chars = display.chars().collect::<Vec<_>>();
    let cursor = display_cursor.min(display_chars.len());
    let current_display_line = display_chars[..cursor]
        .iter()
        .filter(|character| **character == '\n')
        .count();
    if current_display_line == 0 {
        return None;
    }

    let rows = display.split_inclusive('\n').collect::<Vec<_>>();
    let marker_line_index = current_display_line - 1;
    let marker_line = rows.get(marker_line_index)?;
    let marker_content = marker_line
        .strip_suffix('\n')
        .unwrap_or(marker_line)
        .strip_suffix('\r')
        .unwrap_or_else(|| marker_line.strip_suffix('\n').unwrap_or(marker_line));
    if !marker_content.chars().eq([REFERENCE_LENS_ROW_MARKER]) {
        return None;
    }

    let source_line = rows[..marker_line_index]
        .iter()
        .filter(|row| {
            let content = row
                .strip_suffix('\n')
                .unwrap_or(row)
                .strip_suffix('\r')
                .unwrap_or_else(|| row.strip_suffix('\n').unwrap_or(row));
            !content.chars().eq([REFERENCE_LENS_ROW_MARKER])
        })
        .count()
        + 1;
    if !entries.iter().any(|entry| entry.line == source_line) || source_line <= 1 {
        return None;
    }

    let current_line_start = display_chars[..cursor]
        .iter()
        .rposition(|character| *character == '\n')
        .map_or(0, |index| index + 1);
    let column = cursor.saturating_sub(current_line_start);
    source_line_cursor(source, source_line - 2, column)
}

fn source_line_cursor(source: &str, line_index: usize, column: usize) -> Option<usize> {
    let mut offset = 0usize;
    for (index, line) in source.split_inclusive('\n').enumerate() {
        if index == line_index {
            let content = line
                .strip_suffix('\n')
                .unwrap_or(line)
                .strip_suffix('\r')
                .unwrap_or_else(|| line.strip_suffix('\n').unwrap_or(line));
            return Some(offset + column.min(content.chars().count()));
        }
        offset += line.chars().count();
    }
    if source.ends_with('\n') && line_index == source.matches('\n').count() {
        return Some(source.chars().count());
    }
    None
}

fn declaration_name_column(line: &str, name: &str) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    let mut first_identifier = None;
    for (byte_index, _) in line.match_indices(name) {
        let before = line[..byte_index].chars().next_back();
        let after_index = byte_index + name.len();
        let after = line[after_index..].chars().next();
        let is_identifier_character =
            |character: char| character == '_' || character.is_alphanumeric();
        if before.is_some_and(is_identifier_character) || after.is_some_and(is_identifier_character)
        {
            continue;
        }
        let column = line[..byte_index].chars().count();
        first_identifier.get_or_insert(column);
        let rest = line[after_index..].trim_start();
        if rest.starts_with('(') || rest.starts_with('<') {
            return Some(column);
        }
    }
    first_identifier
}

fn is_reference_usage(
    location: &crate::language_server::ReferenceLocation,
    path: &std::path::Path,
    line: usize,
    protocol_column: usize,
) -> bool {
    location.is_declaration != Some(true)
        && !(location.path == path
            && location.line == line
            && location.column == protocol_column.saturating_add(1))
}

#[derive(Clone)]
struct HoverPopupState {
    path: std::path::PathBuf,
    line: usize,
    token_start: usize,
    text: String,
    anchor: egui::Pos2,
    last_active: Instant,
}

#[derive(Clone)]
struct SignatureHelpInfo {
    label: String,
    documentation: Option<String>,
    parameters: Vec<SignatureParameter>,
    overloads: Vec<SignatureOverload>,
    active_signature: usize,
}

#[derive(Clone)]
struct SignatureOverload {
    label: String,
    documentation: Option<String>,
    parameters: Vec<SignatureParameter>,
}

#[derive(Clone)]
struct OverloadSelection {
    labels: Vec<String>,
    selected: usize,
}

#[derive(Clone)]
struct SignatureParameter {
    documentation: Option<String>,
    label_range: Option<std::ops::Range<usize>>,
}

#[derive(Clone)]
struct SignatureRequestTracker {
    path: std::path::PathBuf,
    line: usize,
    character: usize,
    active_parameter: usize,
    text_length: usize,
    since: Instant,
    requested: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CallHierarchyDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone)]
struct PendingCallHierarchy {
    id: u64,
    direction: CallHierarchyDirection,
    prepared_item: Option<serde_json::Value>,
    name: String,
}

#[derive(Clone)]
struct CallHierarchyResults {
    direction: CallHierarchyDirection,
    item: serde_json::Value,
    name: String,
    entries: Vec<crate::language_server::CallHierarchyEntry>,
}

#[derive(Clone)]
struct ReferenceResults {
    locations: Vec<crate::language_server::ReferenceLocation>,
    usage_count: Option<usize>,
}

struct FormatWorker {
    sender: Sender<FormatJob>,
    receiver: Mutex<Receiver<FormatResult>>,
    in_flight: Mutex<HashSet<std::path::PathBuf>>,
    queued: Mutex<HashMap<std::path::PathBuf, FormatJob>>,
}

static FORMAT_WORKER: OnceLock<FormatWorker> = OnceLock::new();

fn format_worker() -> &'static FormatWorker {
    FORMAT_WORKER.get_or_init(|| {
        let (job_sender, job_receiver) = mpsc::channel::<FormatJob>();
        let (result_sender, result_receiver) = mpsc::channel::<FormatResult>();
        std::thread::Builder::new()
            .name("fugh-format-on-save".to_owned())
            .spawn(move || {
                while let Ok(job) = job_receiver.recv() {
                    let result = crate::formatting::format_document(
                        &job.path,
                        &job.root,
                        &job.text,
                        &job.settings,
                    );
                    if result_sender
                        .send(FormatResult {
                            path: job.path,
                            text: job.text,
                            line_ending: job.line_ending,
                            save_after_format: job.save_after_format,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("format worker thread should start");
        FormatWorker {
            sender: job_sender,
            receiver: Mutex::new(result_receiver),
            in_flight: Mutex::new(HashSet::new()),
            queued: Mutex::new(HashMap::new()),
        }
    })
}

impl WindowState {
    pub(super) fn new(mut workspace: Workspace, editor_layout: EditorLayout) -> Self {
        workspace.normalize_paths();
        let default_location = std::env::current_dir()
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("Projects");
        Self {
            toolchains_open: Arc::new(AtomicBool::new(false)),
            wizard_open: Arc::new(AtomicBool::new(false)),
            project_form: Arc::new(Mutex::new(ProjectForm {
                location: default_location
                    .parent()
                    .unwrap_or(&default_location)
                    .display()
                    .to_string(),
                ..ProjectForm::default()
            })),
            workspace: Arc::new(Mutex::new(workspace)),
            editor_layout: Arc::new(Mutex::new(editor_layout)),
            designer: Mutex::new(WindowDesigner::default()),
            package_confirmation: Arc::new(Mutex::new(None)),
            toolchains: ToolchainManager::default(),
            language_server: Mutex::new(crate::language_server::LanguageServerClient::default()),
        }
    }

    pub(super) fn workspace_snapshot(&self) -> Workspace {
        self.workspace
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(super) fn workspace_lock(&self) -> std::sync::MutexGuard<'_, Workspace> {
        self.workspace
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn editor_layout_snapshot(&self) -> EditorLayout {
        self.editor_layout
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(super) fn set_editor_layout(&self, layout: EditorLayout) {
        *self
            .editor_layout
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = layout;
    }

    pub(super) fn toolchain_manager(&self) -> ToolchainManager {
        self.toolchains.clone()
    }

    pub(super) fn open_toolchains(&self) {
        self.toolchains_open.store(true, Ordering::Relaxed);
    }

    pub(super) fn open_project_wizard(&self) {
        self.wizard_open.store(true, Ordering::Relaxed);
    }

    /// Pre-fills the New Project destination (the user's default location).
    pub(super) fn set_project_location(&self, location: &str) {
        self.project_form
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .location = location.to_owned();
    }

    pub(super) fn open_project_path(&self, path: &std::path::Path) -> Result<(), String> {
        self.workspace
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .open_project(path)?;
        Ok(())
    }

    pub(super) fn close_project(&self) {
        self.workspace
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .close_project();
        *self
            .designer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            super::designer::WindowDesigner::default();
        self.language_server
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
    }

    pub(super) fn wizard_active(&self) -> bool {
        self.wizard_open.load(Ordering::Relaxed)
    }

    /// Draws the new-project page inside the main window; true when created.
    pub(super) fn draw_wizard(&self, ui: &mut egui::Ui) -> bool {
        draw_project_wizard(ui, &self.wizard_open, &self.project_form, &self.workspace)
    }

    /// Draws the whole IDE (menu, toolbar, panels, editor) inside the main window.
    pub(super) fn draw_editor(
        &self,
        ui: &mut egui::Ui,
        process: ProcessHandle,
    ) -> Option<EditorCmd> {
        let mut workspace = self
            .workspace
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut layout = self
            .editor_layout
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut designer = self
            .designer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut language_server = self
            .language_server
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        draw_editor_ui(
            ui,
            &mut workspace,
            &mut layout,
            &mut designer,
            &process,
            &mut language_server,
        )
    }

    pub(super) fn show(&self, ctx: &egui::Context) {
        poll_format_results(&mut self.workspace_lock());
        if FORMAT_WORKER.get().is_some_and(|worker| {
            !worker
                .in_flight
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
                || !worker
                    .queued
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_empty()
        }) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.toolchains_open.load(Ordering::Relaxed) {
            if self.toolchains.tools().is_empty() {
                self.toolchains.refresh(ctx.clone());
            }
            show_toolchains_window(
                ctx,
                self.toolchains_open.clone(),
                self.toolchains.clone(),
                self.package_confirmation.clone(),
            );
        }
    }
}

#[derive(Debug)]
struct ProjectForm {
    name: String,
    location: String,
    language: ProjectLanguage,
    template: ProjectTemplate,
    framework: String,
    csharp_scaffold: bool,
    error: Option<String>,
}

impl Default for ProjectForm {
    fn default() -> Self {
        Self {
            name: "Starter".to_owned(),
            location: String::new(),
            language: ProjectLanguage::Cpp,
            template: ProjectTemplate::Console,
            framework: "net10.0".to_owned(),
            csharp_scaffold: false,
            error: None,
        }
    }
}

fn build_toolbar_settings_ui(ui: &mut egui::Ui, workspace: &mut Workspace) {
    let changed = workspace.current.as_mut().is_some_and(|project| {
        let previous = project.properties.clone();
        let is_cpp = project.language == ProjectLanguage::Cpp;
        let (configuration, platform) = if is_cpp {
            (
                &mut project.properties.cpp.build_configuration,
                &mut project.properties.cpp.platform,
            )
        } else {
            (
                &mut project.properties.csharp.build_configuration,
                &mut project.properties.csharp.platform_target,
            )
        };

        let config_label = if configuration.trim().is_empty() {
            "Debug"
        } else {
            configuration.as_str()
        };
        egui::ComboBox::from_id_salt("build-configuration-selector")
            .width(112.0)
            .selected_text(config_label)
            .show_ui(ui, |ui| {
                let configurations: &[&str] = if is_cpp {
                    &["Debug", "Release", "RelWithDebInfo", "MinSizeRel"]
                } else {
                    &["Debug", "Release"]
                };
                for item in configurations {
                    ui.selectable_value(configuration, (*item).to_owned(), *item);
                }
                ui.separator();
                ui.label(RichText::new("Custom configuration").small().color(theme::muted()));
                ui.add(
                    egui::TextEdit::singleline(configuration)
                        .desired_width(150.0)
                        .hint_text("Type a configuration name"),
                );
            })
            .response
            .on_hover_text("Choose a build configuration or enter a custom configuration name.");

        let platform_label = if is_cpp {
            match platform.as_str() {
                "" | "Host" => "Host",
                "x86" | "X86" => "x86",
                "x64" | "X64" => "x64",
                other => other,
            }
        } else {
            match platform.as_str() {
                "" | "AnyCPU" | "AnyCpu" => "Any CPU",
                other => other,
            }
        };
        egui::ComboBox::from_id_salt("build-platform-selector")
            .width(100.0)
            .selected_text(platform_label)
            .show_ui(ui, |ui| {
                if is_cpp {
                    ui.selectable_value(platform, "Host".to_owned(), "Host");
                    ui.selectable_value(platform, "x64".to_owned(), "x64");
                    ui.selectable_value(platform, "x86".to_owned(), "x86");
                } else {
                    ui.selectable_value(platform, "AnyCPU".to_owned(), "Any CPU");
                    ui.selectable_value(platform, "x64".to_owned(), "x64");
                    ui.selectable_value(platform, "x86".to_owned(), "x86");
                    ui.selectable_value(platform, "ARM64".to_owned(), "ARM64");
                }
            })
            .response
            .on_hover_text(if is_cpp {
                "C++ native target architecture. x86/x64 use GCC or Clang architecture flags."
            } else {
                "C# compiler target architecture passed to MSBuild as PlatformTarget."
            });
        previous != project.properties
    });

    if changed && let Err(error) = workspace.save_current_build_settings() {
        workspace.message = error;
    }
}

fn project_action_button(
    ui: &mut egui::Ui,
    process: &ProcessHandle,
    workspace: &mut Workspace,
    action: ProcessAction,
) {
    let process_state = process.state();
    let (running, stopping, active_action) = {
        let state = process_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (state.running, state.stopping, state.active_action)
    };
    if running
        && active_action == Some(action)
        && matches!(action, ProcessAction::Run | ProcessAction::Debug)
    {
        if ui
            .add(
                egui::Button::new(
                    RichText::new(if stopping { "Stopping…" } else { "Stop  ■" }).strong(),
                )
                .fill(Color32::from_rgb(137, 50, 57))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(205, 95, 100))),
            )
            .clicked()
            && !stopping
        {
            process.cancel();
        }
        return;
    }
    let unavailable_reason = workspace
        .current
        .as_ref()
        .and_then(|project| unsupported_project_action(project, action));
    let enabled = workspace.current.is_some() && !running && unavailable_reason.is_none();
    let (label, fill, stroke) = match action {
        ProcessAction::Build => (
            "Build",
            Color32::from_rgb(44, 94, 138),
            Color32::from_rgb(79, 150, 207),
        ),
        ProcessAction::BuildRelease => (
            "Release",
            Color32::from_rgb(82, 94, 112),
            Color32::from_rgb(139, 150, 169),
        ),
        ProcessAction::Rebuild => (
            "Rebuild",
            Color32::from_rgb(82, 94, 112),
            Color32::from_rgb(139, 150, 169),
        ),
        ProcessAction::Clean => (
            "Clean",
            Color32::from_rgb(82, 78, 70),
            Color32::from_rgb(151, 141, 119),
        ),
        ProcessAction::Analyze => (
            "Analyze",
            Color32::from_rgb(84, 74, 115),
            Color32::from_rgb(145, 124, 195),
        ),
        ProcessAction::Publish => (
            "Publish",
            Color32::from_rgb(48, 105, 91),
            Color32::from_rgb(86, 165, 145),
        ),
        ProcessAction::Package => (
            "Package",
            Color32::from_rgb(91, 80, 53),
            Color32::from_rgb(174, 150, 91),
        ),
        ProcessAction::Run => (
            "Run  ▶",
            Color32::from_rgb(35, 112, 79),
            Color32::from_rgb(70, 171, 123),
        ),
        ProcessAction::Debug => (
            "Debug",
            Color32::from_rgb(88, 70, 132),
            Color32::from_rgb(139, 116, 193),
        ),
    };
    let mut response = ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).strong())
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke)),
    );
    if let Some(reason) = unavailable_reason {
        response = response.on_disabled_hover_text(reason);
    }
    if response.clicked() {
        start_project_action(ui, process, workspace, action);
    }
}

fn unsupported_project_action(
    project: &ProjectInfo,
    action: ProcessAction,
) -> Option<&'static str> {
    if project.template == ProjectTemplate::VisualStudioCpp {
        Some(
            "Imported Visual Studio C++ projects can be browsed and edited, but this build runner only builds CMake projects.",
        )
    } else if action == ProcessAction::Publish
        && project.language == ProjectLanguage::Cpp
        && !has_cmake_install_rules(project)
    {
        Some("Add install() rules to CMakeLists.txt and build the project before publishing.")
    } else if action == ProcessAction::Publish
        && project.language == ProjectLanguage::CSharp
        && project.template == ProjectTemplate::ClassLibrary
    {
        Some("Publish is for runnable C# apps. Use Create Package for a class library.")
    } else if action == ProcessAction::Analyze
        && project.language == ProjectLanguage::Cpp
        && !project.path.join("build/compile_commands.json").is_file()
    {
        Some("Build the C++ project once before running clang-tidy analysis.")
    } else if action == ProcessAction::Package
        && project.language == ProjectLanguage::Cpp
        && !project.path.join("build/CPackConfig.cmake").is_file()
    {
        Some("Add include(CPack) to CMakeLists.txt and build before creating a package.")
    } else if action == ProcessAction::Package
        && project.language == ProjectLanguage::CSharp
        && project.template != ProjectTemplate::ClassLibrary
    {
        Some("Create Package is available for C# class libraries.")
    } else if project.template == ProjectTemplate::ClassLibrary
        && matches!(action, ProcessAction::Run | ProcessAction::Debug)
    {
        Some(
            "A class library has no executable to run or debug. Create a console app that links it.",
        )
    } else {
        None
    }
}

fn has_cmake_install_rules(project: &ProjectInfo) -> bool {
    let build_root = project.path.join("build");
    if !build_root.is_dir() {
        return false;
    }
    let mut pending = vec![build_root];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name == "cmake_install.cmake")
                && std::fs::read_to_string(path)
                    .is_ok_and(|script| script.to_ascii_lowercase().contains("file(install"))
            {
                return true;
            }
        }
    }
    false
}

fn start_project_action(
    ui: &egui::Ui,
    process: &ProcessHandle,
    workspace: &mut Workspace,
    action: ProcessAction,
) {
    let Some(project) = workspace.current.clone() else {
        workspace.message = "Open a project before building or running it.".to_owned();
        return;
    };
    if let Some(reason) = unsupported_project_action(&project, action) {
        workspace.message = reason.to_owned();
        return;
    }
    if process
        .state()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .running
    {
        workspace.message = "A build, run, or debug process is already active.".to_owned();
        return;
    }
    // Always build what is on screen: save every modified file first.
    let settings = crate::settings::current(ui.ctx());
    let formatting = settings.formatting;
    if formatting.format_on_save {
        let errors = format_dirty_documents(workspace, &formatting);
        if !errors.is_empty() {
            workspace.message = format!(
                "Formatting skipped for {} file(s): {}",
                errors.len(),
                errors.join("; ")
            );
        }
    }
    if let Err(error) = workspace.save_all_with_line_ending(settings.general.line_ending) {
        workspace.message = error;
        return;
    }
    let breakpoints = if action == ProcessAction::Debug {
        workspace.all_breakpoints()
    } else {
        Vec::new()
    };
    let start_result = if action == ProcessAction::Debug {
        process.start_with_breakpoints(project, action, breakpoints)
    } else {
        process.start(project, action)
    };
    workspace.message = match start_result {
        Ok(()) => format!("{} started", action.label()),
        Err(error) => error,
    };
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum OutputFilter {
    #[default]
    All,
    Errors,
    Warnings,
}

#[derive(Debug, Default, Clone)]
struct OutputViewState {
    query: String,
    filter: OutputFilter,
    follow_output: bool,
}

fn process_output_ui(ui: &mut egui::Ui, process: &ProcessHandle, max_height: f32) {
    let view_id = egui::Id::unique("process-output-view-state");
    let mut view = ui
        .ctx()
        .data(|data| data.get_temp::<OutputViewState>(view_id))
        .unwrap_or_default();
    let process_state = process.state();
    let mut state = process_state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let output = state.output.clone();
    let running = state.running;
    let last_result = state.last_result.clone();
    let mut stdin_to_send = None;

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("BUILD OUTPUT").strong());
            ui.label(
                RichText::new(format!("{} lines", output.len()))
                    .small()
                    .color(theme::muted()),
            );
            ui.separator();
            ui.selectable_value(&mut view.filter, OutputFilter::All, "All");
            ui.selectable_value(&mut view.filter, OutputFilter::Errors, "Errors");
            ui.selectable_value(&mut view.filter, OutputFilter::Warnings, "Warnings");
            ui.add_sized(
                [ui.available_width().min(240.0).max(120.0), 26.0],
                egui::TextEdit::singleline(&mut view.query).hint_text("Find in output"),
            );
            if ui
                .button("Copy")
                .on_hover_text("Copy visible output")
                .clicked()
            {
                let text = filtered_output_lines(&output, &view)
                    .into_iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                ui.ctx().copy_text(text);
            }
            if ui
                .button("Clear")
                .on_hover_text("Clear the output log")
                .clicked()
            {
                state.output.clear();
            }
            ui.checkbox(&mut view.follow_output, "Follow");
        });
        ui.add_space(4.0);
        if let Some(Err(error)) = &last_result {
            ui.colored_label(Color32::LIGHT_RED, error);
        } else if matches!(last_result, Some(Ok(()))) {
            ui.colored_label(Color32::LIGHT_GREEN, "Process completed successfully.");
        }

        let visible_lines = filtered_output_lines(&output, &view);
        if visible_lines.is_empty() {
            let message = if output.is_empty() {
                "Build, run, and debug output will appear here."
            } else {
                "No output matches these filters."
            };
            ui.label(RichText::new(message).color(theme::muted()));
        } else {
            egui::ScrollArea::both()
                .id_salt("process-output")
                .auto_shrink([false, false])
                .stick_to_bottom(view.follow_output || running)
                .max_height(max_height)
                .show(ui, |ui| {
                    for (index, line) in visible_lines.iter().enumerate() {
                        let color = match parse_build_diagnostic(line).map(|item| item.severity) {
                            Some(DiagnosticSeverity::Error) => Color32::LIGHT_RED,
                            Some(DiagnosticSeverity::Warning) => Color32::from_rgb(235, 190, 95),
                            Some(DiagnosticSeverity::Information) => {
                                Color32::from_rgb(90, 165, 235)
                            }
                            Some(DiagnosticSeverity::Hint) => Color32::from_rgb(145, 155, 170),
                            None if line.to_ascii_lowercase().contains("succeeded") => {
                                Color32::LIGHT_GREEN
                            }
                            None if line.starts_with("> ") => theme::accent_hover(),
                            None => theme::text(),
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{:>4}", index + 1))
                                    .small()
                                    .color(theme::muted()),
                            );
                            ui.label(
                                RichText::new(line.as_str())
                                    .monospace()
                                    .size(12.5)
                                    .color(color),
                            );
                        });
                    }
                });
        }
        if running {
            ui.separator();
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.stdin_buffer)
                    .desired_width(f32::INFINITY)
                    .hint_text("Program input · Enter to send"),
            );
            if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                let mut text = std::mem::take(&mut state.stdin_buffer);
                text.push('\n');
                stdin_to_send = Some(text);
            }
        }
    });
    ui.ctx().data_mut(|data| data.insert_temp(view_id, view));
    drop(state);
    if let Some(text) = stdin_to_send {
        let _ = process.send_stdin(text);
    }
}

fn filtered_output_lines<'a>(lines: &'a [String], view: &OutputViewState) -> Vec<&'a String> {
    lines
        .iter()
        .filter(|line| {
            let diagnostic = parse_build_diagnostic(line);
            let severity_matches = match view.filter {
                OutputFilter::All => true,
                OutputFilter::Errors => diagnostic
                    .as_ref()
                    .is_some_and(|d| d.severity == DiagnosticSeverity::Error),
                OutputFilter::Warnings => diagnostic
                    .as_ref()
                    .is_some_and(|d| d.severity == DiagnosticSeverity::Warning),
            };
            severity_matches
                && (view.query.trim().is_empty()
                    || line
                        .to_lowercase()
                        .contains(&view.query.trim().to_lowercase()))
        })
        .collect()
}

fn debugger_console_ui(ui: &mut egui::Ui, process: &ProcessHandle, max_height: f32) {
    let (active, stopping, output, location, last_result, call_stack, selected_frame, running) = {
        let state = process.state();
        let state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (
            state.debugger_active,
            state.stopping,
            state.output.clone(),
            state.debug_location.clone(),
            state.last_result.clone(),
            state.debug_call_stack.clone(),
            state.selected_debug_frame,
            state.running && state.debug_location.is_none(),
        )
    };
    if !active {
        if let Some(Err(error)) = last_result {
            ui.colored_label(
                Color32::LIGHT_RED,
                format!("Debugger could not start: {error}"),
            );
            ui.label("Fix the build errors below, then start debugging again.");
        } else {
            ui.label("No debugger session is active. C++ uses GDB/MI; C# uses NetCoreDbg/MI when installed.");
        }
        egui::ScrollArea::both()
            .id_salt("debugger-startup-output")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .max_height(max_height)
            .show(ui, |ui| {
                for line in output.iter().rev().take(100).rev() {
                    ui.label(RichText::new(line).monospace().size(12.5));
                }
            });
        return;
    }
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Current location");
        if let Some(location) = &location {
            ui.label(
                RichText::new(format!(
                    "{}:{}{}",
                    location.file.display(),
                    location.line,
                    location
                        .function
                        .as_ref()
                        .map(|function| format!(" · {function}"))
                        .unwrap_or_default()
                ))
                .color(theme::accent_hover()),
            );
        } else if stopping {
            ui.label("Stopping the debug session and target program…");
        } else {
            ui.label(
                RichText::new("Running · pause to inspect the current location.")
                    .color(theme::muted()),
            );
        }
    });
    ui.add_space(6.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Execution");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for (label, command) in [
                ("Step Into", "-exec-step"),
                ("Step Over", "-exec-next"),
                ("Step Out", "-exec-finish"),
                ("Continue", "-exec-continue"),
            ] {
                if ui
                    .add_enabled(location.is_some() && !stopping, egui::Button::new(label))
                    .clicked()
                {
                    if let Err(error) = process.debugger_command(command.to_owned()) {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(egui::Id::unique("debugger-command-error"), error)
                        });
                    }
                }
            }
            if ui
                .add_enabled(running && !stopping, egui::Button::new("Pause"))
                .clicked()
                && let Err(error) = process.debugger_command("-exec-interrupt".to_owned())
            {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::unique("debugger-command-error"), error)
                });
            }
            if ui
                .add_enabled(!stopping, egui::Button::new("Restart"))
                .clicked()
            {
                if let Err(error) = process.restart() {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::unique("debugger-command-error"), error)
                    });
                }
            }
            if ui
                .add_enabled(
                    !stopping,
                    egui::Button::new(if stopping { "Stopping…" } else { "Stop" }),
                )
                .clicked()
            {
                process.cancel();
            }
        });
    });
    ui.add_space(6.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Call stack");
        if ui.button("Refresh").clicked() {
            let _ = process.debugger_command("-stack-list-frames".to_owned());
        }
        if call_stack.is_empty() {
            ui.label(
                RichText::new("Pause the program to inspect its call stack.")
                    .small()
                    .color(theme::muted()),
            );
        } else {
            egui::ScrollArea::vertical()
                .id_salt("debugger-call-stack")
                .max_height(110.0)
                .show(ui, |ui| {
                    for frame in &call_stack {
                        let label = format!(
                            "{}  {}{}",
                            frame.function.as_deref().unwrap_or("<unknown function>"),
                            frame
                                .file
                                .as_ref()
                                .map(|path| path.display().to_string())
                                .unwrap_or_else(|| frame.module.clone().unwrap_or_default()),
                            frame
                                .line
                                .map(|line| format!(":{line}"))
                                .unwrap_or_default()
                        );
                        if ui
                            .selectable_label(frame.level == selected_frame, label)
                            .clicked()
                            && let Err(error) = process.select_debug_frame(frame.level)
                        {
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::unique("debugger-command-error"), error)
                            });
                        }
                    }
                });
        }
    });
    ui.add_space(6.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Evaluate expression");
        ui.horizontal(|ui| {
            let id = egui::Id::unique("debugger-expression");
            let mut expression = ui
                .ctx()
                .data(|data| data.get_temp::<String>(id))
                .unwrap_or_default();
            let response = ui.add(
                egui::TextEdit::singleline(&mut expression)
                    .desired_width((ui.available_width() - 96.0).max(120.0))
                    .hint_text("Variable or expression"),
            );
            let evaluate = ui.button("Evaluate").clicked()
                || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
            ui.ctx()
                .data_mut(|data| data.insert_temp(id, expression.clone()));
            if evaluate
                && !expression.trim().is_empty()
                && let Err(error) = process.evaluate_expression(expression.trim())
            {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::unique("debugger-command-error"), error)
                });
            }
        });
    });
    let error = ui
        .ctx()
        .data(|data| data.get_temp::<String>(egui::Id::unique("debugger-command-error")));
    if let Some(error) = error {
        ui.colored_label(Color32::LIGHT_RED, error);
    }
    egui::CollapsingHeader::new("Advanced debugger console")
        .default_open(false)
        .show(ui, |ui| {
            let command_id = egui::Id::unique("debugger-mi-command");
            let mut command = ui
                .ctx()
                .data(|data| data.get_temp::<String>(command_id))
                .unwrap_or_default();
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut command)
                        .desired_width((ui.available_width() - 72.0).max(120.0))
                        .hint_text("Advanced GDB/MI command"),
                );
                let send = ui.button("Send").clicked()
                    || (response.has_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                if send && !command.trim().is_empty() {
                    if let Err(error) = process.debugger_command(command.clone()) {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(egui::Id::unique("debugger-command-error"), error)
                        });
                    } else {
                        command.clear();
                    }
                }
                ui.ctx()
                    .data_mut(|data| data.insert_temp(command_id, command.clone()));
            });
            egui::ScrollArea::both()
                .id_salt("debugger-mi-output")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .max_height(max_height)
                .show(ui, |ui| {
                    for line in output.iter().rev().take(120).rev() {
                        let color = if line.contains("^error") || line.contains("Could not") {
                            Color32::LIGHT_RED
                        } else if line.contains("*stopped") || line.starts_with("Debugger") {
                            theme::accent_hover()
                        } else {
                            theme::text()
                        };
                        ui.label(RichText::new(line).monospace().size(12.5).color(color));
                    }
                });
        });
}

fn debugger_variables_ui(ui: &mut egui::Ui, process: &ProcessHandle, workspace: &Workspace) {
    let (variables, evaluation, location) = {
        let state = process.state();
        let state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (
            state.debug_variables.clone(),
            state.debug_evaluation.clone(),
            state.debug_location.clone(),
        )
    };
    let selected_id = egui::Id::unique("debugger-selected-variable");
    let mut selected = ui
        .ctx()
        .data(|data| data.get_temp::<String>(selected_id))
        .unwrap_or_default();
    let filter_id = egui::Id::unique("debugger-variable-filter");
    let mut filter = ui
        .ctx()
        .data(|data| data.get_temp::<String>(filter_id))
        .unwrap_or_default();
    let project_path = workspace
        .current
        .as_ref()
        .map(|project| project.path.clone());
    let watches_id = egui::Id::unique("debugger-watch-expressions");
    let mut watches = ui
        .ctx()
        .data(|data| data.get_temp::<(Option<std::path::PathBuf>, Vec<String>)>(watches_id))
        .filter(|(path, _)| path == &project_path)
        .map(|(_, watches)| watches)
        .unwrap_or_else(|| load_debug_watches(project_path.as_deref()));
    let watch_input_id = egui::Id::unique("debugger-watch-input");
    let mut watch_input = ui
        .ctx()
        .data(|data| data.get_temp::<String>(watch_input_id))
        .unwrap_or_default();
    egui::Frame::new()
        .fill(theme::panel())
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Variables");
                ui.label(
                    RichText::new(format!("{} locals", variables.len()))
                        .small()
                        .color(theme::muted()),
                );
                if let Some(location) = &location {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{}:{}", location.file.display(), location.line))
                                .small()
                                .color(theme::accent_hover()),
                        );
                    });
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .desired_width(ui.available_width() - 112.0)
                        .hint_text("Filter variables…"),
                );
                if ui
                    .add_enabled(location.is_some(), egui::Button::new("↻ Refresh"))
                    .clicked()
                {
                    let _ =
                        process.debugger_command("-stack-list-variables --all-values".to_owned());
                }
            });
        });
    ui.ctx()
        .data_mut(|data| data.insert_temp(filter_id, filter.clone()));

    let folded_filter = filter.to_ascii_lowercase();
    let filtered = variables
        .iter()
        .filter(|variable| {
            folded_filter.is_empty()
                || variable.name.to_ascii_lowercase().contains(&folded_filter)
                || variable.value.to_ascii_lowercase().contains(&folded_filter)
        })
        .collect::<Vec<_>>();
    egui::ScrollArea::vertical()
        .id_salt("debugger-variable-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Watch Expressions");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut watch_input)
                        .desired_width(ui.available_width() - 55.0)
                        .hint_text("Add expression"),
                );
                let focus_watch = ui.ctx().data_mut(|data| {
                    data.get_temp::<bool>(egui::Id::unique("debugger-focus-watch-input"))
                        .unwrap_or(false)
                });
                if focus_watch {
                    response.request_focus();
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::unique("debugger-focus-watch-input"), false)
                    });
                }
                if ui.button("Add").clicked() && !watch_input.trim().is_empty() {
                    let expression = watch_input.trim().to_owned();
                    if !watches.contains(&expression) {
                        watches.push(expression);
                    }
                    watch_input.clear();
                    save_debug_watches(project_path.as_deref(), &watches);
                }
            });
            for index in (0..watches.len()).rev() {
                ui.horizontal(|ui| {
                    let changed = ui
                        .add(
                            egui::TextEdit::singleline(&mut watches[index])
                                .desired_width(ui.available_width() - 150.0)
                                .font(egui::TextStyle::Monospace),
                        )
                        .changed();
                    if changed {
                        save_debug_watches(project_path.as_deref(), &watches);
                    }
                    if ui
                        .add_enabled(location.is_some(), egui::Button::new("Evaluate"))
                        .clicked()
                    {
                        if let Err(error) = process.evaluate_expression(&watches[index]) {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        }
                    }
                    if ui.small_button("×").on_hover_text("Remove watch").clicked() {
                        watches.remove(index);
                        save_debug_watches(project_path.as_deref(), &watches);
                    }
                });
                if let Some((expression, value)) = &evaluation
                    && expression == &watches[index]
                {
                    ui.label(
                        RichText::new(format!("{expression} = {value}"))
                            .monospace()
                            .color(theme::accent_hover()),
                    );
                }
            }
            ui.ctx().data_mut(|data| {
                data.insert_temp(watches_id, (project_path.clone(), watches.clone()));
                data.insert_temp(watch_input_id, watch_input.clone());
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("LOCAL SCOPE")
                        .small()
                        .strong()
                        .color(theme::muted()),
                );
                ui.separator();
                ui.label(RichText::new("Name").small().color(theme::muted()));
                ui.label(RichText::new("Value").small().color(theme::muted()));
                ui.label(RichText::new("Type").small().color(theme::muted()));
            });
            if filtered.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(if variables.is_empty() {
                        "Pause or step the program to inspect local values."
                    } else {
                        "No variables match this filter."
                    })
                    .color(theme::muted()),
                );
            }
            for variable in &filtered {
                let is_selected = selected == variable.name;
                egui::Frame::new()
                    .fill(if is_selected {
                        theme::hover()
                    } else {
                        theme::panel()
                    })
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .corner_radius(egui::CornerRadius::same(4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let name_width = (ui.available_width() * 0.27).clamp(105.0, 220.0);
                            let response = ui.add_sized(
                                [name_width, 24.0],
                                egui::Button::selectable(
                                    is_selected,
                                    RichText::new(format!(
                                        "{} {}",
                                        if is_selected { "▾" } else { "›" },
                                        variable.name
                                    ))
                                    .monospace(),
                                ),
                            );
                            if response.clicked() {
                                if is_selected {
                                    selected.clear();
                                } else {
                                    selected.clone_from(&variable.name);
                                }
                            }
                            let value_width = (ui.available_width() * 0.68).max(80.0);
                            ui.add_sized(
                                [value_width, 24.0],
                                egui::Label::new(RichText::new(&variable.value).monospace())
                                    .truncate(),
                            )
                            .on_hover_text(&variable.value);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&variable.type_name)
                                        .small()
                                        .color(theme::muted()),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&variable.type_name);
                        });
                    });
            }
            ui.ctx()
                .data_mut(|data| data.insert_temp(selected_id, selected.clone()));

            if let Some(variable) = variables.iter().find(|variable| variable.name == selected) {
                let expression_id = egui::Id::unique("debugger-variable-expression");
                let saved_expression = ui
                    .ctx()
                    .data(|data| data.get_temp::<(String, String)>(expression_id));
                let mut expression = saved_expression
                    .filter(|(owner, _)| owner == &variable.name)
                    .map(|(_, expression)| expression)
                    .unwrap_or_else(|| variable.name.clone());
                egui::Frame::new()
                    .fill(theme::editor())
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .corner_radius(egui::CornerRadius::same(5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.heading(&variable.name);
                            ui.label(
                                RichText::new(&variable.type_name)
                                    .small()
                                    .color(theme::muted()),
                            );
                        });
                        ui.label(RichText::new("Current value").small().color(theme::muted()));
                        egui::Frame::new()
                            .fill(theme::panel())
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new(&variable.value).monospace());
                            });
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut expression)
                                    .desired_width(ui.available_width() - 100.0)
                                    .hint_text("Expression, e.g. player.Health"),
                            );
                            if ui
                                .add_enabled(location.is_some(), egui::Button::new("Evaluate"))
                                .clicked()
                                && let Err(error) = process.evaluate_expression(expression.trim())
                            {
                                ui.colored_label(Color32::LIGHT_RED, error);
                            }
                        });
                        if let Some((evaluated, value)) = &evaluation {
                            ui.label(
                                RichText::new(format!("{evaluated} = {value}"))
                                    .monospace()
                                    .color(theme::accent_hover()),
                            );
                        }
                    });
                ui.ctx().data_mut(|data| {
                    data.insert_temp(expression_id, (variable.name.clone(), expression))
                });
            }
        });
}

fn load_debug_watches(project_path: Option<&std::path::Path>) -> Vec<String> {
    let Some(path) = project_path else {
        return Vec::new();
    };
    std::fs::read(path.join(".cairn/debug-watches.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default()
}

fn save_debug_watches(project_path: Option<&std::path::Path>, watches: &[String]) {
    let Some(path) = project_path else { return };
    let directory = path.join(".cairn");
    if std::fs::create_dir_all(&directory).is_ok()
        && let Ok(bytes) = serde_json::to_vec_pretty(watches)
    {
        let _ = std::fs::write(directory.join("debug-watches.json"), bytes);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Diagnostic {
    severity: DiagnosticSeverity,
    message: String,
    file: Option<String>,
    line: Option<usize>,
    column: Option<usize>,
    end_line: Option<usize>,
    end_column: Option<usize>,
    source: Option<String>,
}

impl Diagnostic {
    fn contains_position(&self, line: usize, column: usize) -> bool {
        let Some(start_line) = self.line else {
            return false;
        };
        if line < start_line || self.end_line.map_or(line != start_line, |end| line > end) {
            return false;
        }
        let starts_here = line != start_line || self.column.is_none_or(|start| column >= start);
        let ends_here =
            self.end_line != Some(line) || self.end_column.is_none_or(|end| column < end);
        starts_here && ends_here
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

fn diagnostic_color(severity: DiagnosticSeverity) -> Color32 {
    match severity {
        DiagnosticSeverity::Error => Color32::from_rgb(239, 83, 80),
        DiagnosticSeverity::Warning => Color32::from_rgb(235, 174, 55),
        DiagnosticSeverity::Information => Color32::from_rgb(80, 155, 235),
        DiagnosticSeverity::Hint => Color32::from_rgb(145, 155, 170),
    }
}

fn parse_build_diagnostic(line: &str) -> Option<Diagnostic> {
    let lower = line.to_ascii_lowercase();
    let (severity, marker) = if let Some(position) = lower.find(": error ") {
        (DiagnosticSeverity::Error, position)
    } else if let Some(position) = lower.find(": error:") {
        (DiagnosticSeverity::Error, position)
    } else if let Some(position) = lower.find(": warning ") {
        (DiagnosticSeverity::Warning, position)
    } else if let Some(position) = lower.find(": warning:") {
        (DiagnosticSeverity::Warning, position)
    } else if let Some(position) = lower.find("): error ") {
        (DiagnosticSeverity::Error, position + 1)
    } else {
        let position = lower.find("): warning ")?;
        (DiagnosticSeverity::Warning, position + 1)
    };
    let prefix = &line[..marker];
    let message_start = if marker + 1 < line.len()
        && matches!(line.as_bytes().get(marker + 1), Some(b' ' | b':'))
    {
        marker + 2
    } else {
        marker + 1
    };
    let message = line[message_start..].trim().to_owned();
    let (file, line_number, column) = parse_diagnostic_location(prefix);
    Some(Diagnostic {
        severity,
        message,
        file,
        line: line_number,
        column,
        end_line: None,
        end_column: None,
        source: Some("build".to_owned()),
    })
}

fn parse_diagnostic_location(prefix: &str) -> (Option<String>, Option<usize>, Option<usize>) {
    if let Some(open) = prefix.rfind('(')
        && let Some(close) = prefix[open..].find(')')
    {
        let location = &prefix[open + 1..open + close];
        let mut parts = location.split(',');
        let line = parts.next().and_then(|value| value.trim().parse().ok());
        let column = parts.next().and_then(|value| value.trim().parse().ok());
        if line.is_some() {
            return (Some(prefix[..open].to_owned()), line, column);
        }
    }
    let mut parts = prefix.rsplitn(3, ':');
    let last = parts.next().and_then(|value| value.parse::<usize>().ok());
    let middle = parts.next().and_then(|value| value.parse::<usize>().ok());
    let file = parts.next();
    match (middle, last, file) {
        (Some(line), Some(column), Some(file)) => (Some(file.to_owned()), Some(line), Some(column)),
        (Some(line), None, Some(file)) => (Some(file.to_owned()), Some(line), None),
        _ => (None, None, None),
    }
}

pub(super) enum EditorCmd {
    Home,
    NewProject,
    Settings,
    OpenProject(std::path::PathBuf),
    Quit,
}

#[derive(Clone)]
struct PendingNewItem {
    directory: std::path::PathBuf,
    kind: NewSourceKind,
    name: String,
    error: Option<String>,
    focus_requested: bool,
}

#[derive(Clone)]
struct PendingNewFolder {
    directory: std::path::PathBuf,
    name: String,
    error: Option<String>,
    focus_requested: bool,
}

fn new_item_dialog_id() -> egui::Id {
    egui::Id::unique("new-project-item-dialog")
}

fn new_folder_dialog_id() -> egui::Id {
    egui::Id::unique("new-project-folder-dialog")
}

fn bar_frame(fill: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(10, 0))
}

pub(super) fn format_selected(
    workspace: &mut Workspace,
    settings: &crate::settings::FormattingSettings,
    line_ending: crate::settings::LineEnding,
) -> Result<(), String> {
    let mut job = selected_format_job(workspace, settings, line_ending).ok_or_else(|| {
        if workspace.current.is_none() {
            "Open a project before formatting a file.".to_owned()
        } else {
            "Save the scratch document into a project before formatting it.".to_owned()
        }
    })?;
    job.save_after_format = false;
    let name = job
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_owned();
    enqueue_format_job(job);
    workspace.message = format!("Formatting {name} in the background…");
    Ok(())
}

fn format_selected_now(
    workspace: &mut Workspace,
    settings: &crate::settings::FormattingSettings,
) -> Result<(), String> {
    let root = workspace
        .current
        .as_ref()
        .ok_or_else(|| "Open a project before formatting a file.".to_owned())?
        .path
        .clone();
    let index = workspace.selected_document;
    let (path, text) = {
        let document = workspace
            .documents
            .get(index)
            .ok_or_else(|| "There is no active editor document to format.".to_owned())?;
        let path = document.path.clone().ok_or_else(|| {
            "Save the scratch document into a project before formatting it.".to_owned()
        })?;
        (path, document.text.clone())
    };
    let formatted = crate::formatting::format_document(&path, &root, &text, settings)?;
    let document = &mut workspace.documents[index];
    document.text = formatted;
    document.dirty = true;
    workspace.message = format!("Formatted {}", path.display());
    Ok(())
}

pub(super) fn save_current(
    workspace: &mut Workspace,
    formatting: &crate::settings::FormattingSettings,
    line_ending: crate::settings::LineEnding,
) {
    let format_job = formatting
        .format_on_save
        .then(|| selected_format_job(workspace, formatting, line_ending))
        .flatten();
    let save_result = workspace.save_selected_with_line_ending(line_ending);
    if let Err(error) = save_result {
        workspace.message = error;
        return;
    }
    if let Some(job) = format_job {
        enqueue_format_job(job);
        workspace.message = "File saved; formatting in background…".to_owned();
    } else {
        workspace.message = "File saved".to_owned();
    }
}

pub(super) fn save_all(
    workspace: &mut Workspace,
    formatting: &crate::settings::FormattingSettings,
    line_ending: crate::settings::LineEnding,
) {
    let jobs = if formatting.format_on_save {
        dirty_format_jobs(workspace, formatting, line_ending)
    } else {
        Vec::new()
    };
    if let Err(error) = workspace.save_all_with_line_ending(line_ending) {
        workspace.message = error;
        return;
    }
    let count = jobs.len();
    for job in jobs {
        enqueue_format_job(job);
    }
    workspace.message = if count == 0 {
        "All files saved".to_owned()
    } else {
        format!("All files saved; formatting {count} file(s) in background…")
    };
}

fn selected_format_job(
    workspace: &Workspace,
    settings: &crate::settings::FormattingSettings,
    line_ending: crate::settings::LineEnding,
) -> Option<FormatJob> {
    let root = workspace.current.as_ref()?.path.clone();
    let document = workspace.documents.get(workspace.selected_document)?;
    Some(FormatJob {
        path: document.path.clone()?,
        root,
        text: document.text.clone(),
        settings: settings.clone(),
        line_ending,
        save_after_format: true,
    })
}

fn dirty_format_jobs(
    workspace: &Workspace,
    settings: &crate::settings::FormattingSettings,
    line_ending: crate::settings::LineEnding,
) -> Vec<FormatJob> {
    let Some(root) = workspace
        .current
        .as_ref()
        .map(|project| project.path.clone())
    else {
        return Vec::new();
    };
    workspace
        .documents
        .iter()
        .filter(|document| document.dirty)
        .filter_map(|document| {
            Some(FormatJob {
                path: document.path.clone()?,
                root: root.clone(),
                text: document.text.clone(),
                settings: settings.clone(),
                line_ending,
                save_after_format: true,
            })
        })
        .collect()
}

fn enqueue_format_job(job: FormatJob) {
    let worker = format_worker();
    let mut in_flight = worker
        .in_flight
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if in_flight.contains(&job.path) {
        worker
            .queued
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(job.path.clone(), job);
        return;
    }
    in_flight.insert(job.path.clone());
    drop(in_flight);
    if worker.sender.send(job).is_err() {
        // The worker is process-wide; a closed receiver means formatting cannot
        // continue, but it must never block or prevent the source save.
        worker
            .in_flight
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        worker
            .queued
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

fn poll_format_results(workspace: &mut Workspace) {
    let Some(worker) = FORMAT_WORKER.get() else {
        return;
    };
    let receiver = worker
        .receiver
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut results = Vec::new();
    loop {
        match receiver.try_recv() {
            Ok(result) => results.push(result),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
    drop(receiver);
    let mut symbols_changed = false;
    for result in results {
        worker
            .in_flight
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&result.path);
        let queued = worker
            .queued
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&result.path);
        if let Some(job) = queued {
            worker
                .in_flight
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(job.path.clone());
            if worker.sender.send(job).is_err() {
                worker
                    .in_flight
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clear();
                worker
                    .queued
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clear();
            }
            // This result belongs to an older saved buffer. The replacement
            // job formats the newest text and will be applied when it returns.
            continue;
        }
        let Some(index) = workspace
            .documents
            .iter()
            .position(|document| document.path.as_ref() == Some(&result.path))
        else {
            continue;
        };
        if workspace.documents[index].text != result.text {
            workspace.message = format!(
                "Formatting result for {} was discarded because the file changed while formatting",
                result.path.display()
            );
            continue;
        }
        match result.result {
            Ok(formatted) if formatted != result.text => {
                workspace.documents[index].text = formatted;
                workspace.documents[index].dirty = true;
                if result.save_after_format {
                    match workspace.save_document_with_line_ending(index, result.line_ending) {
                        Ok(()) => {
                            symbols_changed = true;
                            workspace.message = format!("Formatted {}", result.path.display());
                        }
                        Err(error) => workspace.message = error,
                    }
                } else {
                    workspace.message =
                        format!("Formatted {}; changes are unsaved", result.path.display());
                }
            }
            Ok(_) => workspace.message = format!("{} is already formatted", result.path.display()),
            Err(error) => {
                workspace.message = if result.save_after_format {
                    format!("File saved; formatting skipped: {error}")
                } else {
                    format!("Formatting failed: {error}")
                }
            }
        }
    }
    if symbols_changed && let Err(error) = workspace.refresh_symbols() {
        workspace.message = format!("Files formatted, but symbol refresh failed: {error}");
    }
}

fn close_tabs(workspace: &mut Workspace, mut indices: Vec<usize>) {
    indices.sort_unstable();
    indices.dedup();
    indices.retain(|index| *index < workspace.documents.len());
    if indices
        .iter()
        .any(|index| workspace.documents[*index].dirty)
    {
        workspace.message =
            "Some selected tabs have unsaved changes. Save them before closing.".to_owned();
        return;
    }
    for index in indices.into_iter().rev() {
        if let Err(error) = workspace.close_document(index) {
            workspace.message = error;
            return;
        }
    }
}

pub(super) fn format_dirty_documents(
    workspace: &mut Workspace,
    formatting: &crate::settings::FormattingSettings,
) -> Vec<String> {
    let mut errors = Vec::new();
    for index in 0..workspace.documents.len() {
        if !workspace.documents[index].dirty {
            continue;
        }
        let selected = workspace.selected_document;
        workspace.selected_document = index;
        if let Err(error) = format_selected_now(workspace, formatting) {
            errors.push(error);
        }
        workspace.selected_document = selected;
    }
    errors
}

fn shortcut_text(keymap: &crate::settings::Keymap, action: crate::settings::Action) -> String {
    keymap.display_bindings(action)
}

fn request_or_navigate_definition(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    language_server: &mut crate::language_server::LanguageServerClient,
) {
    let cursor = ctx.data(|data| {
        data.get_temp::<(usize, usize)>(egui::Id::unique("cairn.lsp-cursor-position"))
    });
    let path = workspace
        .active_document()
        .and_then(|document| document.path.clone());
    if let (Some(path), Some((line, character))) = (path.as_deref(), cursor)
        && let Some(id) = language_server.request_definition(path, line, character)
    {
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::unique("cairn.lsp-pending-definition"), id);
        });
        workspace.message = "Looking up definition with the language server…".to_owned();
        return;
    }
    let symbol_id = egui::Id::unique("cairn.editor-symbol-at-cursor");
    let identifier = ctx.data(|data| data.get_temp::<String>(symbol_id));
    if let Some(symbol) = identifier.and_then(|name| {
        workspace
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .cloned()
    }) {
        navigate_to_project_line(ctx, workspace, &symbol.path, symbol.line);
    } else {
        workspace.message = "No definition found. Install clangd or roslyn-language-server for semantic navigation.".to_owned();
    }
}

fn request_references_at_cursor(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    language_server: &mut crate::language_server::LanguageServerClient,
) {
    ctx.data_mut(|data| {
        data.remove::<(std::path::PathBuf, usize, usize)>(egui::Id::unique(
            "cairn.lsp-reference-declaration-filter",
        ));
    });
    let cursor = ctx.data(|data| {
        data.get_temp::<(usize, usize)>(egui::Id::unique("cairn.lsp-cursor-position"))
    });
    let path = workspace
        .active_document()
        .and_then(|document| document.path.clone());
    if let (Some(path), Some((line, character))) = (path.as_deref(), cursor)
        && let Some(id) = language_server.request_references(path, line, character, true)
    {
        let usages_id = language_server.request_references(path, line, character, false);
        ctx.data_mut(|data| {
            data.remove::<usize>(egui::Id::unique("cairn.lsp-reference-usage-count"));
            data.insert_temp(
                egui::Id::unique("cairn.lsp-pending-references"),
                (id, usages_id),
            );
        });
        workspace.message = "Finding references with the language server…".to_owned();
    } else {
        workspace.message =
            "Reference search needs an open C++ or C# file and a running language server."
                .to_owned();
    }
}

fn request_call_hierarchy_at_cursor(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    language_server: &mut crate::language_server::LanguageServerClient,
    direction: CallHierarchyDirection,
) {
    let cursor = ctx.data(|data| {
        data.get_temp::<(usize, usize)>(egui::Id::unique("cairn.lsp-cursor-position"))
    });
    let path = workspace
        .active_document()
        .and_then(|document| document.path.clone());
    if let (Some(path), Some((line, character))) = (path.as_deref(), cursor)
        && let Some(id) = language_server.request_prepare_call_hierarchy(path, line, character)
    {
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::unique("cairn.lsp-pending-call-hierarchy"),
                PendingCallHierarchy {
                    id,
                    direction,
                    prepared_item: None,
                    name: String::new(),
                },
            );
        });
        workspace.message = "Preparing call hierarchy with the language server…".to_owned();
    } else {
        workspace.message =
            "Call hierarchy needs a symbol in an open file and a language server that supports it."
                .to_owned();
    }
}

fn ensure_reference_lens_requests(
    ctx: &egui::Context,
    workspace: &Workspace,
    language_server: &mut crate::language_server::LanguageServerClient,
) -> ReferenceLensState {
    let id = egui::Id::unique("cairn.reference-lens-state");
    let mut state = ctx
        .data(|data| data.get_temp::<ReferenceLensState>(id))
        .unwrap_or_default();
    let (Some(project), Some(document)) = (workspace.current.as_ref(), workspace.active_document())
    else {
        return state;
    };
    let Some(document_path) = document.path.as_deref() else {
        return state;
    };
    let absolute_path = resolve_project_path(&project.path, document_path);
    let same_document = state.root.as_deref() == Some(project.path.as_path())
        && state.document_path.as_deref() == Some(absolute_path.as_path());
    if same_document
        && state
            .checked_at
            .is_some_and(|checked_at| checked_at.elapsed() < Duration::from_millis(200))
    {
        return state;
    }
    let checked_at = Instant::now();
    state.checked_at = Some(checked_at);
    ctx.data_mut(|data| data.insert_temp(id, state.clone()));
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    document.text.hash(&mut hasher);
    let source_stamp = hasher.finish();
    let document_changed = !same_document;
    if document_changed {
        state = ReferenceLensState {
            root: Some(project.path.clone()),
            document_path: Some(absolute_path.clone()),
            source_stamp,
            checked_at: Some(checked_at),
            changed_at: Some(Instant::now()),
            ..ReferenceLensState::default()
        };
        ctx.data_mut(|data| data.insert_temp(id, state.clone()));
        ctx.request_repaint_after(Duration::from_millis(550));
        return state;
    }
    if state.source_stamp != source_stamp {
        // Keep the last known counts visible while the user edits. Drop any
        // in-flight requests from the old buffer and refresh only after the
        // edit debounce expires.
        state.source_stamp = source_stamp;
        state.checked_at = Some(checked_at);
        state.changed_at = Some(Instant::now());
        state.needs_refresh = true;
        state.requested.clear();
        state.retried_empty.clear();
        state.pending.clear();
        ctx.data_mut(|data| data.insert_temp(id, state.clone()));
        ctx.request_repaint_after(Duration::from_millis(550));
        return state;
    }
    if state
        .changed_at
        .is_some_and(|changed_at| changed_at.elapsed() < Duration::from_millis(500))
    {
        ctx.request_repaint_after(Duration::from_millis(550));
        return state;
    }
    let language = language_for_document(document);
    if !matches!(language, "cs" | "cpp") {
        return state;
    }
    if !language_server.is_ready() {
        ctx.request_repaint_after(Duration::from_millis(200));
        return state;
    }
    let candidates =
        crate::project::extract_project_symbols(&document.text, document_path, language)
            .into_iter()
            .filter(|symbol| {
                symbol.kind.contains("function")
                    && resolve_project_path(&project.path, &symbol.path) == absolute_path
            })
            .filter_map(|symbol| {
                let line_text = document.text.lines().nth(symbol.line.saturating_sub(1))?;
                let column = declaration_name_column(line_text, &symbol.name)?;
                let protocol_column = line_text.chars().take(column).map(char::len_utf16).sum();
                Some((symbol.line, column, protocol_column, symbol.name.clone()))
            })
            .collect::<Vec<_>>();
    state.entries.retain(|entry| {
        candidates
            .iter()
            .any(|(line, _, _, name)| entry.line == *line && entry.name == *name)
    });
    let candidate_count = candidates.len();
    let mut requests_sent = 0usize;
    let mut all_candidates_queued = true;
    let request_budget = 40usize.saturating_sub(state.pending.len());
    for (line, column, protocol_column, name) in candidates {
        let key = (absolute_path.clone(), line, name.clone());
        if state.requested.contains(&key)
            || state.pending.iter().any(|pending| {
                pending.path == key.0 && pending.line == line && pending.name == name
            })
            || (!state.needs_refresh
                && state
                    .entries
                    .iter()
                    .any(|entry| entry.path == key.0 && entry.line == line && entry.name == name))
        {
            continue;
        }
        if requests_sent >= request_budget {
            all_candidates_queued = false;
            break;
        }
        // Send the semantic query for every parsed function declaration.
        if let Some(id) =
            language_server.request_references(&absolute_path, line - 1, column, false)
        {
            state.requested.push(key.clone());
            state.pending.push(PendingReferenceLens {
                id,
                path: key.0,
                line,
                column,
                protocol_column,
                name,
            });
            requests_sent += 1;
        } else {
            all_candidates_queued = false;
        }
    }
    if all_candidates_queued {
        state.needs_refresh = false;
    }
    if !state.pending.is_empty() || state.entries.len() < candidate_count || state.needs_refresh {
        // Keep the editor repainting while the server initializes and returns
        // its first reference responses. Otherwise code lenses may not appear
        // until the user types and causes another frame.
        ctx.request_repaint_after(Duration::from_millis(200));
    }
    ctx.data_mut(|data| data.insert_temp(id, state.clone()));
    state
}

fn reference_preview(workspace: &Workspace, path: &std::path::Path, line: usize) -> String {
    let open_document = workspace.documents.iter().find(|document| {
        document.path.as_deref().is_some_and(|document_path| {
            document_path == path
                || workspace
                    .current
                    .as_ref()
                    .is_some_and(|project| project.path.join(document_path) == path)
        })
    });
    let source_line = open_document
        .and_then(|document| {
            document
                .text
                .lines()
                .nth(line.saturating_sub(1))
                .map(str::to_owned)
        })
        .or_else(|| {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|text| text.lines().nth(line.saturating_sub(1)).map(str::to_owned))
        });
    source_line
        .map(|text| {
            let text = text.trim();
            if text.chars().count() > 180 {
                format!("{}…", text.chars().take(177).collect::<String>())
            } else {
                text.to_owned()
            }
        })
        .unwrap_or_else(|| "(source preview unavailable)".to_owned())
}

fn project_relative_display(root: Option<&std::path::Path>, path: &std::path::Path) -> String {
    root.and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn resolve_project_path(root: &std::path::Path, path: &std::path::Path) -> std::path::PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    joined.canonicalize().unwrap_or(joined)
}

fn request_hover_at_cursor(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    language_server: &mut crate::language_server::LanguageServerClient,
) {
    let cursor = ctx.data(|data| {
        data.get_temp::<(usize, usize)>(egui::Id::unique("cairn.lsp-cursor-position"))
    });
    let path = workspace
        .active_document()
        .and_then(|document| document.path.clone());
    if let (Some(path), Some((line, character))) = (path.as_deref(), cursor)
        && let Some(id) = language_server.request_hover(path, line, character)
    {
        ctx.data_mut(|data| data.insert_temp(egui::Id::unique("cairn.lsp-pending-hover"), id));
        workspace.message = "Looking up symbol information…".to_owned();
    } else {
        workspace.message =
            "Symbol information needs an open C++ or C# file and a running language server."
                .to_owned();
    }
}

fn request_rename_at_cursor(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    language_server: &mut crate::language_server::LanguageServerClient,
    new_name: &str,
) {
    if new_name.trim().is_empty() {
        workspace.message = "Enter a new symbol name.".to_owned();
        return;
    }
    let cursor = ctx.data(|data| {
        data.get_temp::<(usize, usize)>(egui::Id::unique("cairn.lsp-cursor-position"))
    });
    let path = workspace
        .active_document()
        .and_then(|document| document.path.clone());
    if let (Some(path), Some((line, character))) = (path.as_deref(), cursor)
        && let Some(id) = language_server.request_rename(path, line, character, new_name.trim())
    {
        ctx.data_mut(|data| data.insert_temp(egui::Id::unique("cairn.lsp-pending-rename"), id));
        workspace.message = "Renaming symbol across the project…".to_owned();
    } else {
        workspace.message =
            "Rename needs an open C++ or C# file and a running language server.".to_owned();
    }
}

fn request_code_actions_at_cursor(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    language_server: &mut crate::language_server::LanguageServerClient,
) {
    let cursor = ctx.data(|data| {
        data.get_temp::<(usize, usize)>(egui::Id::unique("cairn.lsp-cursor-position"))
    });
    let path = workspace
        .active_document()
        .and_then(|document| document.path.clone());
    if let (Some(path), Some((line, character))) = (path.as_deref(), cursor)
        && let Some(id) = language_server.request_code_actions(path, line, character)
    {
        ctx.data_mut(|data| {
            data.remove::<Vec<serde_json::Value>>(egui::Id::unique("cairn.lsp-code-actions"));
            data.insert_temp(egui::Id::unique("cairn.lsp-pending-code-actions"), id);
        });
        workspace.message = "Looking for quick fixes and refactorings…".to_owned();
    } else {
        workspace.message =
            "Quick Actions need an open C++ or C# file and a running language server.".to_owned();
    }
}

fn lsp_position_byte(text: &str, line: usize, character: usize) -> Option<usize> {
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    if line == lines.len() && text.ends_with('\n') && character == 0 {
        return Some(text.len());
    }
    let line_text = *lines.get(line)?;
    let content = line_text
        .strip_suffix('\n')
        .unwrap_or(line_text)
        .strip_suffix('\r')
        .unwrap_or(line_text.strip_suffix('\n').unwrap_or(line_text));
    let mut utf16 = 0;
    for (byte, ch) in content.char_indices() {
        if utf16 == character {
            return Some(line_text.as_ptr() as usize - text.as_ptr() as usize + byte);
        }
        utf16 += ch.len_utf16();
    }
    (utf16 == character)
        .then(|| line_text.as_ptr() as usize - text.as_ptr() as usize + content.len())
}

fn apply_lsp_workspace_edit(
    workspace: &mut Workspace,
    edit: &serde_json::Value,
) -> Result<usize, String> {
    let root = workspace
        .current
        .as_ref()
        .ok_or_else(|| "No project is open.".to_owned())?
        .path
        .canonicalize()
        .map_err(|error| format!("Could not resolve the project folder: {error}"))?;
    let mut edits_by_path: BTreeMap<std::path::PathBuf, Vec<(usize, usize, usize, usize, String)>> =
        BTreeMap::new();
    let mut collect = |uri: &str, edits: &[serde_json::Value]| -> Result<(), String> {
        let path = crate::language_server::parse_file_uri(uri)
            .and_then(|path| path.canonicalize().ok())
            .ok_or_else(|| format!("Could not resolve rename target '{uri}'."))?;
        if !path.starts_with(&root) {
            return Err(format!(
                "Rename target is outside the project: {}",
                path.display()
            ));
        }
        let target_edits = edits_by_path.entry(path).or_default();
        for item in edits {
            let range = item
                .get("range")
                .ok_or_else(|| "Language server returned an edit without a range.".to_owned())?;
            let start_line = range
                .pointer("/start/line")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| "Language server returned an invalid edit start.".to_owned())?
                as usize;
            let start_character = range
                .pointer("/start/character")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| "Language server returned an invalid edit start.".to_owned())?
                as usize;
            let end_line = range
                .pointer("/end/line")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| "Language server returned an invalid edit end.".to_owned())?
                as usize;
            let end_character = range
                .pointer("/end/character")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| "Language server returned an invalid edit end.".to_owned())?
                as usize;
            let new_text = item
                .get("newText")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    "Language server returned an edit without replacement text.".to_owned()
                })?;
            // Positions are converted after loading the source text below.
            target_edits.push((
                start_line,
                start_character,
                end_line,
                end_character,
                new_text.to_owned(),
            ));
        }
        Ok(())
    };
    if let Some(changes) = edit.get("changes").and_then(serde_json::Value::as_object) {
        for (uri, edits) in changes {
            collect(uri, edits.as_array().map(Vec::as_slice).unwrap_or_default())?;
        }
    } else if let Some(changes) = edit
        .get("documentChanges")
        .and_then(serde_json::Value::as_array)
    {
        for change in changes {
            if let (Some(uri), Some(edits)) = (
                change
                    .pointer("/textDocument/uri")
                    .and_then(serde_json::Value::as_str),
                change.get("edits").and_then(serde_json::Value::as_array),
            ) {
                collect(uri, edits)?;
            }
        }
    }
    drop(collect);
    if edits_by_path.is_empty() {
        return Err("The language server returned no file edits for this rename.".to_owned());
    }

    let mut plans = Vec::new();
    for (path, edits) in edits_by_path {
        let original = workspace
            .documents
            .iter()
            .find(|document| {
                document.path.as_ref().is_some_and(|document_path| {
                    document_path == &path
                        || document_path.canonicalize().ok().as_deref() == Some(path.as_path())
                })
            })
            .map(|document| document.text.clone())
            .map(Ok)
            .unwrap_or_else(|| std::fs::read_to_string(&path))
            .map_err(|error| {
                format!("Could not read rename target '{}': {error}", path.display())
            })?;
        let mut byte_edits = Vec::new();
        for (start_line, start_character, end_line, end_character, new_text) in edits {
            let start =
                lsp_position_byte(&original, start_line, start_character).ok_or_else(|| {
                    format!("Rename has an invalid position in '{}'.", path.display())
                })?;
            let end = lsp_position_byte(&original, end_line, end_character).ok_or_else(|| {
                format!("Rename has an invalid position in '{}'.", path.display())
            })?;
            if end < start {
                return Err("Language server returned a reversed edit range.".to_owned());
            }
            byte_edits.push((start, end, new_text.to_owned()));
        }
        byte_edits.sort_by_key(|(start, end, _)| (*start, *end));
        if byte_edits.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(format!(
                "Language server returned overlapping edits for '{}'.",
                path.display()
            ));
        }
        let mut updated = original;
        for (start, end, new_text) in byte_edits.into_iter().rev() {
            updated.replace_range(start..end, &new_text);
        }
        plans.push((path, updated));
    }

    let old_selection = workspace.selected_document;
    for (path, _) in &plans {
        if !workspace.documents.iter().any(|document| {
            document.path.as_ref().is_some_and(|document_path| {
                document_path == path
                    || document_path.canonicalize().ok().as_deref() == Some(path.as_path())
            })
        }) {
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| "Rename target is outside the project.".to_owned())?;
            workspace.open_file(relative)?;
        }
    }
    let mut changed_files = 0;
    for (path, updated) in plans {
        if let Some(document) = workspace.documents.iter_mut().find(|document| {
            document.path.as_ref().is_some_and(|document_path| {
                document_path == &path
                    || document_path.canonicalize().ok().as_deref() == Some(path.as_path())
            })
        }) {
            document.text = updated;
            document.dirty = true;
            changed_files += 1;
        }
    }
    if old_selection < workspace.documents.len() {
        workspace.selected_document = old_selection;
    }
    if changed_files > 0 {
        workspace.refresh_symbols()?;
    }
    Ok(changed_files)
}

fn draw_editor_ui(
    ui: &mut egui::Ui,
    workspace: &mut Workspace,
    editor_layout: &mut EditorLayout,
    designer: &mut WindowDesigner,
    process: &ProcessHandle,
    language_server: &mut crate::language_server::LanguageServerClient,
) -> Option<EditorCmd> {
    let mut command = None;
    let live_settings = crate::settings::current(ui.ctx());
    let keymap = live_settings.keymap.clone();
    let formatting_settings = live_settings.formatting;
    let line_ending = live_settings.general.line_ending;
    if !workspace.current.as_ref().is_some_and(|project| {
        project.template == ProjectTemplate::AvaloniaApp
            && ToolchainManager::supports_fedora_package_management()
    }) {
        editor_layout.designer_mode = false;
    }
    let (debug_stop_sequence, debug_location) = {
        let state = process.state();
        let state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (state.debug_stop_sequence, state.debug_location.clone())
    };
    let handled_stop_id = egui::Id::unique("cairn.handled-debug-stop-sequence");
    let handled_stop = ui
        .ctx()
        .data(|data| data.get_temp::<u64>(handled_stop_id))
        .unwrap_or_default();
    if debug_stop_sequence != 0 && debug_stop_sequence != handled_stop {
        if let (Some(location), Some(project)) = (debug_location, workspace.current.as_ref()) {
            let relative = location
                .file
                .strip_prefix(&project.path)
                .unwrap_or(location.file.as_path());
            if !location.file.is_absolute() || location.file.starts_with(&project.path) {
                editor_layout.output_visible = true;
                editor_layout.show_debugger = true;
                navigate_to_project_line(ui.ctx(), workspace, relative, location.line);
            }
        }
        ui.ctx()
            .data_mut(|data| data.insert_temp(handled_stop_id, debug_stop_sequence));
    }
    language_server.poll();
    let mut reference_lens_state =
        ensure_reference_lens_requests(ui.ctx(), workspace, language_server);
    let pending_definition = ui
        .ctx()
        .data(|data| data.get_temp::<u64>(egui::Id::unique("cairn.lsp-pending-definition")));
    let pending_completion = ui.ctx().data(|data| {
        data.get_temp::<(u64, String)>(egui::Id::unique("cairn.lsp-pending-completion"))
    });
    let pending_references = ui.ctx().data(|data| {
        data.get_temp::<(u64, Option<u64>)>(egui::Id::unique("cairn.lsp-pending-references"))
    });
    let pending_call_hierarchy = ui.ctx().data(|data| {
        data.get_temp::<PendingCallHierarchy>(egui::Id::unique("cairn.lsp-pending-call-hierarchy"))
    });
    let pending_hover = ui
        .ctx()
        .data(|data| data.get_temp::<u64>(egui::Id::unique("cairn.lsp-pending-hover")));
    let pending_mouse_hover = ui.ctx().data(|data| {
        data.get_temp::<(u64, std::path::PathBuf, usize, usize)>(egui::Id::unique(
            "cairn.lsp-pending-mouse-hover",
        ))
    });
    let pending_signature = ui.ctx().data(|data| {
        data.get_temp::<(u64, std::path::PathBuf, usize, usize, usize, usize)>(egui::Id::unique(
            "cairn.lsp-pending-signature-help",
        ))
    });
    let pending_rename = ui
        .ctx()
        .data(|data| data.get_temp::<u64>(egui::Id::unique("cairn.lsp-pending-rename")));
    let pending_code_actions = ui
        .ctx()
        .data(|data| data.get_temp::<u64>(egui::Id::unique("cairn.lsp-pending-code-actions")));
    let pending_document_symbols = ui.ctx().data(|data| {
        data.get_temp::<(u64, std::path::PathBuf, String)>(egui::Id::unique(
            "cairn.lsp-pending-document-symbols",
        ))
    });
    let pending_execute_command = ui.ctx().data(|data| {
        data.get_temp::<(u64, String)>(egui::Id::unique("cairn.lsp-pending-execute-command"))
    });
    let mut completion_response = None;
    let mut references_response = None;
    let mut usage_count_response = None;
    let mut code_actions_response = None;
    let mut hover_response = None;
    let mut mouse_hover_response = None;
    let mut signature_response = None;
    for response in language_server.take_responses() {
        let response_id = response.get("id").and_then(serde_json::Value::as_u64);
        if response.get("method").and_then(serde_json::Value::as_str) == Some("workspace/applyEdit")
        {
            let id = response
                .get("id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let result = match response.pointer("/params/edit") {
                Some(edit) => match apply_lsp_workspace_edit(workspace, edit) {
                    Ok(files) => {
                        workspace.message =
                            format!("Language server applied an edit in {files} file(s).");
                        serde_json::json!({"applied": true})
                    }
                    Err(error) => {
                        workspace.message = format!("Language server edit failed: {error}");
                        serde_json::json!({"applied": false, "failureReason": error})
                    }
                },
                None => {
                    serde_json::json!({"applied": false, "failureReason": "Missing workspace edit."})
                }
            };
            let _ = language_server.respond_to_server_request(id, result);
        } else if let (Some((pending_id, path, text)), Some(id)) =
            (&pending_document_symbols, response_id)
            && id == *pending_id
        {
            let symbols = crate::language_server::document_symbol_entries(&response, path);
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::unique("cairn.lsp-document-symbols"),
                    DocumentSymbolsCache {
                        path: path.clone(),
                        text: text.clone(),
                        symbols,
                    },
                );
                data.remove::<(u64, std::path::PathBuf, String)>(egui::Id::unique(
                    "cairn.lsp-pending-document-symbols",
                ));
            });
        } else if response_id == pending_definition {
            ui.ctx().data_mut(|data| {
                data.remove::<u64>(egui::Id::unique("cairn.lsp-pending-definition"));
            });
            if let Some((path, line)) = crate::language_server::definition_location(&response)
                && let Some(root) = workspace.current.as_ref().map(|project| &project.path)
                && let Ok(relative) = path.strip_prefix(root)
            {
                navigate_to_project_line(ui.ctx(), workspace, relative, line);
            } else if let Some((path, _)) = crate::language_server::definition_location(&response) {
                workspace.message =
                    format!("The definition is outside the project: {}", path.display());
            } else if let Some(error) = response
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
            {
                workspace.message = format!("Language server error: {error}");
            } else if response
                .get("result")
                .is_some_and(serde_json::Value::is_null)
            {
                workspace.message = "The language server found no definition here.".to_owned();
            }
        } else if let (Some((pending_id, word)), Some(id)) = (&pending_completion, response_id)
            && id == *pending_id
        {
            completion_response = Some((word.clone(), parse_completion_items(&response)));
            ui.ctx().data_mut(|data| {
                data.remove::<(u64, String)>(egui::Id::unique("cairn.lsp-pending-completion"));
            });
        } else if let (Some((references_id, _)), Some(id)) = (pending_references, response_id)
            && id == references_id
        {
            references_response = Some(crate::language_server::reference_locations(&response));
        } else if let (Some((_, Some(usages_id))), Some(id)) = (pending_references, response_id)
            && id == usages_id
        {
            usage_count_response =
                Some(crate::language_server::reference_locations(&response).len());
        } else if let Some(index) = response_id.and_then(|id| {
            reference_lens_state
                .pending
                .iter()
                .position(|pending| pending.id == id)
        }) {
            let pending = reference_lens_state.pending.remove(index);
            if response.get("error").is_none() {
                // Some language servers include the declaration even when
                // includeDeclaration=false. Exclude its exact source line so
                // this number always means call sites/usages.
                let count = crate::language_server::reference_locations(&response)
                    .iter()
                    .filter(|location| {
                        is_reference_usage(
                            location,
                            &pending.path,
                            pending.line,
                            pending.protocol_column,
                        )
                    })
                    .count();
                let key = (pending.path.clone(), pending.line, pending.name.clone());
                if count == 0 && !reference_lens_state.retried_empty.contains(&key) {
                    // A few servers complete initialize before their project
                    // index is ready. Retry an empty first answer once so the
                    // opening frame does not permanently cache a false zero.
                    reference_lens_state.retried_empty.push(key.clone());
                    reference_lens_state
                        .requested
                        .retain(|requested| requested != &key);
                    reference_lens_state.changed_at = Some(Instant::now());
                    reference_lens_state.needs_refresh = true;
                    ui.ctx().request_repaint_after(Duration::from_millis(550));
                } else {
                    reference_lens_state.entries.retain(|entry| {
                        entry.path != pending.path
                            || entry.line != pending.line
                            || entry.name != pending.name
                    });
                    reference_lens_state.entries.push(ReferenceLensEntry {
                        path: pending.path,
                        line: pending.line,
                        column: pending.column,
                        protocol_column: pending.protocol_column,
                        name: pending.name,
                        count,
                    });
                }
            } else {
                // A request can race the first didOpen/initialize on project
                // load. Retry after a short pause instead of caching a false 0
                // or flooding the server on every repaint.
                reference_lens_state.requested.retain(|(path, line, name)| {
                    path != &pending.path || *line != pending.line || name != &pending.name
                });
                reference_lens_state.changed_at = Some(Instant::now());
                reference_lens_state.needs_refresh = true;
                ui.ctx().request_repaint_after(Duration::from_millis(550));
            }
        } else if let (Some(pending), Some(id)) = (&pending_call_hierarchy, response_id)
            && id == pending.id
        {
            if let Some(item) = &pending.prepared_item {
                let entries = crate::language_server::call_hierarchy_entries(
                    &response,
                    pending.direction == CallHierarchyDirection::Incoming,
                );
                let direction = pending.direction;
                let count = entries.len();
                ui.ctx().data_mut(|data| {
                    data.remove::<PendingCallHierarchy>(egui::Id::unique(
                        "cairn.lsp-pending-call-hierarchy",
                    ));
                });
                let label = if direction == CallHierarchyDirection::Incoming {
                    "callers"
                } else {
                    "calls"
                };
                if let Some(error) = response
                    .pointer("/error/message")
                    .and_then(serde_json::Value::as_str)
                {
                    workspace.message = format!("Call hierarchy is unavailable: {error}");
                } else {
                    workspace.message = format!("Found {count} {label} for {}.", pending.name);
                }
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        egui::Id::unique("cairn.lsp-call-hierarchy"),
                        CallHierarchyResults {
                            direction,
                            item: item.clone(),
                            name: pending.name.clone(),
                            entries,
                        },
                    )
                });
            } else {
                let items = crate::language_server::call_hierarchy_items(&response);
                if let Some(item) = items.into_iter().next() {
                    let name = item
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("symbol")
                        .to_owned();
                    let request = match pending.direction {
                        CallHierarchyDirection::Incoming => {
                            language_server.request_incoming_calls(item.clone())
                        }
                        CallHierarchyDirection::Outgoing => {
                            language_server.request_outgoing_calls(item.clone())
                        }
                    };
                    if let Some(id) = request {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                egui::Id::unique("cairn.lsp-pending-call-hierarchy"),
                                PendingCallHierarchy {
                                    id,
                                    direction: pending.direction,
                                    prepared_item: Some(item),
                                    name,
                                },
                            )
                        });
                        workspace.message = "Loading call hierarchy…".to_owned();
                    } else {
                        ui.ctx().data_mut(|data| {
                            data.remove::<PendingCallHierarchy>(egui::Id::unique(
                                "cairn.lsp-pending-call-hierarchy",
                            ))
                        });
                        workspace.message =
                            "The language server could not request call hierarchy.".to_owned();
                    }
                } else {
                    ui.ctx().data_mut(|data| {
                        data.remove::<PendingCallHierarchy>(egui::Id::unique(
                            "cairn.lsp-pending-call-hierarchy",
                        ))
                    });
                    workspace.message =
                        "The language server does not provide call hierarchy for this symbol."
                            .to_owned();
                }
            }
        } else if response_id == pending_hover {
            hover_response = crate::language_server::hover_contents(&response);
            ui.ctx().data_mut(|data| {
                data.remove::<u64>(egui::Id::unique("cairn.lsp-pending-hover"));
            });
        } else if let (Some((pending_id, path, line, column)), Some(id)) =
            (&pending_mouse_hover, response_id)
            && id == *pending_id
        {
            mouse_hover_response = crate::language_server::hover_contents(&response)
                .map(|text| (path.clone(), *line, *column, text));
            ui.ctx().data_mut(|data| {
                data.remove::<(u64, std::path::PathBuf, usize, usize)>(egui::Id::unique(
                    "cairn.lsp-pending-mouse-hover",
                ));
            });
        } else if let (
            Some((pending_id, path, line, character, active_parameter, text_length)),
            Some(id),
        ) = (&pending_signature, response_id)
            && id == *pending_id
        {
            signature_response = Some((
                path.clone(),
                *line,
                *character,
                *active_parameter,
                *text_length,
                signature_help_info(&response),
            ));
            ui.ctx().data_mut(|data| {
                data.remove::<(u64, std::path::PathBuf, usize, usize, usize, usize)>(
                    egui::Id::unique("cairn.lsp-pending-signature-help"),
                );
            });
        } else if response_id == pending_rename {
            let result = response
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            match apply_lsp_workspace_edit(workspace, &result) {
                Ok(files) => {
                    workspace.message =
                        format!("Renamed symbol in {files} file(s). Save to write changes.")
                }
                Err(error) => workspace.message = format!("Rename failed: {error}"),
            }
            ui.ctx().data_mut(|data| {
                data.remove::<u64>(egui::Id::unique("cairn.lsp-pending-rename"));
            });
        } else if response_id == pending_code_actions {
            code_actions_response = Some(response.clone());
            ui.ctx().data_mut(|data| {
                data.remove::<u64>(egui::Id::unique("cairn.lsp-pending-code-actions"));
            });
        } else if let (Some((pending_id, title)), Some(id)) =
            (&pending_execute_command, response_id)
            && id == *pending_id
        {
            let result = response
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str);
            workspace.message = result.map_or_else(
                || format!("Quick action finished: {title}"),
                |message| format!("Quick action failed: {message}"),
            );
            ui.ctx().data_mut(|data| {
                data.remove::<(u64, String)>(egui::Id::unique("cairn.lsp-pending-execute-command"));
            });
        }
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            egui::Id::unique("cairn.reference-lens-state"),
            reference_lens_state.clone(),
        )
    });
    if let Some(response) = completion_response {
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::unique("cairn.lsp-completions"), response);
        });
    }
    if let Some(response) = code_actions_response {
        let server_actions = response
            .get("result")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let actions = server_actions
            .iter()
            .filter(|action| {
                action.get("edit").is_some_and(serde_json::Value::is_object)
                    || action.get("command").is_some_and(|command| {
                        command.is_string()
                            || command
                                .get("command")
                                .is_some_and(serde_json::Value::is_string)
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        if actions.is_empty() {
            let detail = response
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str);
            workspace.message = detail.map_or_else(
                || "No quick fixes or refactorings are available here.".to_owned(),
                |message| format!("Language server quick action failed: {message}"),
            );
            ui.ctx().data_mut(|data| {
                data.remove::<Vec<serde_json::Value>>(egui::Id::unique("cairn.lsp-code-actions"));
            });
        } else {
            workspace.message = format!("{} quick action(s) available.", actions.len());
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::unique("cairn.lsp-code-actions"), actions);
            });
        }
    }
    if let Some(mut references) = references_response {
        let declaration_filter = ui.ctx().data(|data| {
            data.get_temp::<(std::path::PathBuf, usize, usize)>(egui::Id::unique(
                "cairn.lsp-reference-declaration-filter",
            ))
        });
        if let Some((declaration_path, declaration_line, declaration_column)) = declaration_filter {
            ui.ctx().data_mut(|data| {
                data.remove::<(std::path::PathBuf, usize, usize)>(egui::Id::unique(
                    "cairn.lsp-reference-declaration-filter",
                ));
            });
            references.retain(|reference| {
                is_reference_usage(
                    reference,
                    &declaration_path,
                    declaration_line,
                    declaration_column,
                )
            });
            let matched_usages = references.len();
            if let Some(entry) = reference_lens_state.entries.iter_mut().find(|entry| {
                entry.path == declaration_path
                    && entry.line == declaration_line
                    && entry.protocol_column == declaration_column
            }) {
                // Keep the visible CodeLens in sync with the exact result the
                // user just opened. This also corrects an earlier empty answer
                // from a server that was still indexing the workspace.
                entry.count = matched_usages;
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        egui::Id::unique("cairn.reference-lens-state"),
                        reference_lens_state.clone(),
                    );
                });
            }
        }
        if references.is_empty() {
            workspace.message = "The language server found no references at the cursor.".to_owned();
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::unique("cairn.lsp-references"),
                    ReferenceResults {
                        locations: Vec::new(),
                        usage_count: Some(0),
                    },
                );
            });
        } else {
            workspace.message = format!("Found {} references.", references.len());
            let usage_count = usage_count_response.or_else(|| {
                ui.ctx().data(|data| {
                    data.get_temp::<usize>(egui::Id::unique("cairn.lsp-reference-usage-count"))
                })
            });
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::unique("cairn.lsp-references"),
                    ReferenceResults {
                        locations: references,
                        usage_count,
                    },
                );
            });
        }
    }
    if let Some(usage_count) = usage_count_response {
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique("cairn.lsp-reference-usage-count"),
                usage_count,
            );
            if let Some(mut results) =
                data.get_temp::<ReferenceResults>(egui::Id::unique("cairn.lsp-references"))
            {
                results.usage_count = Some(usage_count);
                data.insert_temp(egui::Id::unique("cairn.lsp-references"), results);
            }
        });
    }
    if let Some(text) = hover_response {
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::unique("cairn.lsp-hover"), text));
    }
    if let Some(hover) = mouse_hover_response {
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::unique("cairn.lsp-hover-tooltip"), hover));
    }
    if let Some((path, line, character, active_parameter, text_length, info)) = signature_response {
        ui.ctx().data_mut(|data| {
            let id = egui::Id::unique("cairn.lsp-signature-help");
            if let Some(info) = info {
                data.insert_temp(
                    id,
                    (path, line, character, active_parameter, text_length, info),
                );
            } else {
                data.remove::<(
                    std::path::PathBuf,
                    usize,
                    usize,
                    usize,
                    usize,
                    SignatureHelpInfo,
                )>(id);
            }
        });
    }
    if let Some(text) = ui
        .ctx()
        .data(|data| data.get_temp::<String>(egui::Id::unique("cairn.lsp-hover")))
    {
        let mut open = true;
        egui::Window::new("Symbol Information")
            .default_width(420.0)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(text).monospace());
                    });
            });
        if !open {
            ui.ctx()
                .data_mut(|data| data.remove::<String>(egui::Id::unique("cairn.lsp-hover")));
        }
    }
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
        let initial = ui
            .ctx()
            .data(|data| data.get_temp::<String>(egui::Id::unique("cairn.editor-symbol-at-cursor")))
            .unwrap_or_default();
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::unique("cairn.rename-dialog-open"), true);
            data.insert_temp(egui::Id::unique("cairn.rename-new-name"), initial);
        });
    }
    let mut rename_open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(egui::Id::unique("cairn.rename-dialog-open")))
        .unwrap_or(false);
    if rename_open {
        let mut name = ui
            .ctx()
            .data(|data| data.get_temp::<String>(egui::Id::unique("cairn.rename-new-name")))
            .unwrap_or_default();
        let mut submit = false;
        let mut cancel = false;
        egui::Window::new("Rename Symbol")
            .collapsible(false)
            .resizable(false)
            .open(&mut rename_open)
            .show(ui.ctx(), |ui| {
                ui.label("Enter the new name. The language server will update project references.");
                let response = ui.add(egui::TextEdit::singleline(&mut name).desired_width(300.0));
                if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        submit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::unique("cairn.rename-dialog-open"),
                rename_open && !submit && !cancel,
            );
            data.insert_temp(egui::Id::unique("cairn.rename-new-name"), name.clone());
        });
        if submit {
            request_rename_at_cursor(ui.ctx(), workspace, language_server, &name);
        }
    }
    let quick_actions = ui.ctx().data(|data| {
        data.get_temp::<Vec<serde_json::Value>>(egui::Id::unique("cairn.lsp-code-actions"))
    });
    if let Some(actions) = quick_actions {
        let mut open = true;
        let mut selected_action = None;
        egui::Window::new("Quick Actions")
            .collapsible(false)
            .resizable(true)
            .default_width(440.0)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.label("Choose a language-server fix or refactoring:");
                ui.separator();
                for (index, action) in actions.iter().enumerate() {
                    let title = action
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Unnamed action");
                    let kind = action
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("action");
                    let disabled = action.get("disabled").is_some_and(|value| !value.is_null());
                    if ui
                        .add_enabled(!disabled, egui::Button::new(title))
                        .on_hover_text(kind)
                        .clicked()
                    {
                        selected_action = Some((index, action.clone()));
                    }
                }
            });
        if !open || selected_action.is_some() {
            ui.ctx().data_mut(|data| {
                data.remove::<Vec<serde_json::Value>>(egui::Id::unique("cairn.lsp-code-actions"));
            });
        }
        if let Some((_, action)) = selected_action {
            let mut can_execute_command = true;
            if let Some(edit) = action.get("edit") {
                match apply_lsp_workspace_edit(workspace, edit) {
                    Ok(files) => {
                        workspace.message = format!(
                            "Applied quick action in {files} file(s). Save to write changes."
                        );
                    }
                    Err(error) => {
                        can_execute_command = false;
                        workspace.message = format!("Quick action failed: {error}");
                    }
                }
            }
            if can_execute_command && let Some(command_value) = action.get("command") {
                let (command, arguments) = if let Some(command) = command_value.as_str() {
                    (
                        command,
                        action
                            .get("arguments")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!([])),
                    )
                } else if let Some(command) = command_value
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                {
                    (
                        command,
                        command_value
                            .get("arguments")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!([])),
                    )
                } else {
                    ("", serde_json::json!([]))
                };
                let title = action
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(command)
                    .to_owned();
                if let Some(id) = language_server.execute_command(command, arguments) {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            egui::Id::unique("cairn.lsp-pending-execute-command"),
                            (id, title.clone()),
                        )
                    });
                    workspace.message = format!("Running quick action: {title}");
                } else {
                    workspace.message =
                        "The language server could not run this quick action.".to_owned();
                }
            }
        }
    }
    let reference_locations = ui
        .ctx()
        .data(|data| data.get_temp::<ReferenceResults>(egui::Id::unique("cairn.lsp-references")));
    if let Some(results) = reference_locations {
        let references = &results.locations;
        let root = workspace
            .current
            .as_ref()
            .map(|project| project.path.clone());
        let mut open = true;
        let mut navigate = None;
        egui::Window::new("References")
            .default_width(460.0)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                let classified_usages = references
                    .iter()
                    .filter(|entry| entry.is_declaration == Some(false))
                    .count();
                let declaration_count = references
                    .iter()
                    .filter(|entry| entry.is_declaration == Some(true))
                    .count();
                if let Some(usage_count) = results.usage_count {
                    ui.label(format!(
                        "{} references · {usage_count} usages · {declaration_count} declarations",
                        references.len()
                    ));
                } else if declaration_count + classified_usages > 0 {
                    ui.label(format!(
                        "{} references · {} usages · {} declarations",
                        references.len(),
                        classified_usages,
                        declaration_count
                    ));
                } else {
                    ui.label(format!("{} references", references.len()));
                }
                if references.is_empty() {
                    ui.label("No usages found for this function.");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(480.0)
                        .show(ui, |ui| {
                            for reference in references {
                                let label = format!(
                                    "{}:{}:{}",
                                    project_relative_display(root.as_deref(), &reference.path),
                                    reference.line,
                                    reference.column
                                );
                                let kind = match reference.is_declaration {
                                    Some(true) => "Declaration",
                                    Some(false) => "Usage",
                                    None => "Reference",
                                };
                                ui.group(|ui| {
                                    if ui.link(format!("{kind}  {label}")).clicked() {
                                        navigate = Some((
                                            reference.path.clone(),
                                            reference.line,
                                            reference.column,
                                        ));
                                    }
                                    ui.label(
                                        RichText::new(reference_preview(
                                            workspace,
                                            &reference.path,
                                            reference.line,
                                        ))
                                        .monospace()
                                        .small()
                                        .color(theme::muted()),
                                    );
                                });
                            }
                        });
                }
            });
        if let (Some((path, line, column)), Some(root)) = (navigate, root.as_deref()) {
            if let Ok(relative) = path.strip_prefix(root) {
                navigate_to_project_location(ui.ctx(), workspace, relative, line, column);
                open = false;
            } else {
                workspace.message =
                    format!("Reference is outside the open project: {}", path.display());
            }
        }
        if !open {
            ui.ctx().data_mut(|data| {
                data.remove::<ReferenceResults>(egui::Id::unique("cairn.lsp-references"));
            });
        }
    }
    let call_results = ui.ctx().data(|data| {
        data.get_temp::<CallHierarchyResults>(egui::Id::unique("cairn.lsp-call-hierarchy"))
    });
    if let Some(results) = call_results {
        let root = workspace
            .current
            .as_ref()
            .map(|project| project.path.clone());
        let mut open = true;
        let mut navigate = None;
        egui::Window::new(match results.direction {
            CallHierarchyDirection::Incoming => "Incoming Calls",
            CallHierarchyDirection::Outgoing => "Outgoing Calls",
        })
        .default_width(540.0)
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} {}",
                    results.entries.len(),
                    if results.direction == CallHierarchyDirection::Incoming {
                        "callers"
                    } else {
                        "calls"
                    }
                ));
                for (label, direction) in [
                    ("Incoming Calls", CallHierarchyDirection::Incoming),
                    ("Outgoing Calls", CallHierarchyDirection::Outgoing),
                ] {
                    if ui.button(label).clicked() {
                        let request = match direction {
                            CallHierarchyDirection::Incoming => {
                                language_server.request_incoming_calls(results.item.clone())
                            }
                            CallHierarchyDirection::Outgoing => {
                                language_server.request_outgoing_calls(results.item.clone())
                            }
                        };
                        if let Some(id) = request {
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(
                                    egui::Id::unique("cairn.lsp-pending-call-hierarchy"),
                                    PendingCallHierarchy {
                                        id,
                                        direction,
                                        prepared_item: Some(results.item.clone()),
                                        name: results.name.clone(),
                                    },
                                )
                            });
                            workspace.message = "Loading call hierarchy…".to_owned();
                        }
                    }
                }
            });
            egui::ScrollArea::vertical()
                .max_height(480.0)
                .show(ui, |ui| {
                    for entry in &results.entries {
                        let detail = entry.detail.as_deref().unwrap_or("");
                        ui.group(|ui| {
                            if ui
                                .link(format!(
                                    "{}{}  {}:{}:{}",
                                    entry.name,
                                    if detail.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" — {detail}")
                                    },
                                    project_relative_display(root.as_deref(), &entry.path),
                                    entry.line,
                                    entry.column
                                ))
                                .clicked()
                            {
                                navigate = Some((entry.path.clone(), entry.line, entry.column));
                            }
                            ui.label(
                                RichText::new(reference_preview(
                                    workspace,
                                    &entry.path,
                                    entry.line,
                                ))
                                .monospace()
                                .small()
                                .color(theme::muted()),
                            );
                        });
                    }
                    if results.entries.is_empty() {
                        ui.label("No calls were reported by the language server.");
                    }
                });
        });
        if let (Some((path, line, column)), Some(root)) = (navigate, root.as_deref()) {
            if let Ok(relative) = path.strip_prefix(root) {
                navigate_to_project_location(ui.ctx(), workspace, relative, line, column);
            }
        }
        if !open {
            ui.ctx().data_mut(|data| {
                data.remove::<CallHierarchyResults>(egui::Id::unique("cairn.lsp-call-hierarchy"))
            });
        }
    }
    let server_diagnostics = language_server
        .diagnostics()
        .iter()
        .filter_map(|diagnostic| {
            let severity = match diagnostic.severity {
                1 => DiagnosticSeverity::Error,
                2 => DiagnosticSeverity::Warning,
                3 => DiagnosticSeverity::Information,
                4 => DiagnosticSeverity::Hint,
                _ => return None,
            };
            Some(Diagnostic {
                severity,
                message: diagnostic.message.clone(),
                file: Some(diagnostic.file.display().to_string()),
                line: Some(diagnostic.line),
                column: Some(diagnostic.column),
                end_line: Some(diagnostic.end_line),
                end_column: Some(diagnostic.end_column),
                source: diagnostic
                    .source
                    .clone()
                    .or_else(|| diagnostic.code.clone()),
            })
        })
        .collect::<Vec<_>>();
    let mut triggered: Vec<crate::settings::Action> = Vec::new();
    for binding in keymap.bindings.iter().chain(keymap.aliases.iter()) {
        if let Some(key) = binding.egui_key()
            && ui.input_mut(|input| input.consume_key(binding.modifiers(), key))
        {
            triggered.push(binding.action);
        }
    }
    for action in triggered {
        match action {
            crate::settings::Action::Save => {
                save_current(workspace, &formatting_settings, line_ending)
            }
            crate::settings::Action::ToggleOutputPanel => {
                editor_layout.output_visible = !editor_layout.output_visible;
            }
            crate::settings::Action::ToggleSidePanel => {
                editor_layout.sidebar_visible = !editor_layout.sidebar_visible;
            }
            crate::settings::Action::FindInFiles => {
                let search_id = egui::Id::unique("cairn.find-in-files-state");
                let mut state = ui
                    .ctx()
                    .data(|data| data.get_temp::<SearchPanelState>(search_id))
                    .unwrap_or_default();
                state.open = true;
                state.query.clear();
                state.results.clear();
                state.message = None;
                ui.ctx().data_mut(|data| data.insert_temp(search_id, state));
            }
            crate::settings::Action::GoToDeclaration => {
                request_or_navigate_definition(ui.ctx(), workspace, language_server);
            }
            crate::settings::Action::FormatDocument => {
                if let Err(error) = format_selected(workspace, &formatting_settings, line_ending) {
                    workspace.message = error;
                }
            }
            crate::settings::Action::DebugContinue => {
                let (debugger_active, paused, running) = {
                    let state = process.state();
                    let state = state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    (
                        state.debugger_active,
                        state.debug_location.is_some(),
                        state.running,
                    )
                };
                if debugger_active && paused {
                    let _ = process.debugger_command("-exec-continue".to_owned());
                } else if !running {
                    start_project_action(ui, process, workspace, ProcessAction::Debug);
                }
            }
            crate::settings::Action::DebugStop => {
                if process
                    .state()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .running
                {
                    process.cancel();
                }
            }
            crate::settings::Action::DebugRestart => {
                let (debugger_active, running) = {
                    let state = process.state();
                    let state = state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    (state.debugger_active, state.running)
                };
                if debugger_active {
                    if let Err(error) = process.restart() {
                        workspace.message = error;
                    }
                } else if !running {
                    start_project_action(ui, process, workspace, ProcessAction::Run);
                }
            }
            crate::settings::Action::DebugStepOver => {
                let _ = process.debugger_command("-exec-next".to_owned());
            }
            crate::settings::Action::DebugStepInto => {
                let (active, paused) = {
                    let state = process.state();
                    let state = state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    (state.debugger_active, state.debug_location.is_some())
                };
                if active && paused {
                    let _ = process.debugger_command("-exec-step".to_owned());
                } else if !active {
                    let fullscreen = ui
                        .ctx()
                        .input(|input| input.viewport().fullscreen.unwrap_or(false));
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
                }
            }
            crate::settings::Action::DebugStepOut => {
                let _ = process.debugger_command("-exec-finish".to_owned());
            }
            crate::settings::Action::OpenSettings => command = Some(EditorCmd::Settings),
            crate::settings::Action::OpenProject => {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    command = Some(EditorCmd::OpenProject(path));
                }
            }
            crate::settings::Action::OpenFile => {
                if workspace.current.is_none() {
                    workspace.message = "Open a project before opening a source file.".to_owned();
                } else if let Some(path) = rfd::FileDialog::new()
                    .set_directory(
                        workspace
                            .current
                            .as_ref()
                            .map(|project| project.path.as_path())
                            .unwrap_or_else(|| std::path::Path::new(".")),
                    )
                    .pick_file()
                {
                    let root = workspace.current.as_ref().map(|project| &project.path);
                    if let Some(root) = root {
                        if let Ok(relative) = path.strip_prefix(root) {
                            if let Err(error) = workspace.open_file(relative) {
                                workspace.message = error;
                            }
                        } else {
                            workspace.message =
                                "Choose a file inside the current project.".to_owned();
                        }
                    }
                }
            }
            crate::settings::Action::SaveAll => {
                save_all(workspace, &formatting_settings, line_ending)
            }
            crate::settings::Action::NextTab => {
                if !workspace.documents.is_empty() {
                    workspace.selected_document =
                        (workspace.selected_document + 1) % workspace.documents.len();
                }
            }
            crate::settings::Action::PreviousTab => {
                if !workspace.documents.is_empty() {
                    workspace.selected_document = if workspace.selected_document == 0 {
                        workspace.documents.len() - 1
                    } else {
                        workspace.selected_document - 1
                    };
                }
            }
            crate::settings::Action::ReopenClosedTab => {
                if let Err(error) = workspace.reopen_closed_document() {
                    workspace.message = error;
                }
            }
            crate::settings::Action::CloseAllTabs => {
                if workspace.documents.iter().any(|document| document.dirty) {
                    workspace.message =
                        "Some tabs have unsaved changes. Save them first.".to_owned();
                } else {
                    workspace.documents.clear();
                    workspace.selected_document = 0;
                }
            }
            crate::settings::Action::ToggleZenMode => {
                editor_layout.zen_mode = !editor_layout.zen_mode
            }
            crate::settings::Action::FocusExplorer => {
                editor_layout.sidebar_visible = true;
                editor_layout.sidebar_tab = SidebarTab::Project;
            }
            crate::settings::Action::FocusSourceControl => {
                editor_layout.sidebar_visible = true;
                editor_layout.sidebar_tab = SidebarTab::SourceControl;
            }
            crate::settings::Action::FocusBreakpoints => {
                editor_layout.sidebar_visible = true;
                editor_layout.sidebar_tab = SidebarTab::Breakpoints;
            }
            crate::settings::Action::FocusProblems => {
                editor_layout.output_visible = true;
                editor_layout.show_problems = true;
                editor_layout.show_debugger = false;
                editor_layout.show_variables = false;
            }
            crate::settings::Action::FocusOutput => {
                editor_layout.output_visible = true;
                editor_layout.show_problems = false;
                editor_layout.show_debugger = false;
                editor_layout.show_variables = false;
            }
            crate::settings::Action::FocusDebugger | crate::settings::Action::FocusDebugConsole => {
                editor_layout.output_visible = true;
                editor_layout.show_debugger = true;
                editor_layout.show_problems = false;
                editor_layout.show_variables = false;
            }
            crate::settings::Action::BuildProject => {
                start_project_action(ui, process, workspace, ProcessAction::Build)
            }
            crate::settings::Action::RunProject => {
                start_project_action(ui, process, workspace, ProcessAction::Run)
            }
            crate::settings::Action::ToggleBreakpoint => {
                toggle_current_breakpoint(ui.ctx(), workspace, process)
            }
            crate::settings::Action::RemoveAllBreakpoints => {
                for document in &mut workspace.documents {
                    document.breakpoints.clear();
                }
                if let Err(error) = workspace.persist_breakpoints() {
                    workspace.message = error;
                }
                if let Err(error) = process.sync_breakpoints(&[]) {
                    workspace.message = error;
                }
            }
            crate::settings::Action::AddWatch => {
                editor_layout.output_visible = true;
                editor_layout.show_variables = true;
                editor_layout.show_debugger = false;
                editor_layout.show_problems = false;
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::unique("debugger-focus-watch-input"), true)
                });
            }
            crate::settings::Action::QuickOpenFile => {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::unique("cairn.quick-open-file"), true);
                    data.insert_temp(egui::Id::unique("cairn.quick-open-file-focus"), true);
                });
            }
            crate::settings::Action::CloseWindow => {
                command = Some(EditorCmd::Quit);
            }
            crate::settings::Action::CloseTab => {
                if workspace.documents.is_empty() {
                    continue;
                }
                // Same rule as the tab close button: unsaved tabs are closed from the tab itself.
                let index = workspace.selected_document;
                if workspace
                    .documents
                    .get(index)
                    .is_some_and(|document| document.dirty)
                {
                    workspace.message =
                        "Save the file first, or use the tab's × to discard changes.".to_owned();
                } else if let Err(error) = workspace.close_document(index) {
                    workspace.message = error;
                }
            }
        }
    }
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::F12)) {
        request_references_at_cursor(ui.ctx(), workspace, language_server);
    }
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::F1)) {
        request_hover_at_cursor(ui.ctx(), workspace, language_server);
    }
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Period)) {
        request_code_actions_at_cursor(ui.ctx(), workspace, language_server);
    }

    egui::Panel::top("editor_menu")
        .exact_size(30.0)
        .frame(bar_frame(theme::bg_dark()))
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.label(RichText::new("FUGH").strong().color(theme::accent_hover()));
                ui.separator();
                ui.menu_button("Project", |ui| {
                    let project_path = workspace.current.as_ref().map(|project| {
                        (project.path.clone(), project.template)
                    });
                    let properties_enabled = project_path.as_ref().is_some_and(|(_, template)| {
                        *template != ProjectTemplate::VisualStudioCpp
                    });
                    if ui
                        .add_enabled(
                            properties_enabled,
                            egui::Button::new("Properties…"),
                        )
                        .on_disabled_hover_text(
                            "Project properties are currently available for CMake C++ and C# projects.",
                        )
                        .clicked()
                    {
                        open_project_properties(ui.ctx(), workspace);
                        ui.close();
                    }
                    if let Some((path, _)) = project_path {
                        ui.label(RichText::new(path.display().to_string()).small().color(theme::muted()));
                    } else {
                        ui.label(RichText::new("No project is open").small().color(theme::muted()));
                    }
                });
                ui.menu_button("File", |ui| {
                    if ui.button("New Project…").clicked() {
                        command = Some(EditorCmd::NewProject);
                        ui.close();
                    }
                    if ui.button("Open Project Folder…").clicked() {
                        ui.close();
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            command = Some(EditorCmd::OpenProject(path));
                        }
                    }
                    if ui
                        .button(format!(
                            "Save  {}",
                            shortcut_text(&keymap, crate::settings::Action::Save)
                        ))
                        .clicked()
                    {
                        save_current(workspace, &formatting_settings, line_ending);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Settings…").clicked() {
                        command = Some(EditorCmd::Settings);
                        ui.close();
                    }
                    if ui.button("Close Project (Start Window)").clicked() {
                        command = Some(EditorCmd::Home);
                        ui.close();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Quick Actions…   Ctrl+.").clicked() {
                        request_code_actions_at_cursor(ui.ctx(), workspace, language_server);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(format!(
                            "Format Document   {}",
                            shortcut_text(&keymap, crate::settings::Action::FormatDocument)
                        ))
                        .clicked()
                    {
                        if let Err(error) =
                            format_selected(workspace, &formatting_settings, line_ending)
                        {
                            workspace.message = error;
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(format!(
                            "Find in Files…   {}",
                            shortcut_text(&keymap, crate::settings::Action::FindInFiles)
                        ))
                        .clicked()
                    {
                        let search_id = egui::Id::unique("cairn.find-in-files-state");
                        let mut state = ui
                            .ctx()
                            .data(|data| data.get_temp::<SearchPanelState>(search_id))
                            .unwrap_or_default();
                        state.open = true;
                        ui.ctx().data_mut(|data| data.insert_temp(search_id, state));
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "Go to Indexed Declaration   {}",
                            shortcut_text(&keymap, crate::settings::Action::GoToDeclaration)
                        ))
                        .clicked()
                    {
                        request_or_navigate_definition(ui.ctx(), workspace, language_server);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            !workspace.documents.is_empty(),
                            egui::Button::new("Save All"),
                        )
                        .clicked()
                    {
                        save_all(workspace, &formatting_settings, line_ending);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            !workspace.documents.is_empty(),
                            egui::Button::new("Close All Tabs"),
                        )
                        .clicked()
                    {
                        if workspace.documents.iter().any(|document| document.dirty) {
                            workspace.message =
                                "Some tabs have unsaved changes. Save them first.".to_owned();
                        } else {
                            workspace.documents.clear();
                            workspace.selected_document = 0;
                        }
                        ui.close();
                    }
                });
                ui.menu_button("Navigate", |ui| {
                    if ui.button("Find All References   Shift+F12").clicked() {
                        request_references_at_cursor(ui.ctx(), workspace, language_server);
                        ui.close();
                    }
                    if ui.button("Incoming Calls").clicked() {
                        request_call_hierarchy_at_cursor(
                            ui.ctx(),
                            workspace,
                            language_server,
                            CallHierarchyDirection::Incoming,
                        );
                        ui.close();
                    }
                    if ui.button("Outgoing Calls").clicked() {
                        request_call_hierarchy_at_cursor(
                            ui.ctx(),
                            workspace,
                            language_server,
                            CallHierarchyDirection::Outgoing,
                        );
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.selectable_value(
                        &mut editor_layout.sidebar_tab,
                        SidebarTab::Project,
                        "Solution Explorer",
                    );
                    ui.selectable_value(
                        &mut editor_layout.sidebar_tab,
                        SidebarTab::Symbols,
                        "Project Symbols",
                    );
                    ui.selectable_value(
                        &mut editor_layout.sidebar_tab,
                        SidebarTab::Helper,
                        "Fugh Guide",
                    );
                    ui.separator();
                    ui.checkbox(
                        &mut editor_layout.sidebar_visible,
                        format!(
                            "Side panel   {}",
                            shortcut_text(&keymap, crate::settings::Action::ToggleSidePanel)
                        ),
                    );
                    ui.checkbox(
                        &mut editor_layout.output_visible,
                        format!(
                            "Output panel   {}",
                            shortcut_text(&keymap, crate::settings::Action::ToggleOutputPanel)
                        ),
                    );
                });
                ui.menu_button("Window", |ui| {
                    let has_tabs = !workspace.documents.is_empty();
                    if ui
                        .add_enabled(has_tabs, egui::Button::new("Next Tab"))
                        .clicked()
                    {
                        workspace.selected_document =
                            (workspace.selected_document + 1) % workspace.documents.len();
                        ui.close();
                    }
                    if ui
                        .add_enabled(has_tabs, egui::Button::new("Previous Tab"))
                        .clicked()
                    {
                        workspace.selected_document = if workspace.selected_document == 0 {
                            workspace.documents.len() - 1
                        } else {
                            workspace.selected_document - 1
                        };
                        ui.close();
                    }
                    if ui
                        .add_enabled(has_tabs, egui::Button::new("Close Tab"))
                        .clicked()
                    {
                        close_tabs(workspace, vec![workspace.selected_document]);
                        ui.close();
                    }
                    let can_reopen = !workspace.closed_documents.is_empty();
                    if ui
                        .add_enabled(can_reopen, egui::Button::new("Reopen Closed Tab"))
                        .clicked()
                    {
                        if let Err(error) = workspace.reopen_closed_document() {
                            workspace.message = error;
                        }
                        ui.close();
                    }
                    if has_tabs {
                        let active = workspace.selected_document;
                        let pinned = workspace.documents[active].pinned;
                        if ui
                            .button(if pinned { "Unpin Tab" } else { "Pin Tab" })
                            .clicked()
                        {
                            workspace.documents[active].pinned = !pinned;
                            ui.close();
                        }
                    }
                    if ui
                        .add_enabled(
                            workspace.documents.len() > 1,
                            egui::Button::new("Close Other Tabs"),
                        )
                        .clicked()
                    {
                        let selected = workspace.selected_document;
                        let close = (0..workspace.documents.len())
                            .filter(|index| {
                                *index != selected && !workspace.documents[*index].pinned
                            })
                            .collect();
                        close_tabs(workspace, close);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            workspace.selected_document + 1 < workspace.documents.len(),
                            egui::Button::new("Close Tabs to the Right"),
                        )
                        .clicked()
                    {
                        close_tabs(
                            workspace,
                            (workspace.selected_document + 1..workspace.documents.len())
                                .filter(|index| !workspace.documents[*index].pinned)
                                .collect(),
                        );
                        ui.close();
                    }
                    if ui
                        .add_enabled(has_tabs, egui::Button::new("Close All Tabs"))
                        .clicked()
                    {
                        close_tabs(workspace, (0..workspace.documents.len()).collect());
                        ui.close();
                    }
                    ui.separator();
                    ui.checkbox(&mut editor_layout.sidebar_visible, "Show/Hide Sidebar");
                    ui.checkbox(&mut editor_layout.output_visible, "Show/Hide Bottom Panel");
                    ui.checkbox(&mut editor_layout.zen_mode, "Toggle Zen Mode");
                    ui.menu_button("Dock Sidebar", |ui| {
                        for (side, label) in [
                            (DockSide::Left, "Left"),
                            (DockSide::Right, "Right"),
                            (DockSide::Top, "Top"),
                            (DockSide::Bottom, "Bottom"),
                        ] {
                            if ui
                                .selectable_label(editor_layout.sidebar_side == side, label)
                                .clicked()
                            {
                                editor_layout.dock(DockItem::Sidebar, side);
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button("Dock Bottom Panel", |ui| {
                        for (side, label) in [
                            (DockSide::Left, "Left"),
                            (DockSide::Right, "Right"),
                            (DockSide::Top, "Top"),
                            (DockSide::Bottom, "Bottom"),
                        ] {
                            if ui
                                .selectable_label(editor_layout.output_side == side, label)
                                .clicked()
                            {
                                editor_layout.dock(DockItem::Output, side);
                                ui.close();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Layout name");
                        ui.add(
                            egui::TextEdit::singleline(&mut editor_layout.layout_name)
                                .desired_width(120.0)
                                .hint_text("Work layout"),
                        );
                        if ui.button("Save Current Layout").clicked() {
                            match editor_layout.save_named_layout() {
                                Ok(()) => workspace.message = "Window layout saved".to_owned(),
                                Err(error) => workspace.message = error,
                            }
                            ui.close();
                        }
                    });
                    if !editor_layout.saved_layouts.is_empty() {
                        ui.menu_button("Saved Layouts", |ui| {
                            for index in 0..editor_layout.saved_layouts.len() {
                                ui.horizontal(|ui| {
                                    let saved_name =
                                        editor_layout.saved_layouts[index].name.clone();
                                    if ui.button(format!("Load: {saved_name}")).clicked() {
                                        if let Err(error) = editor_layout.load_named_layout(index) {
                                            workspace.message = error;
                                        }
                                        ui.close();
                                    }
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut editor_layout.saved_layouts[index].name,
                                        )
                                        .desired_width(110.0),
                                    )
                                    .on_hover_text("Rename saved layout");
                                    if ui
                                        .small_button("×")
                                        .on_hover_text("Delete saved layout")
                                        .clicked()
                                    {
                                        editor_layout.saved_layouts.remove(index);
                                        ui.close();
                                    }
                                });
                            }
                        });
                    }
                    ui.separator();
                    if ui.button("Toggle Full Screen").clicked() {
                        let fullscreen = ui
                            .ctx()
                            .input(|input| input.viewport().fullscreen.unwrap_or(false));
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
                        ui.close();
                    }
                    if ui.button("Reset Window Layout").clicked() {
                        editor_layout.reset_to_default();
                        ui.close();
                    }
                });
                ui.menu_button("Build", |ui| {
                    project_action_button(ui, process, workspace, ProcessAction::Build);
                    project_action_button(ui, process, workspace, ProcessAction::BuildRelease);
                    project_action_button(ui, process, workspace, ProcessAction::Rebuild);
                    project_action_button(ui, process, workspace, ProcessAction::Clean);
                    ui.separator();
                    project_action_button(ui, process, workspace, ProcessAction::Analyze);
                    project_action_button(ui, process, workspace, ProcessAction::Publish);
                    project_action_button(ui, process, workspace, ProcessAction::Package);
                    ui.separator();
                    project_action_button(ui, process, workspace, ProcessAction::Run);
                    project_action_button(ui, process, workspace, ProcessAction::Debug);
                    let process_state = process.state();
                    let (running, stopping) = {
                        let state = process_state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        (state.running, state.stopping)
                    };
                    if ui
                        .add_enabled(
                            running && !stopping,
                            egui::Button::new(if stopping {
                                "Stopping…"
                            } else {
                                "Stop Process"
                            }),
                        )
                        .clicked()
                    {
                        process.cancel();
                        ui.close();
                    }
                    if ui
                        .add_enabled(!running, egui::Button::new("Restart Process"))
                        .clicked()
                    {
                        if let Err(error) = process.restart() {
                            workspace.message = error;
                        }
                        ui.close();
                    }
                });
                ui.menu_button("Help", |ui| {
                    ui.label(RichText::new("Keyboard shortcuts").strong());
                    for action in crate::settings::Action::ALL {
                        ui.label(
                            RichText::new(format!(
                                "{}   {}",
                                shortcut_text(&keymap, action),
                                action.label()
                            ))
                            .monospace()
                            .small(),
                        );
                    }
                    for line in [
                        "Tab / Enter   Accept completion",
                        "Esc   Dismiss completion",
                        "Ctrl+.   Show language-server quick actions",
                        "Shift+F12   Find all references at the cursor",
                        "Shift+F1   Show symbol information",
                        "F2   Rename symbol across the project",
                    ] {
                        ui.label(RichText::new(line).monospace().small());
                    }
                });
            });
        });

    show_project_properties_window(ui.ctx(), workspace);

    if live_settings.appearance.toolbar_visible {
        egui::Panel::top("editor_toolbar")
            .exact_size(40.0)
            .frame(bar_frame(theme::bg()))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let label = workspace
                        .current
                        .as_ref()
                        .map(|p| (p.name.clone(), p.path.display().to_string()));
                    match label {
                        Some((name, path)) => {
                            egui::Frame::new()
                                .fill(theme::accent())
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::symmetric(12, 3))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(name).strong().color(Color32::WHITE));
                                });
                            ui.label(RichText::new(path).small().color(theme::muted()));
                        }
                        None => {
                            ui.label(RichText::new("No project open").color(theme::muted()));
                        }
                    }
                    ui.separator();
                    build_toolbar_settings_ui(ui, workspace);
                    ui.separator();
                    if workspace.current.as_ref().is_some_and(|project| {
                        project.template == ProjectTemplate::AvaloniaApp
                            && ToolchainManager::supports_fedora_package_management()
                    }) {
                        ui.selectable_value(&mut editor_layout.designer_mode, false, "Code");
                        if ui
                            .selectable_value(
                                &mut editor_layout.designer_mode,
                                true,
                                "Window Designer",
                            )
                            .clicked()
                            && workspace
                                .active_document()
                                .is_some_and(|document| document.dirty)
                        {
                            save_current(workspace, &formatting_settings, line_ending);
                        }
                        ui.separator();
                    }
                    project_action_button(ui, process, workspace, ProcessAction::Run);
                    project_action_button(ui, process, workspace, ProcessAction::Build);
                    project_action_button(ui, process, workspace, ProcessAction::Debug);
                    if ui.button("Save").clicked() {
                        save_current(workspace, &formatting_settings, line_ending);
                    }
                });
            });
    }

    if live_settings.appearance.status_bar_visible {
        egui::Panel::bottom("editor_status")
            .exact_size(24.0)
            .frame(bar_frame(theme::accent()))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new(format!("●  {}", workspace.message))
                            .small()
                            .color(Color32::WHITE),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(document) = workspace.active_document() {
                            ui.label(
                                RichText::new(format!(
                                    "{} lines   {}",
                                    document.text.lines().count(),
                                    language_for_document(document)
                                ))
                                .small()
                                .color(Color32::WHITE),
                            );
                        }
                    });
                });
            });
    }

    let debugging_active = {
        let state = process.state();
        let state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.debugger_active && state.running
    };
    let sidebar_backup_id = egui::Id::unique("cairn.debug-sidebar-tab-backup");
    if debugging_active && editor_layout.sidebar_tab == SidebarTab::Helper {
        ui.ctx().data_mut(|data| {
            data.insert_temp(sidebar_backup_id, Some(SidebarTab::Helper));
        });
        editor_layout.sidebar_tab = SidebarTab::Project;
    } else if !debugging_active {
        let previous = ui.ctx().data_mut(|data| {
            data.get_temp::<Option<SidebarTab>>(sidebar_backup_id)
                .flatten()
        });
        if previous == Some(SidebarTab::Helper) && editor_layout.sidebar_tab == SidebarTab::Project
        {
            editor_layout.sidebar_tab = SidebarTab::Helper;
        }
        ui.ctx()
            .data_mut(|data| data.remove::<Option<SidebarTab>>(sidebar_backup_id));
    }
    if !editor_layout.zen_mode {
        // egui lays dock panels out in declaration order. Reserve top/bottom
        // space before left/right space so a bottom output panel ends at the
        // edge of the sidebar instead of painting underneath it.
        let panel_order = |side| match side {
            DockSide::Top => 0,
            DockSide::Bottom => 1,
            DockSide::Left => 2,
            DockSide::Right => 3,
        };
        if panel_order(editor_layout.output_side) < panel_order(editor_layout.sidebar_side) {
            if editor_layout.output_visible {
                show_output_panel(ui, workspace, editor_layout, process, &server_diagnostics);
            }
            if editor_layout.sidebar_visible {
                show_sidebar_panel(ui, workspace, editor_layout, process);
            }
        } else {
            if editor_layout.sidebar_visible {
                show_sidebar_panel(ui, workspace, editor_layout, process);
            }
            if editor_layout.output_visible {
                show_output_panel(ui, workspace, editor_layout, process, &server_diagnostics);
            }
        }
    }
    show_new_item_dialog(ui, workspace);
    show_new_folder_dialog(ui, workspace);
    show_delete_confirmation(ui, workspace);

    let central_response = egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::editor()))
        .show(ui, |ui| {
            let (_body, dropped_item) = ui.dnd_drop_zone::<DockItem, _>(egui::Frame::new(), |ui| {
                // Fill the whole central area; without this the frame shrinks to its content.
                ui.set_min_size(ui.available_size());
                let has_unsaved_xaml = workspace.current.as_ref().is_some_and(|project| {
                    let xaml_path = project.path.join("MainWindow.axaml");
                    project.template == ProjectTemplate::AvaloniaApp
                        && !designer.has_unsaved_changes()
                        && workspace.documents.iter().any(|document| {
                            document.path.as_deref() == Some(xaml_path.as_path()) && document.dirty
                        })
                });
                if editor_layout.designer_mode && has_unsaved_xaml {
                    ui.vertical_centered(|ui| {
                        ui.add_space(36.0);
                        ui.heading("Save the XAML before opening the designer");
                        ui.label("The visual designer will not replace unsaved changes in MainWindow.axaml.");
                        if ui.button("Return to Code").clicked() {
                            editor_layout.designer_mode = false;
                        }
                    });
                } else if editor_layout.designer_mode
                    && let Some(project) = workspace.current.clone()
                    && project.template == ProjectTemplate::AvaloniaApp
                    && ToolchainManager::supports_fedora_package_management()
                {
                    if let Some(code_path) = designer.show(ui, &project) {
                        if let Some(handler) = designer.take_open_code_handler() {
                            let full_path = project.path.join(&code_path);
                            let canonical_path = full_path.canonicalize().unwrap_or(full_path);
                            if let Some(document) = workspace.documents.iter_mut().find(|document| {
                                document.path.as_deref() == Some(canonical_path.as_path())
                                    && document.dirty
                            }) {
                                match super::designer::merge_generated_handler(
                                    &mut document.text,
                                    &handler,
                                ) {
                                    Ok(true) => document.dirty = true,
                                    Ok(false) => {}
                                    Err(error) => workspace.message = error,
                                }
                            }
                        }
                        editor_layout.designer_mode = false;
                        if let Err(error) = workspace.open_file(&code_path) {
                            workspace.message = error;
                        }
                    } else {
                        let xaml_path = project.path.join("MainWindow.axaml");
                        let pending = designer.has_unsaved_changes();
                        if pending
                            && !workspace.documents.iter().any(|document| {
                                document.path.as_deref() == Some(xaml_path.as_path())
                            })
                            && let Err(error) = workspace.open_file(std::path::Path::new("MainWindow.axaml"))
                        {
                            workspace.message = error;
                        }
                        if let Some(document) = workspace
                            .documents
                            .iter_mut()
                            .find(|document| document.path.as_deref() == Some(xaml_path.as_path()))
                        {
                            let text = designer.current_xaml(&project);
                            if document.text != text || document.dirty != pending {
                                document.text = text;
                                document.dirty = pending;
                            }
                        }
                    }
                } else {
                    show_code_editor(ui, workspace, process, language_server);
                }
            });
            if egui::DragAndDrop::has_any_payload(ui.ctx()) {
                ui.label(
                    RichText::new("Drop near an editor edge to dock this panel.")
                        .small()
                        .color(Color32::LIGHT_BLUE),
                );
            }
            dropped_item
        });
    if let (Some(item), Some(pointer)) = (
        central_response.inner,
        ui.input(|input| input.pointer.interact_pos()),
    ) {
        let rect = central_response.response.rect;
        let x_fraction = (pointer.x - rect.left()) / rect.width().max(1.0);
        let y_fraction = (pointer.y - rect.top()) / rect.height().max(1.0);
        let side = if y_fraction < 0.22 {
            DockSide::Top
        } else if y_fraction > 0.78 {
            DockSide::Bottom
        } else if x_fraction < 0.22 {
            DockSide::Left
        } else {
            DockSide::Right
        };
        editor_layout.dock(*item, side);
    }
    draw_find_in_files(ui, workspace);
    draw_quick_open_file(ui, workspace);
    command
}

fn open_project_properties(ctx: &egui::Context, workspace: &Workspace) {
    let Some(project) = workspace.current.as_ref() else {
        return;
    };
    let draft = ProjectPropertiesDraft {
        project_path: project.path.clone(),
        properties: project.properties.clone(),
        tab: match project.language {
            ProjectLanguage::Cpp => ProjectPropertiesTab::Cpp,
            ProjectLanguage::CSharp => ProjectPropertiesTab::CSharp,
        },
        open: true,
        error: None,
    };
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::unique("fugh.project-properties-dialog"), draft);
    });
}

fn show_project_properties_window(ctx: &egui::Context, workspace: &mut Workspace) {
    let id = egui::Id::unique("fugh.project-properties-dialog");
    let Some(mut draft) = ctx.data(|data| data.get_temp::<ProjectPropertiesDraft>(id)) else {
        return;
    };
    let Some(project) = workspace.current.as_ref() else {
        draft.open = false;
        ctx.data_mut(|data| data.remove::<ProjectPropertiesDraft>(id));
        return;
    };
    if project.path != draft.project_path {
        ctx.data_mut(|data| data.remove::<ProjectPropertiesDraft>(id));
        return;
    }

    let mut save = false;
    let mut cancel = false;
    egui::Window::new(format!("{} — Project Properties", project.name))
        .id(id)
        .open(&mut draft.open)
        .resizable(true)
        .default_size([720.0, 650.0])
        .show(ctx, |ui| {
            ui.label(
                RichText::new(draft.project_path.display().to_string())
                    .small()
                    .color(theme::muted()),
            );
            ui.horizontal(|ui| {
                ui.selectable_value(&mut draft.tab, ProjectPropertiesTab::Cpp, "C++");
                ui.selectable_value(&mut draft.tab, ProjectPropertiesTab::CSharp, "C#");
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("fugh.project-properties-content")
                .show(ui, |ui| match draft.tab {
                    ProjectPropertiesTab::Cpp => {
                        cpp_project_properties_ui(
                            ui,
                            &mut draft.properties.cpp,
                            &draft.project_path,
                        );
                    }
                    ProjectPropertiesTab::CSharp => {
                        csharp_project_properties_ui(
                            ui,
                            &mut draft.properties.csharp,
                            &draft.project_path,
                        );
                    }
                });
            ui.separator();
            if let Some(error) = &draft.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                ui.label(
                    RichText::new("Saved to this project; settings apply on the next build.")
                        .small()
                        .color(theme::muted()),
                );
            });
        });

    if save {
        match validate_project_properties(&draft.properties) {
            Ok(()) => {
                let previous = workspace
                    .current
                    .as_ref()
                    .map(|project| project.properties.clone());
                if let Some(project) = workspace.current.as_mut() {
                    project.properties = draft.properties.clone();
                }
                match workspace.save_current_project_properties() {
                    Ok(()) => draft.open = false,
                    Err(error) => {
                        if let (Some(project), Some(previous)) =
                            (workspace.current.as_mut(), previous)
                        {
                            project.properties = previous;
                        }
                        draft.error = Some(error);
                    }
                }
            }
            Err(error) => draft.error = Some(error),
        }
    }
    if cancel || !draft.open {
        ctx.data_mut(|data| data.remove::<ProjectPropertiesDraft>(id));
    } else {
        ctx.data_mut(|data| data.insert_temp(id, draft));
    }
}

fn cpp_project_properties_ui(
    ui: &mut egui::Ui,
    properties: &mut CppProjectProperties,
    project_root: &std::path::Path,
) {
    ui.heading("C++ build properties");
    ui.label("CMake applies these settings during the next configure/build. Relative paths use the project folder.");
    egui::ComboBox::from_label("C++ language standard")
        .selected_text(if properties.standard.is_empty() {
            "Use CMakeLists.txt"
        } else {
            properties.standard.as_str()
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut properties.standard,
                String::new(),
                "Use CMakeLists.txt",
            );
            for standard in ["14", "17", "20", "23", "26"] {
                ui.selectable_value(
                    &mut properties.standard,
                    standard.to_owned(),
                    format!("C++{standard}"),
                );
            }
        });

    ui.separator();
    path_list_editor(
        ui,
        "Additional include directories",
        &mut properties.include_directories,
        project_root,
    );
    path_list_editor(
        ui,
        "Library directories",
        &mut properties.library_directories,
        project_root,
    );
    string_list_editor(
        ui,
        "Libraries to link",
        &mut properties.libraries,
        "Library name (e.g. ssl) or full .a/.so/.lib path",
    );
    string_list_editor(
        ui,
        "Preprocessor definitions",
        &mut properties.definitions,
        "One definition per item (e.g. APP_FEATURE=1)",
    );
    string_list_editor(
        ui,
        "Additional compiler options",
        &mut properties.compiler_options,
        "One compiler argument per item (e.g. -Wall)",
    );
    string_list_editor(
        ui,
        "Additional linker options",
        &mut properties.linker_options,
        "One linker argument per item",
    );
    path_list_editor(
        ui,
        "Runtime library directories (DLL/.so search path)",
        &mut properties.runtime_library_directories,
        project_root,
    );
    file_path_list_editor(
        ui,
        "Runtime DLL/shared-library files (copied beside executables)",
        &mut properties.runtime_files,
        project_root,
        "Runtime libraries",
        &["dll", "so", "dylib"],
    );
}

fn csharp_project_properties_ui(
    ui: &mut egui::Ui,
    properties: &mut CSharpProjectProperties,
    project_root: &std::path::Path,
) {
    ui.heading("C# / .NET build properties");
    ui.label("Blank values inherit from the .csproj. These overrides are passed to MSBuild for build, run, and debug.");
    project_property_text(
        ui,
        "Target framework",
        &mut properties.target_framework,
        "e.g. net10.0",
    );
    project_property_text(
        ui,
        "Runtime identifier",
        &mut properties.runtime_identifier,
        "e.g. linux-x64; blank uses the SDK default",
    );
    project_property_text(
        ui,
        "Assembly name",
        &mut properties.assembly_name,
        "Blank uses the project name",
    );
    project_property_text(
        ui,
        "Root namespace",
        &mut properties.root_namespace,
        "Blank uses the .csproj default",
    );
    project_property_text(
        ui,
        "C# language version",
        &mut properties.language_version,
        "e.g. 12.0, preview, or blank",
    );
    project_property_choice(
        ui,
        "Nullable context",
        &mut properties.nullable,
        &[
            ("", "Use project file"),
            ("enable", "Enable"),
            ("disable", "Disable"),
            ("warnings", "Warnings only"),
            ("annotations", "Annotations only"),
        ],
    );
    project_property_choice(
        ui,
        "Implicit usings",
        &mut properties.implicit_usings,
        &[
            ("", "Use project file"),
            ("enable", "Enable"),
            ("disable", "Disable"),
        ],
    );
    project_property_choice(
        ui,
        "Treat warnings as errors",
        &mut properties.treat_warnings_as_errors,
        &[("", "Use project file"), ("true", "Yes"), ("false", "No")],
    );
    ui.separator();
    nuget_package_list_editor(ui, &mut properties.nuget_packages);
    file_path_list_editor(
        ui,
        "Managed assembly references (.dll)",
        &mut properties.assembly_references,
        project_root,
        ".NET assemblies",
        &["dll"],
    );
    ui.label("Package references restore on the next build. Native libraries and platform-specific settings remain managed in the .csproj or through the C++ runtime library paths.");
}

fn project_property_text(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(290.0)
                .hint_text(hint),
        );
    });
}

fn project_property_choice(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    choices: &[(&str, &str)],
) {
    egui::ComboBox::from_label(label)
        .selected_text(
            choices
                .iter()
                .find(|(choice, _)| *choice == value.as_str())
                .map_or("Use project file", |(_, label)| label),
        )
        .show_ui(ui, |ui| {
            for (choice, label) in choices {
                ui.selectable_value(value, (*choice).to_owned(), *label);
            }
        });
}

fn string_list_editor(ui: &mut egui::Ui, label: &str, values: &mut Vec<String>, hint: &str) {
    ui.collapsing(label, |ui| {
        let mut remove = None;
        for (index, value) in values.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(470.0)
                        .hint_text(hint),
                );
                if ui.small_button("×").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            values.remove(index);
        }
        if ui.small_button("Add item").clicked() {
            values.push(String::new());
        }
    });
}

fn path_list_editor(
    ui: &mut egui::Ui,
    label: &str,
    values: &mut Vec<String>,
    project_root: &std::path::Path,
) {
    ui.collapsing(label, |ui| {
        let mut remove = None;
        for (index, value) in values.iter_mut().enumerate() {
            let mut browse = false;
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(420.0)
                        .hint_text("Project-relative or absolute path"),
                );
                if ui.small_button("Browse…").clicked() {
                    browse = true;
                }
                if ui.small_button("×").clicked() {
                    remove = Some(index);
                }
            });
            if browse
                && let Some(path) = rfd::FileDialog::new()
                    .set_directory(project_root)
                    .pick_folder()
            {
                *value = path.strip_prefix(project_root).map_or_else(
                    |_| path.display().to_string(),
                    |relative| relative.display().to_string(),
                );
            }
        }
        if let Some(index) = remove {
            values.remove(index);
        }
        if ui.small_button("Add directory").clicked() {
            values.push(String::new());
        }
    });
}

fn file_path_list_editor(
    ui: &mut egui::Ui,
    label: &str,
    values: &mut Vec<String>,
    project_root: &std::path::Path,
    filter_name: &str,
    extensions: &[&str],
) {
    ui.collapsing(label, |ui| {
        let mut remove = None;
        for (index, value) in values.iter_mut().enumerate() {
            let mut browse = false;
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(420.0)
                        .hint_text("Project-relative or absolute .dll path"),
                );
                if ui.small_button("Browse…").clicked() {
                    browse = true;
                }
                if ui.small_button("×").clicked() {
                    remove = Some(index);
                }
            });
            if browse
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter(filter_name, extensions)
                    .set_directory(project_root)
                    .pick_file()
            {
                *value = path.strip_prefix(project_root).map_or_else(
                    |_| path.display().to_string(),
                    |relative| relative.display().to_string(),
                );
            }
        }
        if let Some(index) = remove {
            values.remove(index);
        }
        if ui.small_button("Add assembly").clicked() {
            values.push(String::new());
        }
    });
}

fn nuget_package_list_editor(ui: &mut egui::Ui, packages: &mut Vec<NuGetPackageReference>) {
    ui.collapsing("NuGet package references", |ui| {
        let mut remove = None;
        for (index, package) in packages.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut package.id)
                        .desired_width(260.0)
                        .hint_text("Package ID"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut package.version)
                        .desired_width(150.0)
                        .hint_text("Version (optional)"),
                );
                if ui.small_button("×").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            packages.remove(index);
        }
        if ui.small_button("Add NuGet package").clicked() {
            packages.push(NuGetPackageReference::default());
        }
    });
}

fn validate_project_properties(properties: &ProjectProperties) -> Result<(), String> {
    if !properties.cpp.standard.trim().is_empty()
        && !matches!(
            properties.cpp.standard.trim(),
            "14" | "17" | "20" | "23" | "26"
        )
    {
        return Err(
            "Choose a supported C++ standard or use the value from CMakeLists.txt.".to_owned(),
        );
    }
    if !properties.csharp.nullable.is_empty()
        && !["enable", "disable", "warnings", "annotations"]
            .contains(&properties.csharp.nullable.as_str())
    {
        return Err("Choose a valid Nullable context value.".to_owned());
    }
    if !properties.csharp.implicit_usings.is_empty()
        && !["enable", "disable"].contains(&properties.csharp.implicit_usings.as_str())
    {
        return Err("Choose Enable, Disable, or Use project file for implicit usings.".to_owned());
    }
    if !properties.csharp.treat_warnings_as_errors.is_empty()
        && !["true", "false"].contains(&properties.csharp.treat_warnings_as_errors.as_str())
    {
        return Err("Choose Yes, No, or Use project file for warnings as errors.".to_owned());
    }
    let values = properties
        .cpp
        .include_directories
        .iter()
        .chain(&properties.cpp.library_directories)
        .chain(&properties.cpp.libraries)
        .chain(&properties.cpp.definitions)
        .chain(&properties.cpp.compiler_options)
        .chain(&properties.cpp.linker_options)
        .chain(&properties.cpp.runtime_library_directories)
        .chain(&properties.cpp.runtime_files)
        .chain([
            &properties.csharp.target_framework,
            &properties.csharp.runtime_identifier,
            &properties.csharp.assembly_name,
            &properties.csharp.root_namespace,
            &properties.csharp.language_version,
        ]);
    if values.into_iter().any(|value| {
        value
            .chars()
            .any(|character| matches!(character, '\n' | '\r' | '\0'))
    }) || properties
        .csharp
        .nuget_packages
        .iter()
        .flat_map(|package| [&package.id, &package.version])
        .chain(&properties.csharp.assembly_references)
        .any(|value| {
            value
                .chars()
                .any(|character| matches!(character, '\n' | '\r' | '\0'))
        })
    {
        return Err(
            "Project property values cannot contain line breaks or null characters.".to_owned(),
        );
    }
    Ok(())
}

fn show_sidebar_panel(
    ui: &mut egui::Ui,
    workspace: &mut Workspace,
    layout: &mut EditorLayout,
    process: &ProcessHandle,
) {
    let side = layout.sidebar_side;
    let mut show = |ui: &mut egui::Ui| {
        let process_state = process.state();
        let (running, runtime_pid, active_action, last_result) = {
            let state = process_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                state.running,
                state.runtime_pid,
                state.active_action,
                state.last_result.clone(),
            )
        };
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.dnd_drag_source(
                    egui::Id::unique("editor-sidebar-drag-handle"),
                    DockItem::Sidebar,
                    |ui| ui.label(RichText::new("⠿").color(Color32::LIGHT_BLUE)),
                );
                ui.selectable_value(&mut layout.sidebar_tab, SidebarTab::Project, "Project");
                ui.selectable_value(&mut layout.sidebar_tab, SidebarTab::Symbols, "Symbols");
                ui.selectable_value(&mut layout.sidebar_tab, SidebarTab::Outline, "Outline");
                ui.menu_button("More", |ui| {
                    ui.selectable_value(&mut layout.sidebar_tab, SidebarTab::Search, "Search");
                    ui.selectable_value(
                        &mut layout.sidebar_tab,
                        SidebarTab::Breadcrumbs,
                        "Breadcrumbs",
                    );
                    ui.selectable_value(
                        &mut layout.sidebar_tab,
                        SidebarTab::SourceControl,
                        "Source Control",
                    );
                    ui.selectable_value(
                        &mut layout.sidebar_tab,
                        SidebarTab::Breakpoints,
                        "Breakpoints",
                    );
                    ui.selectable_value(&mut layout.sidebar_tab, SidebarTab::Helper, "Help");
                });
                ui.selectable_value(&mut layout.sidebar_tab, SidebarTab::Process, "Tasks");
            });
            ui.separator();
            match layout.sidebar_tab {
                SidebarTab::Project => {
                    ui.label(
                        RichText::new("SOLUTION EXPLORER")
                            .small()
                            .strong()
                            .color(Color32::from_rgb(155, 173, 197)),
                    );
                    show_project_tree(ui, workspace);
                    ui.separator();
                    show_project_properties(ui, workspace);
                }
                SidebarTab::Search => show_search_panel(ui),
                SidebarTab::Symbols => show_symbol_list(ui, workspace),
                SidebarTab::Outline => show_active_file_outline(ui, workspace),
                SidebarTab::Breadcrumbs => show_breadcrumbs(ui, workspace),
                SidebarTab::SourceControl => show_source_control(ui, workspace),
                SidebarTab::Breakpoints => show_breakpoints(ui, workspace),
                SidebarTab::Helper => show_helper_panel(ui, workspace, process),
                SidebarTab::Process => {
                    show_process_monitor(ui, running, active_action, runtime_pid, last_result)
                }
            }
        });
    };

    match side {
        DockSide::Left => egui::Panel::left("editor_sidebar")
            .resizable(true)
            .default_size(285.0)
            .show(ui, |ui| show(ui)),
        DockSide::Right => egui::Panel::right("editor_sidebar")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| show(ui)),
        DockSide::Top => egui::Panel::top("editor_sidebar")
            .resizable(true)
            .default_size(230.0)
            .show(ui, |ui| show(ui)),
        DockSide::Bottom => egui::Panel::bottom("editor_sidebar")
            .resizable(true)
            .default_size(230.0)
            .show(ui, |ui| show(ui)),
    };
}

fn show_process_monitor(
    ui: &mut egui::Ui,
    running: bool,
    active_action: Option<ProcessAction>,
    runtime_pid: Option<u32>,
    last_result: Option<Result<(), String>>,
) {
    ui.label(
        RichText::new("TASKS AND PROCESSES")
            .small()
            .strong()
            .color(theme::muted()),
    );
    let Some(pid) = runtime_pid else {
        if running {
            let action = active_action.map_or("Task", ProcessAction::label);
            ui.spinner();
            ui.label(format!(
                "{action} is running. Detailed process metrics will appear when its program starts."
            ));
        } else if let Some(Err(error)) = last_result {
            ui.colored_label(Color32::LIGHT_RED, format!("Last task failed: {error}"));
        } else if matches!(last_result, Some(Ok(()))) {
            ui.label("Last task completed successfully.");
        } else {
            ui.label("No build or program process is active. Build history is in Output.");
        }
        return;
    };
    #[cfg(target_os = "linux")]
    {
        let Some((cpu_percent, memory_bytes, total_memory)) = linux_process_metrics(ui.ctx(), pid)
        else {
            ui.label(format!("Process {pid} is starting or has exited."));
            return;
        };
        ui.label(format!("PID  {pid}"));
        ui.add_space(8.0);
        ui.label("CPU");
        ui.add(
            egui::ProgressBar::new((cpu_percent / 100.0).clamp(0.0, 1.0))
                .text(format!("{cpu_percent:.1}%")),
        );
        ui.add_space(6.0);
        ui.label("Memory");
        let memory_percent = memory_bytes as f32 / total_memory.max(1) as f32;
        ui.add(
            egui::ProgressBar::new(memory_percent.clamp(0.0, 1.0))
                .text(format!("{} MiB", memory_bytes / (1024 * 1024))),
        );
        ui.add_space(10.0);
        ui.label(
            RichText::new("Live usage of the launched program")
                .small()
                .color(theme::muted()),
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = ui;
        ui.label(format!(
            "Process {pid} is running. Resource sampling is currently available on Linux."
        ));
    }
}

#[cfg(target_os = "linux")]
fn linux_process_metrics(ctx: &egui::Context, pid: u32) -> Option<(f32, u64, u64)> {
    use std::time::Instant;

    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields = stat
        .rsplit_once(')')?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    let cpu_ticks = fields
        .get(11)?
        .parse::<u64>()
        .ok()?
        .saturating_add(fields.get(12)?.parse::<u64>().ok()?);
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let memory_kib = status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
    })?;
    let total_memory_kib = std::fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemTotal:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })?;
    let now = Instant::now();
    let sample_id = egui::Id::unique("cairn.process-monitor-sample").with(pid);
    let previous = ctx.data_mut(|data| {
        let previous = data.get_temp::<(u64, Instant)>(sample_id);
        data.insert_temp(sample_id, (cpu_ticks, now));
        previous
    });
    let cpu_percent = previous.map_or(0.0, |(previous_ticks, previous_time)| {
        let seconds = now.duration_since(previous_time).as_secs_f32();
        if seconds <= 0.0 {
            0.0
        } else {
            cpu_ticks.saturating_sub(previous_ticks) as f32 / seconds
        }
    });
    Some((cpu_percent, memory_kib * 1024, total_memory_kib * 1024))
}

fn show_output_panel(
    ui: &mut egui::Ui,
    workspace: &mut Workspace,
    layout: &mut EditorLayout,
    process: &ProcessHandle,
    language_server: &[Diagnostic],
) {
    let max_output_height = (ui.ctx().content_rect().height() * 0.65).max(180.0);
    layout.output_height = if layout.output_height.is_finite() {
        layout.output_height.clamp(120.0, max_output_height)
    } else {
        165.0
    };
    let side = layout.output_side;
    let mut show = |ui: &mut egui::Ui| {
        let process_state = process.state();
        let state = process_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let debugger_active = state.debugger_active && state.running;
        if state.running
            && state.active_action == Some(ProcessAction::Debug)
            && !layout.show_variables
        {
            layout.show_debugger = true;
            layout.show_problems = false;
            layout.show_variables = false;
        }
        if !state.debugger_active || !state.running {
            layout.show_variables = false;
        }
        let build_diagnostics = state
            .output
            .iter()
            .filter_map(|line| parse_build_diagnostic(line))
            .collect::<Vec<_>>();
        let error_count = build_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
            .count()
            + language_server
                .iter()
                .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
                .count();
        let warning_count = build_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
            .count()
            + language_server
                .iter()
                .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
                .count();
        drop(state);
        let output_body_height = if matches!(side, DockSide::Bottom | DockSide::Top) {
            (layout.output_height - 42.0).max(55.0)
        } else {
            ui.available_height()
        };
        ui.horizontal(|ui| {
            ui.dnd_drag_source(
                egui::Id::unique("editor-output-drag-handle"),
                DockItem::Output,
                |ui| ui.label(RichText::new("⠿").strong()),
            );
            if ui
                .selectable_label(
                    !layout.show_problems && !layout.show_debugger && !layout.show_variables,
                    "Build Output",
                )
                .clicked()
            {
                layout.show_problems = false;
                layout.show_debugger = false;
                layout.show_variables = false;
            }
            if ui
                .selectable_label(layout.show_debugger, "Debugger")
                .clicked()
            {
                layout.show_debugger = true;
                layout.show_problems = false;
                layout.show_variables = false;
            }
            if debugger_active
                && ui
                    .selectable_label(layout.show_variables, "Variables")
                    .clicked()
            {
                layout.show_variables = true;
                layout.show_debugger = false;
                layout.show_problems = false;
            }
            if ui
                .selectable_label(
                    layout.show_problems,
                    format!("Problems ({error_count} errors, {warning_count} warnings)"),
                )
                .clicked()
            {
                layout.show_problems = true;
                layout.show_debugger = false;
                layout.show_variables = false;
            }
            let process_state = process.state();
            let process_state = process_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if process_state.running {
                ui.spinner();
                let action = process_state
                    .active_action
                    .map_or("Process".to_owned(), |action| action.label().to_owned());
                if let Some(pid) = process_state.runtime_pid {
                    ui.label(format!("{action} · PID {pid}"));
                } else {
                    ui.label(format!("{action} in progress"));
                }
            } else {
                ui.label(if process_state.last_result.is_some() {
                    "Idle · last process finished"
                } else {
                    "Idle"
                });
            }
        });
        if layout.show_variables && debugger_active {
            debugger_variables_ui(ui, process, workspace);
        } else if layout.show_debugger {
            debugger_console_ui(ui, process, output_body_height);
        } else if layout.show_problems {
            show_problem_list(
                ui,
                process,
                workspace,
                &build_diagnostics,
                language_server,
                output_body_height,
            );
        } else {
            process_output_ui(ui, process, output_body_height);
        }
    };

    let response = match side {
        DockSide::Left => egui::Panel::left("editor_output")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| show(ui)),
        DockSide::Right => egui::Panel::right("editor_output")
            .resizable(true)
            .default_size(340.0)
            .show(ui, |ui| show(ui)),
        DockSide::Top => egui::Panel::top("editor_output")
            .resizable(true)
            .max_size(max_output_height)
            .default_size(layout.output_height)
            .show(ui, |ui| show(ui)),
        DockSide::Bottom => egui::Panel::bottom("editor_output")
            .resizable(true)
            .max_size(max_output_height)
            .default_size(layout.output_height)
            .show(ui, |ui| show(ui)),
    };
    drop(show);
    if matches!(side, DockSide::Bottom | DockSide::Top) {
        // Content updates (especially Build Output) must not change the saved
        // splitter position. Only persist the panel size while its divider is
        // being dragged by the user.
        let resize_id = ui.scope_id().with("editor_output").with("__resize");
        if ui
            .read_response(resize_id)
            .is_some_and(|handle| handle.dragged() || handle.drag_stopped())
        {
            layout.output_height = response
                .response
                .rect
                .height()
                .clamp(120.0, max_output_height);
        }
    }
}

fn show_problem_list(
    ui: &mut egui::Ui,
    process: &ProcessHandle,
    workspace: &mut Workspace,
    build: &[Diagnostic],
    language_server: &[Diagnostic],
    max_height: f32,
) {
    let filter_id = egui::Id::unique("problems-view-filter");
    let mut filter = ui
        .ctx()
        .data(|data| data.get_temp::<OutputFilter>(filter_id))
        .unwrap_or_default();
    let total_errors = build
        .iter()
        .chain(language_server)
        .filter(|item| item.severity == DiagnosticSeverity::Error)
        .count();
    let total_warnings = build
        .iter()
        .chain(language_server)
        .filter(|item| item.severity == DiagnosticSeverity::Warning)
        .count();
    let mut clear_severity = None;

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("PROBLEMS").strong());
            ui.label(
                RichText::new(format!("{total_errors} errors · {total_warnings} warnings"))
                    .small()
                    .color(theme::muted()),
            );
            ui.separator();
            ui.selectable_value(&mut filter, OutputFilter::All, "All");
            ui.selectable_value(&mut filter, OutputFilter::Errors, "Errors");
            ui.selectable_value(&mut filter, OutputFilter::Warnings, "Warnings");
            if ui
                .button("Clear build warnings")
                .on_hover_text("Remove warning lines from the current build output")
                .clicked()
            {
                clear_severity = Some(DiagnosticSeverity::Warning);
            }
            if ui
                .button("Clear build errors")
                .on_hover_text("Remove error lines from the current build output")
                .clicked()
            {
                clear_severity = Some(DiagnosticSeverity::Error);
            }
        });
        ui.label(
            RichText::new("Click a problem to open its file and jump to the reported line.")
                .small()
                .color(theme::muted()),
        );
        ui.add_space(6.0);
        let matches = |diagnostic: &&Diagnostic| match filter {
            OutputFilter::All => true,
            OutputFilter::Errors => diagnostic.severity == DiagnosticSeverity::Error,
            OutputFilter::Warnings => diagnostic.severity == DiagnosticSeverity::Warning,
        };
        let filtered_language = language_server.iter().filter(matches).collect::<Vec<_>>();
        let filtered_build = build.iter().filter(matches).collect::<Vec<_>>();
        let shown_count = filtered_language.len() + filtered_build.len();
        if shown_count == 0 {
            ui.label(
                RichText::new(match filter {
                    OutputFilter::Errors => "No errors match this filter.",
                    OutputFilter::Warnings => "No warnings match this filter.",
                    OutputFilter::All => {
                        "No problems found. Build and language-server diagnostics appear here."
                    }
                })
                .color(theme::muted()),
            );
        } else {
            egui::ScrollArea::vertical()
                .id_salt("problem-list")
                .auto_shrink([false, false])
                .max_height(max_height)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    draw_problem_group(ui, workspace, "LANGUAGE SERVER", &filtered_language);
                    draw_problem_group(ui, workspace, "BUILD", &filtered_build);
                });
        }
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(filter_id, filter));
    if let Some(severity) = clear_severity {
        let process_state = process.state();
        let mut state = process_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.output.retain(|line| {
            parse_build_diagnostic(line).is_none_or(|diagnostic| diagnostic.severity != severity)
        });
    }
}

fn draw_problem_group(
    ui: &mut egui::Ui,
    workspace: &mut Workspace,
    title: &str,
    diagnostics: &[&Diagnostic],
) {
    if diagnostics.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.label(RichText::new(title).small().strong().color(theme::muted()));
    for diagnostic in diagnostics {
        show_diagnostic_row(ui, workspace, diagnostic);
    }
}

fn show_diagnostic_row(ui: &mut egui::Ui, workspace: &mut Workspace, diagnostic: &Diagnostic) {
    let (icon, color) = match diagnostic.severity {
        DiagnosticSeverity::Error => ("✖", Color32::from_rgb(239, 83, 80)),
        DiagnosticSeverity::Warning => ("⚠", Color32::from_rgb(230, 180, 80)),
        DiagnosticSeverity::Information => ("ℹ", Color32::from_rgb(90, 165, 235)),
        DiagnosticSeverity::Hint => ("◆", Color32::from_rgb(150, 160, 175)),
    };
    let clicked = ui
        .horizontal(|ui| {
            ui.label(RichText::new(icon).color(color));
            let response = ui.add(
                egui::Button::new(RichText::new(&diagnostic.message).monospace().size(12.5))
                    .frame(false),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(line) = diagnostic.line {
                    let location = diagnostic.column.map_or_else(
                        || format!("line {line}"),
                        |column| format!("{line}:{column}"),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{}  {location}",
                            diagnostic.file.as_deref().unwrap_or_default()
                        ))
                        .small()
                        .color(theme::muted()),
                    );
                } else if let Some(file) = &diagnostic.file {
                    ui.label(RichText::new(file).small().color(theme::muted()));
                }
            });
            response
                .on_hover_text("Click to open this location")
                .clicked()
        })
        .inner;
    if clicked {
        if let (Some(file), Some(line), Some(root)) = (
            diagnostic.file.as_deref(),
            diagnostic.line,
            workspace
                .current
                .as_ref()
                .map(|project| project.path.as_path()),
        ) {
            let path = std::path::PathBuf::from(file);
            let relative = if path.is_absolute() {
                path.strip_prefix(root)
                    .ok()
                    .map(std::path::Path::to_path_buf)
            } else {
                Some(path)
            };
            if let Some(relative) = relative {
                navigate_to_project_location(
                    ui.ctx(),
                    workspace,
                    &relative,
                    line,
                    diagnostic.column.unwrap_or(1),
                );
            }
        }
    }
}

#[derive(Default)]
struct FileTreeNode {
    directories: BTreeMap<String, FileTreeNode>,
    files: Vec<(String, std::path::PathBuf)>,
}

fn show_project_tree(ui: &mut egui::Ui, workspace: &mut Workspace) {
    let mut root = FileTreeNode::default();
    for file in &workspace.files {
        let mut node = &mut root;
        let components = file
            .components()
            .filter_map(|component| component.as_os_str().to_str())
            .collect::<Vec<_>>();
        for directory in components.iter().take(components.len().saturating_sub(1)) {
            node = node.directories.entry((*directory).to_owned()).or_default();
        }
        if let Some(name) = components.last() {
            node.files.push(((*name).to_owned(), file.clone()));
        }
    }
    for directory in &workspace.directories {
        let mut node = &mut root;
        for component in directory
            .components()
            .filter_map(|part| part.as_os_str().to_str())
        {
            node = node.directories.entry(component.to_owned()).or_default();
        }
    }

    let tree_height = (ui.available_height() - 150.0).max(140.0);
    egui::ScrollArea::both()
        .id_salt("solution-tree")
        .max_height(tree_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(project) = workspace.current.clone() {
                let response = tree_row(
                    ui,
                    0,
                    Some(true),
                    "▣",
                    theme::accent_hover(),
                    &project.name,
                    false,
                    true,
                );
                response.context_menu(|ui| {
                    show_new_file_menu(ui, workspace, std::path::Path::new(""));
                });
                render_file_tree(ui, &root, workspace, std::path::Path::new(""), 1);
            } else {
                ui.add_space(6.0);
                ui.label(RichText::new("No project open.").color(theme::muted()));
            }
        });
    let selected_path = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Option<std::path::PathBuf>>(tree_selection_id()))
        .flatten();
    if !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|input| input.key_pressed(egui::Key::Delete))
        && let Some(path) = selected_path
        && !path.as_os_str().is_empty()
    {
        request_project_item_delete(ui, path);
    }
}

#[derive(Debug, Default, Clone)]
struct ProjectSymbolFilter {
    query: String,
}

fn show_symbol_list(ui: &mut egui::Ui, workspace: &mut Workspace) {
    ui.label(
        RichText::new("PROJECT SYMBOLS")
            .small()
            .strong()
            .color(Color32::from_rgb(155, 173, 197)),
    );
    let filter_id = egui::Id::unique("cairn.project-symbol-filter");
    let mut filter = ui
        .ctx()
        .data(|data| data.get_temp::<ProjectSymbolFilter>(filter_id))
        .unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut filter.query)
            .hint_text("Filter symbols")
            .desired_width(f32::INFINITY),
    );
    ui.ctx()
        .data_mut(|data| data.insert_temp(filter_id, filter.clone()));
    let query = filter.query.trim().to_lowercase();
    let mut symbols = workspace.symbols.clone();
    symbols.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    egui::ScrollArea::vertical()
        .id_salt("project-symbol-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for symbol in symbols.iter().filter(|symbol| {
                query.is_empty()
                    || symbol.name.to_lowercase().contains(&query)
                    || symbol.kind.to_lowercase().contains(&query)
                    || symbol
                        .path
                        .display()
                        .to_string()
                        .to_lowercase()
                        .contains(&query)
            }) {
                let label = format!("{}  ·  {}", symbol.name, symbol.kind);
                if ui
                    .button(label)
                    .on_hover_text(format!("{}:{}", symbol.path.display(), symbol.line))
                    .clicked()
                {
                    navigate_to_project_line(ui.ctx(), workspace, &symbol.path, symbol.line);
                }
            }
        });
}

fn show_active_file_outline(ui: &mut egui::Ui, workspace: &mut Workspace) {
    ui.label(
        RichText::new("ACTIVE FILE OUTLINE")
            .small()
            .strong()
            .color(theme::muted()),
    );
    let Some((path, active_text)) = workspace.active_document().and_then(|document| {
        document
            .path
            .clone()
            .map(|path| (path, document.text.clone()))
    }) else {
        ui.label("Open a project file to see its symbols.");
        return;
    };
    let filter_id = ui.make_persistent_id(("active-file-outline-filter", &path));
    let mut query = ui
        .ctx()
        .data(|data| data.get_temp::<String>(filter_id))
        .unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text("Filter this file's symbols")
            .desired_width(f32::INFINITY),
    );
    ui.ctx()
        .data_mut(|data| data.insert_temp(filter_id, query.clone()));
    let query = query.to_lowercase();
    let lsp_symbols = ui
        .ctx()
        .data(|data| {
            data.get_temp::<DocumentSymbolsCache>(egui::Id::unique("cairn.lsp-document-symbols"))
        })
        .filter(|cache| cache.path == path && cache.text == active_text);
    let symbols = lsp_symbols.map(|cache| cache.symbols);
    let fallback_symbols = workspace
        .symbols
        .iter()
        .filter(|symbol| symbol.path == path)
        .filter(|symbol| query.is_empty() || symbol.name.to_lowercase().contains(&query))
        .cloned()
        .collect::<Vec<_>>();
    if symbols.as_ref().is_some_and(|symbols| !symbols.is_empty()) {
        let symbols = symbols.unwrap_or_default();
        egui::ScrollArea::vertical()
            .id_salt(("active-file-outline-lsp", &path))
            .show(ui, |ui| {
                for symbol in symbols.iter().filter(|symbol| {
                    query.is_empty()
                        || symbol.name.to_lowercase().contains(&query)
                        || symbol
                            .detail
                            .as_ref()
                            .is_some_and(|detail| detail.to_lowercase().contains(&query))
                }) {
                    let kind = lsp_symbol_kind(symbol.kind);
                    let label = format!(
                        "{}{}  ·  {}",
                        "  ".repeat(symbol.depth.min(12)),
                        symbol.name,
                        kind
                    );
                    if ui
                        .button(label)
                        .on_hover_text(symbol.detail.as_deref().unwrap_or(&symbol.name))
                        .clicked()
                    {
                        let target = workspace
                            .current
                            .as_ref()
                            .and_then(|project| symbol.path.strip_prefix(&project.path).ok())
                            .unwrap_or(&symbol.path);
                        navigate_to_project_line(ui.ctx(), workspace, target, symbol.line);
                    }
                }
            });
    } else if !fallback_symbols.is_empty() {
        ui.label(
            RichText::new("Indexed symbols (language-server outline pending)")
                .small()
                .color(theme::muted()),
        );
        egui::ScrollArea::vertical()
            .id_salt(("active-file-outline-fallback", path))
            .show(ui, |ui| {
                for symbol in fallback_symbols {
                    if ui
                        .button(format!("{}  ·  {}", symbol.name, symbol.kind))
                        .on_hover_text(format!("Line {} (workspace index)", symbol.line))
                        .clicked()
                    {
                        navigate_to_project_line(ui.ctx(), workspace, &symbol.path, symbol.line);
                    }
                }
            });
    } else if symbols.is_none() {
        ui.label(
            RichText::new("Waiting for the C++/C# language server outline…").color(theme::muted()),
        );
    } else {
        ui.label(RichText::new("No symbols found in this file.").color(theme::muted()));
    }
}

fn lsp_symbol_kind(kind: Option<u64>) -> &'static str {
    match kind {
        Some(1) => "File",
        Some(2) => "Module",
        Some(3) => "Namespace",
        Some(4) => "Package",
        Some(5) => "Class",
        Some(6) => "Method",
        Some(7) => "Property",
        Some(8) => "Field",
        Some(9) => "Constructor",
        Some(10) => "Enum",
        Some(11) => "Interface",
        Some(12) => "Function",
        Some(13) => "Variable",
        Some(14) => "Constant",
        Some(15) => "String",
        Some(16) => "Number",
        Some(17) => "Boolean",
        Some(18) => "Array",
        Some(19) => "Object",
        Some(20) => "Key",
        Some(21) => "Null",
        Some(22) => "Enum member",
        Some(23) => "Struct",
        Some(24) => "Event",
        Some(25) => "Operator",
        Some(26) => "Type parameter",
        _ => "Symbol",
    }
}

fn show_breadcrumbs(ui: &mut egui::Ui, workspace: &mut Workspace) {
    ui.label(
        RichText::new("BREADCRUMBS")
            .small()
            .strong()
            .color(theme::muted()),
    );
    let Some(document) = workspace.active_document() else {
        ui.label("No active file.");
        return;
    };
    let Some(path) = document.path.clone() else {
        ui.label(&document.title);
        return;
    };
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    ui.horizontal_wrapped(|ui| {
        for (index, component) in components.iter().enumerate() {
            if index > 0 {
                ui.label(RichText::new("›").color(theme::muted()));
            }
            ui.label(*component);
        }
    });
    ui.separator();
    ui.label("Symbols in this file");
    let symbols = workspace
        .symbols
        .iter()
        .filter(|symbol| symbol.path == path)
        .cloned()
        .collect::<Vec<_>>();
    for symbol in symbols {
        if ui
            .small_button(format!("{}  ›  {}", symbol.kind, symbol.name))
            .clicked()
        {
            navigate_to_project_line(ui.ctx(), workspace, &symbol.path, symbol.line);
        }
    }
}

#[derive(Clone, Default)]
struct GitPanelState {
    project: Option<std::path::PathBuf>,
    loaded: bool,
    branch: String,
    branches: String,
    status: String,
    commits: String,
    diff: String,
    error: Option<String>,
}

fn git_read(project: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .map_err(|error| format!("Could not start Git: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() {
        Ok(text)
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if error.is_empty() {
            format!("Git {} failed with {}.", args.join(" "), output.status)
        } else {
            error
        })
    }
}

fn show_source_control(ui: &mut egui::Ui, workspace: &Workspace) {
    ui.label(
        RichText::new("SOURCE CONTROL · GIT")
            .small()
            .strong()
            .color(theme::muted()),
    );
    ui.label(
        RichText::new("Read-only status, branches, commits, and diff review.")
            .small()
            .color(theme::muted()),
    );
    let Some(project) = workspace.current.as_ref() else {
        ui.label("Open a project to inspect its Git repository.");
        return;
    };
    let id = ui.make_persistent_id("cairn.git-panel-state");
    let mut state = ui
        .ctx()
        .data(|data| data.get_temp::<GitPanelState>(id))
        .unwrap_or_default();
    if state.project.as_deref() != Some(project.path.as_path()) {
        state = GitPanelState {
            project: Some(project.path.clone()),
            ..GitPanelState::default()
        };
    }
    if ui.button("Refresh Git status").clicked() || !state.loaded {
        state.loaded = true;
        state.error = None;
        match git_read(&project.path, &["status", "--short", "--branch"]) {
            Ok(value) => state.status = value,
            Err(error) => state.error = Some(error),
        }
        state.branch = git_read(&project.path, &["branch", "--show-current"]).unwrap_or_default();
        state.branches = git_read(&project.path, &["branch", "--list"]).unwrap_or_default();
        state.commits =
            git_read(&project.path, &["log", "-5", "--oneline", "--decorate"]).unwrap_or_default();
        state.diff = git_read(&project.path, &["diff", "--"]).unwrap_or_default();
    }
    if let Some(error) = &state.error {
        ui.colored_label(Color32::LIGHT_RED, error);
    } else {
        ui.label(format!(
            "Branch: {}",
            if state.branch.is_empty() {
                "(detached or unavailable)"
            } else {
                &state.branch
            }
        ));
        egui::ScrollArea::vertical()
            .id_salt(("git-panel-scroll", &project.path))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.collapsing("Changes", |ui| {
                    ui.label(if state.status.is_empty() {
                        "Working tree clean"
                    } else {
                        &state.status
                    });
                });
                ui.collapsing("Branches", |ui| {
                    ui.label(if state.branches.is_empty() {
                        "No branches"
                    } else {
                        &state.branches
                    })
                });
                ui.collapsing("Recent commits", |ui| {
                    ui.label(if state.commits.is_empty() {
                        "No commits found"
                    } else {
                        &state.commits
                    })
                });
                ui.collapsing("Unstaged diff", |ui| {
                    if state.diff.is_empty() {
                        ui.label("No unstaged changes.");
                    } else {
                        ui.add(
                            egui::Label::new(RichText::new(&state.diff).monospace())
                                .selectable(true),
                        );
                    }
                });
            });
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, state));
}

fn show_breakpoints(ui: &mut egui::Ui, workspace: &mut Workspace) {
    ui.label(
        RichText::new("BREAKPOINTS")
            .small()
            .strong()
            .color(theme::muted()),
    );
    let entries = workspace.all_breakpoints();
    if entries.is_empty() {
        ui.label("No breakpoints are set. Click the editor gutter to add one.");
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("breakpoints-panel")
        .show(ui, |ui| {
            for (path, line) in entries {
                let display = workspace
                    .current
                    .as_ref()
                    .and_then(|project| path.strip_prefix(&project.path).ok())
                    .unwrap_or(&path)
                    .display();
                if ui.button(format!("●  {display}:{line}")).clicked()
                    && let Some(relative) = workspace
                        .current
                        .as_ref()
                        .and_then(|project| path.strip_prefix(&project.path).ok())
                {
                    navigate_to_project_line(ui.ctx(), workspace, relative, line);
                }
            }
        });
}

fn toggle_current_breakpoint(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    process: &ProcessHandle,
) {
    let Some((line, _)) = ctx.data(|data| {
        data.get_temp::<(usize, usize)>(egui::Id::unique("cairn.lsp-cursor-position"))
    }) else {
        workspace.message =
            "Place the editor cursor on a line before toggling its breakpoint.".to_owned();
        return;
    };
    let line = line + 1;
    let Some(document) = workspace.documents.get_mut(workspace.selected_document) else {
        workspace.message = "Open a source file before setting a breakpoint.".to_owned();
        return;
    };
    if document.path.is_none() {
        workspace.message = "Breakpoints need a saved source file.".to_owned();
        return;
    }
    if let Some(index) = document.breakpoints.iter().position(|&entry| entry == line) {
        document.breakpoints.remove(index);
    } else {
        document.breakpoints.push(line);
        document.breakpoints.sort_unstable();
    }
    let all_breakpoints = workspace.all_breakpoints();
    if let Err(error) = workspace.persist_breakpoints() {
        workspace.message = error;
    }
    if let Err(error) = process.sync_breakpoints(&all_breakpoints) {
        workspace.message = error;
    }
}

fn tree_selection_id() -> egui::Id {
    egui::Id::unique("solution-tree-selected-item")
}

fn request_project_item_delete(ui: &egui::Ui, path: std::path::PathBuf) {
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::unique("solution-tree-delete-request"), Some(path));
    });
}

/// One row of a Visual Studio style tree: indent guides, chevron, icon, label.
/// `open` is `Some(state)` for folders and `None` for leaf files.
#[allow(clippy::too_many_arguments)]
fn tree_row(
    ui: &mut egui::Ui,
    depth: usize,
    open: Option<bool>,
    icon: &str,
    icon_color: Color32,
    label: &str,
    selected: bool,
    bold: bool,
) -> egui::Response {
    let row_height = 22.0;
    let indent = 14.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(160.0), row_height),
        egui::Sense::click(),
    );
    let painter = ui.painter().with_clip_rect(rect);
    if selected {
        painter.rect_filled(
            rect,
            egui::CornerRadius::ZERO,
            Color32::from_rgb(58, 52, 120),
        );
    } else if response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::ZERO, theme::hover());
    }
    for level in 0..depth {
        let x = rect.left() + 10.0 + level as f32 * indent;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, theme::line()),
        );
    }
    let mut x = rect.left() + 6.0 + depth as f32 * indent;
    if let Some(is_open) = open {
        painter.text(
            egui::pos2(x + 5.0, rect.center().y),
            egui::Align2::CENTER_CENTER,
            if is_open { "▾" } else { "▸" },
            egui::FontId::proportional(12.0),
            theme::muted(),
        );
    }
    x += 14.0;
    painter.text(
        egui::pos2(x + 6.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(13.0),
        icon_color,
    );
    x += 18.0;
    painter.text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(if bold { 14.0 } else { 13.5 }),
        if selected {
            Color32::WHITE
        } else {
            theme::text()
        },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn show_project_properties(ui: &mut egui::Ui, workspace: &Workspace) {
    ui.label(
        RichText::new("PROPERTIES")
            .small()
            .strong()
            .color(Color32::from_rgb(155, 173, 197)),
    );
    if let Some(project) = &workspace.current {
        egui::Grid::new("project_properties")
            .num_columns(2)
            .spacing([12.0, 5.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Name").weak());
                ui.label(&project.name);
                ui.end_row();
                ui.label(RichText::new("Language").weak());
                ui.label(project.language.label());
                ui.end_row();
                ui.label(RichText::new("Type").weak());
                ui.label(project.template.label());
                ui.end_row();
            });
    } else {
        ui.label(RichText::new("No project selected").weak());
    }
}

fn render_file_tree(
    ui: &mut egui::Ui,
    node: &FileTreeNode,
    workspace: &mut Workspace,
    relative_directory: &std::path::Path,
    depth: usize,
) {
    for (name, directory) in &node.directories {
        let child_path = relative_directory.join(name);
        let state_id = ui.make_persistent_id(("tree-folder", child_path.clone()));
        let mut is_open = ui
            .data_mut(|data| data.get_temp::<bool>(state_id))
            .unwrap_or(true);
        let response = tree_row(
            ui,
            depth,
            Some(is_open),
            "▪",
            Color32::from_rgb(220, 183, 105),
            name,
            false,
            false,
        );
        if response.clicked() {
            is_open = !is_open;
            ui.data_mut(|data| data.insert_temp(state_id, is_open));
            ui.ctx().data_mut(|data| {
                data.insert_temp(tree_selection_id(), Some(child_path.clone()));
            });
        }
        response.context_menu(|ui| {
            show_new_file_menu(ui, workspace, &child_path);
            if ui.button("Delete").clicked() {
                request_project_item_delete(ui, child_path.clone());
                ui.close();
            }
        });
        if is_open {
            render_file_tree(ui, directory, workspace, &child_path, depth + 1);
        }
    }
    for (name, path) in &node.files {
        let selected = workspace.active_document().is_some_and(|document| {
            document
                .path
                .as_ref()
                .zip(workspace.current.as_ref())
                .is_some_and(|(document_path, project)| project.path.join(path) == *document_path)
        });
        let (icon, icon_color) = match path.extension().and_then(|extension| extension.to_str()) {
            Some("cpp" | "cc" | "cxx" | "hpp" | "h") => ("◆", Color32::from_rgb(86, 164, 224)),
            Some("cs") => ("◆", Color32::from_rgb(142, 196, 111)),
            Some("axaml" | "xaml") => ("◇", Color32::from_rgb(211, 163, 93)),
            Some("cmake") => ("⚙", Color32::from_rgb(182, 149, 222)),
            _ if name == "CMakeLists.txt" => ("⚙", Color32::from_rgb(182, 149, 222)),
            _ => ("·", theme::muted()),
        };
        let response = tree_row(ui, depth, None, icon, icon_color, name, selected, false);
        if response.clicked() {
            ui.ctx().data_mut(|data| {
                data.insert_temp(tree_selection_id(), Some(path.clone()));
            });
            if let Err(error) = workspace.open_file(path) {
                workspace.message = error;
            }
        }
        response.context_menu(|ui| {
            if ui.button("Open").clicked() {
                if let Err(error) = workspace.open_file(path) {
                    workspace.message = error;
                }
                ui.close();
            }
            if let Some(parent) = path.parent() {
                show_new_file_menu(ui, workspace, parent);
            }
            if ui.button("Delete").clicked() {
                request_project_item_delete(ui, path.clone());
                ui.close();
            }
        });
    }
}

fn show_new_file_menu(ui: &mut egui::Ui, workspace: &mut Workspace, directory: &std::path::Path) {
    let Some((language, template)) = workspace
        .current
        .as_ref()
        .map(|project| (project.language, project.template))
    else {
        return;
    };
    ui.menu_button("Add New Item", |ui| {
        let mut options = match language {
            ProjectLanguage::CSharp => vec![
                ("Class", crate::project::NewSourceKind::CSharpClass),
                ("Interface", crate::project::NewSourceKind::CSharpInterface),
                ("Enum", crate::project::NewSourceKind::CSharpEnum),
                ("Record", crate::project::NewSourceKind::CSharpRecord),
                ("Struct", crate::project::NewSourceKind::CSharpStruct),
                (
                    "Static class",
                    crate::project::NewSourceKind::CSharpStaticClass,
                ),
            ],
            ProjectLanguage::Cpp => vec![
                (
                    "C++ Source (.cpp)",
                    crate::project::NewSourceKind::CppSource,
                ),
                (
                    "C++ Header (.hpp)",
                    crate::project::NewSourceKind::CppHeader,
                ),
            ],
        };
        if template == ProjectTemplate::AvaloniaApp {
            options.push((
                "Avalonia View (.axaml)",
                crate::project::NewSourceKind::AvaloniaXaml,
            ));
        }
        for (label, kind) in options {
            if ui.button(label).clicked() {
                let pending = PendingNewItem {
                    directory: directory.to_path_buf(),
                    kind,
                    name: kind.default_name().to_owned(),
                    error: None,
                    focus_requested: false,
                };
                let dialog_id = new_item_dialog_id();
                ui.ctx().data_mut(|data| {
                    data.insert_temp(dialog_id, Some(pending));
                });
                ui.close();
            }
        }
    });
    if ui.button("New Folder").clicked() {
        let dialog_id = new_folder_dialog_id();
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                dialog_id,
                Some(PendingNewFolder {
                    directory: directory.to_path_buf(),
                    name: String::new(),
                    error: None,
                    focus_requested: false,
                }),
            );
        });
        ui.close();
    }
}

fn show_new_item_dialog(ui: &mut egui::Ui, workspace: &mut Workspace) {
    let dialog_id = new_item_dialog_id();
    let pending = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Option<PendingNewItem>>(dialog_id))
        .flatten();
    let Some(mut pending) = pending else {
        return;
    };

    let mut create = false;
    let mut cancel = false;
    egui::Modal::new(dialog_id.with("modal")).show(ui.ctx(), |ui| {
        ui.set_min_width(390.0);
        ui.heading(format!("Add {}", pending.kind.label()));
        ui.label("Enter a file name:");
        let name_response = ui.add(
            egui::TextEdit::singleline(&mut pending.name)
                .desired_width(f32::INFINITY)
                .hint_text(pending.kind.default_name()),
        );
        if !pending.focus_requested {
            name_response.request_focus();
            pending.focus_requested = true;
        }
        if name_response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            create = true;
        }
        ui.label(
            RichText::new(format!(
                "The .{} extension is added automatically.",
                item_extension(pending.kind)
            ))
            .small()
            .color(theme::muted()),
        );
        if let Some(error) = &pending.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Create").clicked() {
                create = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });

    if cancel {
        ui.ctx()
            .data_mut(|data| data.insert_temp(dialog_id, None::<PendingNewItem>));
    } else if create {
        match workspace.create_source_file(&pending.directory, pending.kind, &pending.name) {
            Ok(()) => {
                workspace.message = "File created".to_owned();
                ui.ctx()
                    .data_mut(|data| data.insert_temp(dialog_id, None::<PendingNewItem>));
            }
            Err(error) => pending.error = Some(error),
        }
    }
    if !cancel && (!create || pending.error.is_some()) {
        ui.ctx()
            .data_mut(|data| data.insert_temp(dialog_id, Some(pending)));
    }
}

fn show_new_folder_dialog(ui: &mut egui::Ui, workspace: &mut Workspace) {
    let dialog_id = new_folder_dialog_id();
    let pending = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Option<PendingNewFolder>>(dialog_id))
        .flatten();
    let Some(mut pending) = pending else {
        return;
    };

    let mut create = false;
    let mut cancel = false;
    egui::Modal::new(dialog_id.with("modal")).show(ui.ctx(), |ui| {
        ui.set_min_width(390.0);
        ui.heading("Create Folder");
        ui.label("Enter a folder name:");
        let name_response = ui.add(
            egui::TextEdit::singleline(&mut pending.name)
                .desired_width(f32::INFINITY)
                .hint_text("Folder name"),
        );
        if !pending.focus_requested {
            name_response.request_focus();
            pending.focus_requested = true;
        }
        if name_response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            create = true;
        }
        if let Some(error) = &pending.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Create").clicked() {
                create = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });

    if cancel {
        ui.ctx()
            .data_mut(|data| data.insert_temp(dialog_id, None::<PendingNewFolder>));
    } else if create {
        match workspace.create_folder(&pending.directory, &pending.name) {
            Ok(()) => {
                workspace.message = format!("Folder '{}' created", pending.name.trim());
                ui.ctx()
                    .data_mut(|data| data.insert_temp(dialog_id, None::<PendingNewFolder>));
            }
            Err(error) => pending.error = Some(error),
        }
    }
    if !cancel && (!create || pending.error.is_some()) {
        ui.ctx()
            .data_mut(|data| data.insert_temp(dialog_id, Some(pending)));
    }
}

fn show_delete_confirmation(ui: &mut egui::Ui, workspace: &mut Workspace) {
    let request_id = egui::Id::unique("solution-tree-delete-request");
    let pending = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Option<std::path::PathBuf>>(request_id))
        .flatten();
    let Some(path) = pending else {
        return;
    };
    if !crate::settings::current(ui.ctx()).general.confirm_delete {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("item")
            .to_owned();
        workspace.message = match workspace.delete_project_item(&path) {
            Ok(()) => format!("Deleted '{name}'"),
            Err(error) => error,
        };
        ui.ctx().data_mut(|data| {
            data.insert_temp(request_id, None::<std::path::PathBuf>);
            data.insert_temp(tree_selection_id(), None::<std::path::PathBuf>);
        });
        return;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("this item");
    let is_directory = workspace
        .current
        .as_ref()
        .is_some_and(|project| project.path.join(&path).is_dir());
    let mut confirmed = false;
    let mut cancelled = false;
    egui::Modal::new(request_id.with("confirm")).show(ui.ctx(), |ui| {
        ui.set_min_width(390.0);
        ui.heading("Confirm Delete");
        if is_directory {
            ui.label(format!(
                "Delete folder '{name}' and everything inside it? This cannot be undone."
            ));
        } else {
            ui.label(format!("Delete '{name}'? This cannot be undone."));
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Delete").clicked() {
                confirmed = true;
            }
            if ui.button("Cancel").clicked() {
                cancelled = true;
            }
        });
    });
    if confirmed {
        workspace.message = match workspace.delete_project_item(&path) {
            Ok(()) => format!("Deleted '{name}'"),
            Err(error) => error,
        };
        ui.ctx()
            .data_mut(|data| data.insert_temp(request_id, None::<std::path::PathBuf>));
        ui.ctx()
            .data_mut(|data| data.insert_temp(tree_selection_id(), None::<std::path::PathBuf>));
    } else if cancelled {
        ui.ctx()
            .data_mut(|data| data.insert_temp(request_id, None::<std::path::PathBuf>));
    }
}

fn item_extension(kind: NewSourceKind) -> &'static str {
    match kind {
        NewSourceKind::CSharpClass
        | NewSourceKind::CSharpInterface
        | NewSourceKind::CSharpEnum
        | NewSourceKind::CSharpRecord
        | NewSourceKind::CSharpStruct
        | NewSourceKind::CSharpStaticClass => "cs",
        NewSourceKind::CppSource => "cpp",
        NewSourceKind::CppHeader => "hpp",
        NewSourceKind::AvaloniaXaml => "axaml",
    }
}

fn show_helper_panel(ui: &mut egui::Ui, workspace: &mut Workspace, process: &ProcessHandle) {
    let page_id = egui::Id::unique("cairn-guide-page");
    let mut page = ui
        .ctx()
        .data(|data| data.get_temp::<usize>(page_id))
        .unwrap_or(0)
        .min(2);
    ui.horizontal(|ui| {
        for (index, label) in ["Overview", "Editor", "Build & Debug"]
            .into_iter()
            .enumerate()
        {
            ui.selectable_value(&mut page, index, label);
        }
    });
    ui.ctx().data_mut(|data| data.insert_temp(page_id, page));

    // Keep this ID stable across frames. egui owns and updates the scroll
    // state, so rebuilding the guide contents does not overwrite wheel or
    // scrollbar input with a stale offset from the previous frame.
    egui::ScrollArea::vertical()
        .id_salt(("cairn-guide-scroll", page))
        .auto_shrink([false, false])
        .show(ui, |ui| {
        ui.heading(match page {
            0 => "Fugh Guide · Overview",
            1 => "Fugh Guide · Editor",
            _ => "Fugh Guide · Build & Debug",
        });
        ui.label("Contextual help for the active project and document.");
        ui.separator();
        if page == 0 && let Some(project) = &workspace.current {
            ui.label(RichText::new(&project.name).strong());
            ui.label(format!("Language: {}", project.language.label()));
            if let Some(document) = workspace.active_document() {
                ui.label(format!("File: {}", document.title));
                ui.label(format!("{} lines", document.text.lines().count()));
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Quick actions").strong());
            if ui.button("Insert TODO comment").clicked()
                && let Some(document) = workspace.active_document_mut()
            {
                if !document.text.is_empty() && !document.text.ends_with('\n') {
                    document.text.push('\n');
                }
                document
                    .text
                    .push_str("// TODO: describe the next change\n");
                document.dirty = true;
            }
        } else if page == 0 {
            ui.label("Open or create a project to see file and build context here.");
        }
        ui.separator();
        if page == 1 {
        ui.label(RichText::new("Editor shortcuts").strong());
        ui.label("Shortcuts are listed under Help and can be changed in File ▸ Settings ▸ Keyboard.");
        ui.add_space(8.0);
        ui.label(RichText::new("Code completion").strong());
        ui.label("Type part of a C++ or C# word to see matching keywords and common APIs.");
        ui.label("Use ↑ / ↓ to choose, Enter or Tab to insert, Esc to dismiss.");
        ui.label("Type a period after a value to list callable project functions and common members.");
        ui.label(
            "Fugh also suggests project classes, types, and functions from other source files. Selecting a C++ symbol declared in a project header adds its #include; C# symbols add a using directive when needed.",
        );
        }
        if page == 2 {
        ui.add_space(8.0);
        ui.label(RichText::new("Build and run").strong());
        ui.label("Use the Run, Build, and Debug controls in the top toolbar. Output and Problems appear in the bottom panel.");
        ui.add_space(8.0);
        ui.label(RichText::new("Debugging").strong());
        ui.label("Set breakpoints in the editor gutter. Debug uses GDB/MI for C++ and NetCoreDbg/MI for C#.");
        ui.label("Drag the ⠿ handle to an editor edge to move this panel.");
        ui.label("Resize a panel by dragging its separator.");
        let process_state = process.state();
        let state = process_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(Err(error)) = &state.last_result {
            ui.add_space(8.0);
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        }
    });
}

fn show_code_editor(
    ui: &mut egui::Ui,
    workspace: &mut Workspace,
    process: &ProcessHandle,
    language_server: &mut crate::language_server::LanguageServerClient,
) {
    let line_ending = crate::settings::current(ui.ctx()).general.line_ending;
    let debugging_active = {
        let state = process.state();
        let state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.debugger_active && state.running
    };
    let mut server_started = false;
    if let (Some(root), Some(document)) = (workspace.current.as_ref(), workspace.active_document())
        && let Some(path) = document.path.as_deref()
    {
        let server_language = match language_for_document(document) {
            "cpp" => Some(crate::language_server::ServerLanguage::Cpp),
            "cs" => Some(crate::language_server::ServerLanguage::CSharp),
            _ => None,
        };
        if let Some(server_language) = server_language {
            language_server.sync_document(
                &root.path,
                path,
                server_language,
                &document.text,
                ui.ctx(),
            );
            let request_state_id = egui::Id::unique("cairn.lsp-document-symbol-request-state");
            let mut request_state = ui
                .ctx()
                .data(|data| data.get_temp::<DocumentSymbolsRequestTracker>(request_state_id));
            if request_state
                .as_ref()
                .is_none_or(|state| state.path != path || state.text != document.text)
            {
                request_state = Some(DocumentSymbolsRequestTracker {
                    path: path.to_path_buf(),
                    text: document.text.clone(),
                    since: Instant::now(),
                    requested: false,
                });
            }
            if let Some(state) = request_state.as_mut()
                && language_server.is_ready()
                && !state.requested
                && state.since.elapsed() >= Duration::from_millis(250)
                && let Some(id) = language_server.request_document_symbols(path)
            {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        egui::Id::unique("cairn.lsp-pending-document-symbols"),
                        (id, path.to_path_buf(), document.text.clone()),
                    );
                });
                state.requested = true;
            }
            ui.ctx()
                .data_mut(|data| data.insert_temp(request_state_id, request_state));
            server_started = true;
        }
    }
    if !server_started {
        language_server.close_document();
    }
    let minimap_visible_id = egui::Id::unique("cairn.editor-minimap-visible");
    let mut minimap_visible = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(minimap_visible_id))
        .unwrap_or(true);
    ui.horizontal(|ui| {
        if server_started {
            if let Some(status) = language_server.status() {
                let color = if status.contains("connected") {
                    Color32::from_rgb(95, 190, 125)
                } else if status.contains("Retrying") || status.contains("Could not") {
                    Color32::LIGHT_RED
                } else {
                    theme::muted()
                };
                ui.label(RichText::new(format!("● Live diagnostics: {status}")).color(color))
                    .on_hover_text("Diagnostics come from the configured language server and update while you edit.");
            }
        } else if workspace.active_document().is_some() {
            ui.label(
                RichText::new("Live diagnostics are available for C++ (clangd) and C# (Roslyn).")
                    .small()
                    .color(theme::muted()),
            );
        }
        if ui
            .selectable_label(minimap_visible, "Minimap")
            .on_hover_text("Toggle the source overview strip. Alt+click in the editor adds a cursor; Escape clears extra cursors.")
            .clicked()
        {
            minimap_visible = !minimap_visible;
            ui.ctx().data_mut(|data| data.insert_temp(minimap_visible_id, minimap_visible));
        }
    });
    let editor_diagnostics = workspace
        .active_document()
        .and_then(|document| {
            let path = document.path.as_deref()?;
            let project_root = workspace.current.as_ref()?.path.as_path();
            let mut diagnostics = language_server
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.file == path)
                .map(|diagnostic| Diagnostic {
                    severity: match diagnostic.severity {
                        1 => DiagnosticSeverity::Error,
                        2 => DiagnosticSeverity::Warning,
                        3 => DiagnosticSeverity::Information,
                        4 => DiagnosticSeverity::Hint,
                        _ => DiagnosticSeverity::Information,
                    },
                    message: diagnostic.message.clone(),
                    file: Some(diagnostic.file.display().to_string()),
                    line: Some(diagnostic.line),
                    column: Some(diagnostic.column),
                    end_line: Some(diagnostic.end_line),
                    end_column: Some(diagnostic.end_column),
                    source: diagnostic
                        .source
                        .clone()
                        .or_else(|| diagnostic.code.clone()),
                })
                .collect::<Vec<_>>();
            let output = process.state();
            let output = output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for diagnostic in output
                .output
                .iter()
                .filter_map(|line| parse_build_diagnostic(line))
            {
                let Some(file) = diagnostic.file.as_deref() else {
                    continue;
                };
                let diagnostic_path = std::path::Path::new(file);
                let matches = if diagnostic_path.is_absolute() {
                    diagnostic_path == path
                } else {
                    project_root.join(diagnostic_path) == path
                };
                if matches {
                    diagnostics.push(diagnostic);
                }
            }
            Some(diagnostics)
        })
        .unwrap_or_default();
    let close_request_id = ui.make_persistent_id("editor-close-tab-request");
    let mut close_requested = None;
    let mut reorder_requested = None;
    let project_root = workspace
        .current
        .as_ref()
        .map(|project| project.path.clone());
    let symbol_snapshot_id = egui::Id::unique("cairn.editor-project-symbol-snapshot");
    let project_symbols = ui
        .ctx()
        .data(|data| {
            data.get_temp::<(
                Option<std::path::PathBuf>,
                u64,
                Arc<Vec<crate::project::ProjectSymbol>>,
            )>(symbol_snapshot_id)
        })
        .filter(|(root, revision, _)| {
            *root == project_root && *revision == workspace.symbol_revision
        })
        .map(|(_, _, symbols)| symbols)
        .unwrap_or_else(|| {
            let symbols = Arc::new(workspace.symbols.clone());
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    symbol_snapshot_id,
                    (
                        project_root.clone(),
                        workspace.symbol_revision,
                        symbols.clone(),
                    ),
                )
            });
            symbols
        });
    let active_document_path = workspace
        .active_document()
        .and_then(|document| document.path.clone());
    let mut refresh_symbols = false;
    let mut breakpoints_changed = false;
    let (debug_location, _) = {
        let state = process.state();
        let state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (state.debug_location.clone(), state.debug_stop_sequence)
    };
    egui::ScrollArea::horizontal()
        .id_salt("editor-tabs")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let count = workspace.documents.len();
                for index in 0..count {
                    let document = &workspace.documents[index];
                    let label = if document.dirty {
                        format!(
                            "{}●  {}",
                            if document.pinned { "◆ " } else { "" },
                            document.title
                        )
                    } else {
                        format!(
                            "{}{}",
                            if document.pinned { "◆ " } else { "" },
                            document.title
                        )
                    };
                    let selected = workspace.selected_document == index;
                    let tab_frame = egui::Frame::new()
                        .fill(if selected {
                            theme::editor()
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(egui::Stroke::new(
                            1.0,
                            if selected {
                                theme::accent()
                            } else {
                                Color32::TRANSPARENT
                            },
                        ))
                        .corner_radius(egui::CornerRadius::same(
                            if crate::settings::current(ui.ctx()).appearance.tabs_rounded {
                                5
                            } else {
                                0
                            },
                        ))
                        .inner_margin(egui::Margin::symmetric(9, 5));
                    let (_tab, dropped) = ui.dnd_drop_zone::<usize, _>(tab_frame, |ui| {
                        ui.set_min_height(30.0);
                        ui.horizontal(|ui| {
                            ui.dnd_drag_source(
                                ui.make_persistent_id(("editor-tab-drag", index)),
                                index,
                                |ui| {
                                    ui.label(RichText::new("⠿").small().color(theme::muted()))
                                        .on_hover_text("Drag this handle to reorder tabs");
                                },
                            );
                            if ui.selectable_label(selected, label).clicked() {
                                workspace.selected_document = index;
                            }
                            if ui.small_button("×").on_hover_text("Close tab").clicked() {
                                close_requested = Some(index);
                            }
                        });
                    });
                    if let Some(from) = dropped.as_deref().copied()
                        && from != index
                    {
                        reorder_requested = Some((from, index));
                    }
                }
            });
        });

    if let Some((from, to)) = reorder_requested
        && from < workspace.documents.len()
        && to < workspace.documents.len()
    {
        let selected = workspace.selected_document;
        let document = workspace.documents.remove(from);
        workspace.documents.insert(to, document);
        workspace.selected_document = if selected == from {
            to
        } else if from < selected && to >= selected {
            selected - 1
        } else if from > selected && to <= selected {
            selected + 1
        } else {
            selected
        };
    }

    if let Some(index) = close_requested {
        if workspace
            .documents
            .get(index)
            .is_some_and(|document| document.dirty)
            && crate::settings::current(ui.ctx())
                .general
                .confirm_close_unsaved
        {
            ui.ctx()
                .data_mut(|data| data.insert_temp(close_request_id, Some(index)));
        } else if let Err(error) = workspace.close_document(index) {
            workspace.message = error;
        }
    }

    let pending_close = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Option<usize>>(close_request_id))
        .flatten();
    if let Some(index) = pending_close {
        let title = workspace
            .documents
            .get(index)
            .map_or("this file", |document| document.title.as_str())
            .to_owned();
        let mut choice: Option<Option<bool>> = None;
        egui::Modal::new(close_request_id.with("confirm")).show(ui.ctx(), |ui| {
            ui.set_min_width(360.0);
            ui.heading("Save changes?");
            ui.label(format!("Save changes to '{title}' before closing it?"));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Save and Close").clicked() {
                    choice = Some(Some(true));
                }
                if ui.button("Discard").clicked() {
                    choice = Some(Some(false));
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(None);
                }
            });
        });
        if let Some(choice) = choice {
            ui.ctx()
                .data_mut(|data| data.insert_temp(close_request_id, None::<usize>));
            match choice {
                Some(true) => match workspace.save_document_with_line_ending(index, line_ending) {
                    Ok(()) => {
                        if let Err(error) = workspace.close_document(index) {
                            workspace.message = error;
                        }
                    }
                    Err(error) => workspace.message = error,
                },
                Some(false) => {
                    if let Err(error) = workspace.close_document(index) {
                        workspace.message = error;
                    }
                }
                None => {}
            }
        }
    }

    if let Some(document) = workspace.active_document_mut() {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(
                    document
                        .path
                        .as_ref()
                        .map_or_else(|| document.title.clone(), |path| path.display().to_string()),
                )
                .small()
                .color(Color32::GRAY),
            );
            if let Some(status) = language_server.status().filter(|status| {
                status.starts_with("Could not")
                    || status.contains("rejected")
                    || status.contains("exited during startup")
            }) {
                ui.colored_label(Color32::LIGHT_RED, RichText::new(status).small())
                    .on_hover_text(status);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(language_for_document(document))
                        .small()
                        .color(Color32::LIGHT_BLUE),
                );
            });
        });
        ui.separator();

        let desired_rows = (ui.available_height() / ui.text_style_height(&TextStyle::Monospace))
            .floor()
            .max(8.0) as usize;
        let editor_settings = crate::settings::current(ui.ctx()).editor;
        let word_wrap = editor_settings.word_wrap;
        let scroll_area = if word_wrap {
            egui::ScrollArea::vertical()
        } else {
            egui::ScrollArea::both()
        };
        scroll_area
            .id_salt(("editor-scroll", &document.title))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let language = language_for_document(document).to_owned();
                let reference_lens_state = ui.ctx().data(|data| {
                    data.get_temp::<ReferenceLensState>(egui::Id::unique(
                        "cairn.reference-lens-state",
                    ))
                }).unwrap_or_default();
                let previous_text = document.text.clone();
                let reference_lens_path = document.path.as_deref().and_then(|path| {
                    project_root
                        .as_deref()
                        .map(|root| resolve_project_path(root, path))
                });
                let active_reference_lenses = reference_lens_state
                    .entries
                    .iter()
                    .filter(|entry| Some(&entry.path) == reference_lens_path.as_ref())
                    .cloned()
                    .collect::<Vec<_>>();
                let fold_path = document.path.clone();
                let foldable_cache_id =
                    egui::Id::unique(("cairn.editor-foldable-cache", &document.title));
                let fold_stamp = if reference_lens_path.is_some()
                    && reference_lens_state.document_path == reference_lens_path
                {
                    reference_lens_state.source_stamp
                } else {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    previous_text.hash(&mut hasher);
                    hasher.finish()
                };
                let cached_foldable = ui
                    .ctx()
                    .data(|data| data.get_temp::<(u64, Vec<FoldRange>)>(foldable_cache_id));
                let foldable = cached_foldable
                    .filter(|(stamp, _)| *stamp == fold_stamp)
                    .map(|(_, ranges)| ranges)
                    .unwrap_or_else(|| {
                        let ranges = foldable_ranges(&previous_text);
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(foldable_cache_id, (fold_stamp, ranges.clone()))
                        });
                        ranges
                    });
                let fold_state_id = egui::Id::unique("cairn.editor-fold-ranges");
                let mut folds_by_path = ui.ctx().data(|data| {
                    data.get_temp::<HashMap<std::path::PathBuf, Vec<FoldRange>>>(fold_state_id)
                }).unwrap_or_default();
                let mut folded_ranges = fold_path
                    .as_ref()
                    .and_then(|path| folds_by_path.get(path))
                    .cloned()
                    .unwrap_or_default();
                folded_ranges.retain(|fold| foldable.contains(fold));
                if let Some(path) = fold_path.as_ref() {
                    folds_by_path.insert(path.clone(), folded_ranges.clone());
                    ui.ctx().data_mut(|data| data.insert_temp(fold_state_id, folds_by_path.clone()));
                }
                let editor_display = editor_display_buffer(
                    &document.text,
                    &active_reference_lenses,
                    reference_lens_path.as_deref(),
                    &folded_ranges,
                );
                let mut editor_buffer = editor_display.text.clone();
                let code_family = match editor_settings.font_family {
                    crate::settings::EditorFontFamily::Monospace => egui::FontFamily::Monospace,
                    crate::settings::EditorFontFamily::Proportional => {
                        egui::FontFamily::Proportional
                    }
                };
                let code_theme = CodeTheme::from_memory(ui.ctx(), ui.style())
                    .with_font_id(egui::FontId::new(editor_settings.font_size, code_family));
                let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                    let mut job =
                        highlight(ui.ctx(), ui.style(), &code_theme, text.as_str(), &language);
                    job.wrap.max_width = if word_wrap { wrap_width } else { f32::INFINITY };
                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                };
                let completion_id = ui.make_persistent_id(("editor-completion", &document.title));
                let mut completion_state = ui
                    .ctx()
                    .data_mut(|memory| memory.get_temp::<CompletionUiState>(completion_id))
                    .unwrap_or_default();
                let mut accept_requested = false;
                let mut dismiss_requested = false;
                let card_rect_id = egui::Id::unique("cairn.editor-unified-hover-card-rect");
                let pointer_position = ui.ctx().pointer_interact_pos();
                let pointer_over_card = ui
                    .ctx()
                    .data(|data| data.get_temp::<egui::Rect>(card_rect_id))
                    .is_some_and(|rect| pointer_position.is_some_and(|position| rect.contains(position)))
                    || pointer_position.is_some_and(|position| {
                        ui.ctx().layer_id_at(position)
                            == Some(egui::LayerId::new(
                                egui::Order::Tooltip,
                                egui::Id::unique("cairn.editor-unified-hover-card"),
                            ))
                    });
                let signature_cache_id = egui::Id::unique("cairn.editor-unified-hover-card-signature");
                let cached_signature = ui.ctx().data(|data| {
                    data.get_temp::<(usize, SignatureHelpInfo, egui::Pos2)>(signature_cache_id)
                });
                if pointer_over_card
                    && let Some((_, info, _)) = cached_signature.as_ref()
                    && info.overloads.len() > 1
                {
                    let labels = info.overloads.iter().map(|overload| overload.label.clone()).collect::<Vec<_>>();
                    let selection_id = egui::Id::unique("cairn.editor-signature-overload-selection");
                    let selection = ui.ctx().data(|data| data.get_temp::<OverloadSelection>(selection_id));
                    let selected = selection.as_ref()
                        .filter(|selection| selection.labels == labels)
                        .map_or(info.active_signature, |selection| selection.selected)
                        .min(labels.len() - 1);
                    let mut changed = false;
                    let selected = ui.input_mut(|input| {
                        let mut selected = selected;
                        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                            selected = (selected + 1) % labels.len();
                            changed = true;
                        }
                        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                            selected = (selected + labels.len() - 1) % labels.len();
                            changed = true;
                        }
                        selected
                    });
                    if changed {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(selection_id, OverloadSelection { labels, selected });
                        });
                        ui.ctx().request_repaint();
                    }
                }
                if completion_state.open {
                    ui.input_mut(|input| {
                        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                            completion_state.selected = completion_state.selected.saturating_add(1);
                        }
                        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                            completion_state.selected = completion_state.selected.saturating_sub(1);
                        }
                        accept_requested = input
                            .consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                            || input.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                        dismiss_requested =
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                    });
                }
                let popup_layer = egui::LayerId::new(egui::PopupKind::Popup.order(), completion_id);
                let pointer_on_popup = completion_state.open
                    && ui
                        .ctx()
                        .pointer_interact_pos()
                        .and_then(|position| ui.ctx().layer_id_at(position))
                        == Some(popup_layer);
                let surrender_focus_on = ui
                    .ctx()
                    .options(|options| options.input_options.surrender_focus_on);
                if pointer_on_popup {
                    ui.ctx().options_mut(|options| {
                        options.input_options.surrender_focus_on = egui::SurrenderFocusOn::Never;
                    });
                }
                let text_edit_id = ui.make_persistent_id((
                    "code-editor-text",
                    document.path.as_deref(),
                    document.title.as_str(),
                ));
                let multi_cursor_id = text_edit_id.with("secondary-cursors");
                let mut secondary_cursors = ui.ctx().data(|data| {
                    data.get_temp::<Vec<usize>>(multi_cursor_id)
                }).unwrap_or_default();
                secondary_cursors.retain(|cursor| *cursor <= previous_text.chars().count());
                let mut primary_source_cursor_before = None;
                let editor_mapping_id = text_edit_id.with("reference-lens-mapping");
                if let Some(mapping) = ui
                    .ctx()
                    .data(|data| data.get_temp::<ReferenceLensEditorMapping>(editor_mapping_id))
                    .filter(|mapping| {
                        mapping.path == document.path && mapping.source_text == previous_text
                    })
                    && let Some(mut state) =
                        egui::text_edit::TextEditState::load(ui.ctx(), text_edit_id)
                {
                    if let Some(mut range) = state.cursor.char_range() {
                        let primary_source = if mapping.display_to_source.is_empty() {
                            source_cursor_index(range.primary.index.0, &mapping.removed_display_indices)
                        } else {
                            source_index_from_display(range.primary.index.0, &mapping.display_to_source)
                        };
                        let secondary_source = if mapping.display_to_source.is_empty() {
                            source_cursor_index(range.secondary.index.0, &mapping.removed_display_indices)
                        } else {
                            source_index_from_display(range.secondary.index.0, &mapping.display_to_source)
                        };
                        primary_source_cursor_before = Some(primary_source);
                        range.primary.index.0 = display_index_from_source(
                            primary_source,
                            &editor_display.display_to_source,
                        );
                        range.secondary.index.0 = display_index_from_source(
                            secondary_source,
                            &editor_display.display_to_source,
                        );
                        let should_skip_virtual_row = ui.ctx().memory(|memory| {
                            memory.has_focus(text_edit_id)
                                && !completion_state.open
                                && !pointer_on_popup
                        }) && folded_ranges.is_empty()
                            && range.is_empty()
                            && ui.input(|input| input.key_pressed(egui::Key::ArrowUp));
                        if should_skip_virtual_row
                            && let Some(source_cursor) = source_cursor_before_reference_lens(
                                &editor_buffer,
                                &previous_text,
                                range.primary.index.0,
                                &active_reference_lenses,
                            )
                            && ui.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                            })
                        {
                            let target = egui::text::CCursor::new(display_index_from_source(
                                source_cursor,
                                &editor_display.display_to_source,
                            ));
                            range = egui::text::CCursorRange::one(target);
                        }
                        state.cursor.set_char_range(Some(range));
                        state.store(ui.ctx(), text_edit_id);
                    }
                }
                let minimap_visible = ui
                    .ctx()
                    .data(|data| data.get_temp::<bool>(egui::Id::unique("cairn.editor-minimap-visible")))
                    .unwrap_or(true);
                let line_font = TextStyle::Monospace.resolve(ui.style());
                let digit_width = ui
                    .painter()
                    .layout_no_wrap("0".to_owned(), line_font.clone(), Color32::WHITE)
                    .size()
                    .x;
                let max_line_number_digits = if editor_settings.line_numbers {
                    editor_display
                        .text
                        .split('\n')
                        .count()
                        .max(1)
                        .to_string()
                        .len()
                } else {
                    0
                };
                let max_line_number_width = digit_width * max_line_number_digits as f32;
                let desired_gutter_scale = (line_font.size / 14.5).clamp(0.75, 2.2);
                // Keep all gutter marks to the left of the widest line number,
                // even when zoom makes three-digit line numbers very wide.
                let gutter_scale = if editor_settings.line_numbers {
                    desired_gutter_scale.min(
                        ((127.0 - 20.0 - max_line_number_width - 2.0) / 44.0)
                            .clamp(0.5, 2.2),
                    )
                } else {
                    desired_gutter_scale
                };
                let editor_gutter_width = (20.0 + max_line_number_width + 44.0 * gutter_scale)
                    .ceil()
                    .clamp(96.0, 127.0) as i8;
                let mut editor_output = None;
                ui.horizontal_top(|ui| {
                    let minimap_width = if minimap_visible { 84.0 } else { 0.0 };
                    let editor_width = (ui.available_width() - minimap_width - 8.0).max(260.0);
                    let output = egui::TextEdit::multiline(&mut editor_buffer)
                        .id(text_edit_id)
                        .font(TextStyle::Monospace)
                        .code_editor()
                        .margin(egui::Margin {
                            left: editor_gutter_width,
                            ..egui::Margin::same(8)
                        })
                        .desired_width(editor_width)
                        .desired_rows(desired_rows)
                        .layouter(&mut layouter)
                        .show(ui);
                    if minimap_visible {
                        let minimap_height = output.response.rect.height().max(120.0);
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(minimap_width, minimap_height),
                            egui::Sense::click(),
                        );
                        let painter = ui.painter().with_clip_rect(rect);
                        painter.rect_filled(rect, 3.0, theme::panel());
                        painter.rect_stroke(
                            rect,
                            3.0,
                            egui::Stroke::new(1.0, theme::line()),
                            egui::StrokeKind::Inside,
                        );
                        let source_lines = previous_text.lines().collect::<Vec<_>>();
                        if !source_lines.is_empty() {
                            let row_height = rect.height() / source_lines.len() as f32;
                            let max_chars = source_lines
                                .iter()
                                .map(|line| line.chars().count())
                                .max()
                                .unwrap_or(1)
                                .max(1);
                            for (index, line) in source_lines.iter().enumerate() {
                                let y = rect.top() + index as f32 * row_height;
                                let width = ((line.chars().count() as f32 / max_chars as f32)
                                    * (rect.width() - 12.0))
                                    .clamp(2.0, rect.width() - 8.0);
                                let trimmed = line.trim_start();
                                let color = if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                                    Color32::from_rgb(81, 130, 101)
                                } else if trimmed.starts_with("class ")
                                    || trimmed.starts_with("struct ")
                                    || trimmed.starts_with("namespace ")
                                {
                                    theme::accent()
                                } else {
                                    Color32::from_rgb(108, 112, 123)
                                };
                                painter.rect_filled(
                                    egui::Rect::from_min_size(
                                        egui::pos2(rect.left() + 5.0, y),
                                        egui::vec2(width, row_height.max(1.0).min(2.0)),
                                    ),
                                    0.5,
                                    color,
                                );
                            }
                            if let Some(cursor) = output.cursor_range {
                                let source_cursor = source_index_from_display(
                                    cursor.primary.index.0,
                                    &editor_display.display_to_source,
                                );
                                let cursor_line = previous_text
                                    .chars()
                                    .take(source_cursor)
                                    .filter(|character| *character == '\n')
                                    .count();
                                let y = rect.top() + cursor_line as f32 * row_height;
                                painter.rect_filled(
                                    egui::Rect::from_min_size(
                                        egui::pos2(rect.left() + 2.0, y),
                                        egui::vec2(rect.width() - 4.0, row_height.max(2.0)),
                                    ),
                                    1.0,
                                    Color32::from_rgba_unmultiplied(78, 132, 255, 80),
                                );
                            }
                            if response.clicked()
                                && let Some(position) = ui.ctx().pointer_interact_pos()
                                && let Some(path) = document.path.clone()
                            {
                                let fraction = ((position.y - rect.top()) / rect.height().max(1.0))
                                    .clamp(0.0, 0.999_999);
                                let line = (fraction * source_lines.len() as f32).floor() as usize + 1;
                                if let Some(fold) = folded_ranges.iter().find(|fold| {
                                    fold.start_line < line && line <= fold.end_line
                                }) {
                                    if fold_path.as_deref() == Some(path.as_path()) {
                                        toggle_editor_fold(ui.ctx(), &path, *fold);
                                    }
                                }
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::unique("cairn.ctrl-click-definition-target"),
                                        (path, line),
                                    )
                                });
                            }
                        }
                    }
                    editor_output = Some(output);
                });
                let mut output = editor_output.expect("the code editor is rendered in its row");
                let zoom_delta = ui.input(|input| {
                    if input.modifiers.ctrl {
                        input.zoom_delta()
                    } else {
                        1.0
                    }
                });
                let mut requested_font_size = (output.response.hovered()
                    && zoom_delta.is_finite()
                    && (zoom_delta - 1.0).abs() > 0.001)
                    .then(|| editor_settings.font_size * zoom_delta);
                if output.response.has_focus() {
                    let keyboard_zoom = ui.input_mut(|input| {
                        if input.consume_key(
                            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                            egui::Key::Equals,
                        ) || input.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
                        {
                            Some(editor_settings.font_size + 1.0)
                        } else if input.consume_key(egui::Modifiers::CTRL, egui::Key::Minus) {
                            Some(editor_settings.font_size - 1.0)
                        } else if input.consume_key(egui::Modifiers::CTRL, egui::Key::Num0) {
                            Some(crate::settings::EditorSettings::default().font_size)
                        } else {
                            None
                        }
                    });
                    requested_font_size = keyboard_zoom.or(requested_font_size);
                }
                if let Some(font_size) = requested_font_size {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            egui::Id::unique("cairn.editor-font-zoom-request"),
                            font_size.clamp(
                                crate::settings::MIN_FONT_SIZE,
                                crate::settings::MAX_FONT_SIZE,
                            ),
                        )
                    });
                    ui.ctx().request_repaint();
                }
                ui.ctx().options_mut(|options| {
                    options.input_options.surrender_focus_on = surrender_focus_on;
                });

                let editor_display_text = editor_buffer.clone();
                let display_cursor_range = output.cursor_range;
                let (mut source_text, mut editor_mapping_after_edit) = if output.response.changed() {
                    (
                        apply_editor_display_edit(
                            &previous_text,
                            &editor_display.text,
                            &editor_display_text,
                            &editor_display.display_to_source,
                        ),
                        editor_mapping_after_edit(
                            &editor_display.text,
                            &editor_display_text,
                            &editor_display.display_to_source,
                        ),
                    )
                } else {
                    (previous_text.clone(), editor_display.display_to_source.clone())
                };
                let mut multi_cursor_edit_applied = false;
                if output.response.changed()
                    && !secondary_cursors.is_empty()
                    && let Some(range) = output.cursor_range.as_mut()
                {
                    multi_cursor_edit_applied = true;
                    let primary_source_cursor_after = source_index_from_display(
                        range.primary.index.0,
                        &editor_mapping_after_edit,
                    );
                    let (updated_text, updated_cursors, updated_primary) = apply_multi_cursor_edit(
                        &previous_text,
                        &source_text,
                        &secondary_cursors,
                        primary_source_cursor_before,
                        primary_source_cursor_after,
                    );
                    source_text = updated_text;
                    secondary_cursors = updated_cursors;
                    range.primary.index.0 = updated_primary;
                    let updated_display = editor_display_buffer(
                        &source_text,
                        &active_reference_lenses,
                        reference_lens_path.as_deref(),
                        &folded_ranges,
                    );
                    editor_mapping_after_edit = updated_display.display_to_source;
                    output.state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(display_index_from_source(
                            updated_primary,
                            &editor_mapping_after_edit,
                        )),
                    )));
                    output.state.clone().store(ui.ctx(), output.response.id);
                    ui.ctx().request_repaint();
                }
                document.text = source_text;
                if !multi_cursor_edit_applied
                    && let Some(range) = output.cursor_range.as_mut()
                {
                    range.primary.index.0 = source_index_from_display(range.primary.index.0, &editor_mapping_after_edit);
                    range.secondary.index.0 = source_index_from_display(range.secondary.index.0, &editor_mapping_after_edit);
                }
                let cursor_range = output.cursor_range;
                let visual_cursor_range = display_cursor_range.or(cursor_range);
                let editor_text_changed = output.response.changed() && document.text != previous_text;
                if output.response.clicked_by(egui::PointerButton::Primary)
                    && ui.input(|input| input.modifiers.alt)
                    && let Some(position) = ui.ctx().pointer_interact_pos()
                {
                    let display_cursor = output
                        .galley
                        .cursor_from_pos(position - output.galley_pos)
                        .index;
                    let source_cursor = source_index_from_display(
                        display_cursor.0,
                        &editor_mapping_after_edit,
                    );
                    if cursor_range.is_none_or(|range| range.primary.index.0 != source_cursor) {
                        if let Some(index) = secondary_cursors
                            .iter()
                            .position(|cursor| *cursor == source_cursor)
                        {
                            secondary_cursors.remove(index);
                        } else {
                            secondary_cursors.push(source_cursor);
                            secondary_cursors.sort_unstable();
                        }
                    }
                }
                if output.response.has_focus()
                    && !completion_state.open
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    })
                {
                    secondary_cursors.clear();
                }
                ui.ctx().data_mut(|data| {
                    data.insert_temp(multi_cursor_id, secondary_cursors.clone())
                });
                let cursor_position = cursor_range.as_ref().map(|range| range.primary.index.0);
                let call_context = cursor_position
                    .and_then(|cursor| call_signature_context(&document.text, cursor));
                let argument_index = call_context.as_ref().map(|(_, index)| *index);
                let signature_tracker_id = egui::Id::unique("cairn.lsp-signature-tracker");
                if output.response.has_focus()
                    && let (Some(path), Some(cursor), Some(active_parameter)) =
                        (document.path.as_deref(), cursor_position, argument_index)
                {
                    let (line, character) = lsp_position(&document.text, cursor);
                    let tracker = ui.ctx().data(|data| {
                        data.get_temp::<SignatureRequestTracker>(signature_tracker_id)
                    });
                    let same_request = tracker.as_ref().is_some_and(|tracker| {
                        tracker.path == path
                            && tracker.line == line
                            && tracker.character == character
                            && tracker.active_parameter == active_parameter
                            && tracker.text_length == document.text.chars().count()
                    });
                    if !same_request {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                signature_tracker_id,
                                SignatureRequestTracker {
                                    path: path.to_path_buf(),
                                    line,
                                    character,
                                    active_parameter,
                                    text_length: document.text.chars().count(),
                                    since: Instant::now(),
                                    requested: false,
                                },
                            );
                        });
                        ui.ctx().data_mut(|data| {
                            data.remove::<(
                                std::path::PathBuf,
                                usize,
                                usize,
                                usize,
                                usize,
                                SignatureHelpInfo,
                            )>(egui::Id::unique("cairn.lsp-signature-help"));
                        });
                    }
                    if let Some(mut tracker) = ui
                        .ctx()
                        .data(|data| data.get_temp::<SignatureRequestTracker>(signature_tracker_id))
                        .filter(|tracker| {
                            !tracker.requested
                                && tracker.since.elapsed() >= Duration::from_millis(90)
                        })
                    {
                        let trigger = document
                            .text
                            .chars()
                            .take(cursor)
                            .last()
                            .filter(|character| *character == ',');
                        if let Some(id) = language_server.request_signature_help(
                            path,
                            line,
                            character,
                            trigger,
                        ) {
                            tracker.requested = true;
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(signature_tracker_id, tracker);
                                data.insert_temp(
                                    egui::Id::unique("cairn.lsp-pending-signature-help"),
                                    (
                                        id,
                                        path.to_path_buf(),
                                        line,
                                        character,
                                        active_parameter,
                                        document.text.chars().count(),
                                    ),
                                );
                            });
                        }
                    }
                    ui.ctx().request_repaint_after(Duration::from_millis(100));
                } else {
                    ui.ctx().data_mut(|data| {
                        data.remove::<SignatureRequestTracker>(signature_tracker_id);
                        data.remove::<(
                            std::path::PathBuf,
                            usize,
                            usize,
                            usize,
                            usize,
                            SignatureHelpInfo,
                        )>(egui::Id::unique("cairn.lsp-signature-help"));
                    });
                }
                // Match the familiar Visual Studio navigation gesture. The
                // text editor's cursor range is updated on the same click, so
                // the language server receives the symbol under the pointer.
                let control_click = output.response.clicked_by(egui::PointerButton::Primary)
                    && ui.input(|input| input.modifiers.ctrl)
                    && !editor_text_changed;
                if control_click
                    && let (Some(path), Some(cursor)) =
                        (document.path.as_deref(), cursor_position)
                {
                    let (line, character) = lsp_position(&document.text, cursor);
                    if let Some(id) = language_server.request_definition(path, line, character) {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                egui::Id::unique("cairn.lsp-pending-definition"),
                                id,
                            );
                        });
                    } else {
                        let local_target = completion_query(&document.text, cursor)
                            .and_then(|(_, name)| {
                                project_symbols
                                    .iter()
                                    .find(|symbol| symbol.name == name)
                                    .map(|symbol| (symbol.path.clone(), symbol.line))
                            });
                        ui.ctx().data_mut(|data| {
                            if let Some(target) = local_target {
                                data.insert_temp(
                                    egui::Id::unique("cairn.ctrl-click-definition-target"),
                                    target,
                                );
                            } else {
                                data.insert_temp(
                                    egui::Id::unique("cairn.ctrl-click-definition-message"),
                                    "No project declaration found at that symbol. Install clangd (C++) or the Roslyn language server (C#) for semantic navigation.".to_owned(),
                                );
                            }
                        });
                    }
                }
                let cursor_moved = cursor_position.is_some()
                    && completion_state.cursor_position.is_some()
                    && cursor_position != completion_state.cursor_position;
                if debugging_active {
                    completion_state.armed = false;
                    completion_state.open = false;
                } else if editor_text_changed {
                    completion_state.armed = true;
                    completion_state.dismissed_word = None;
                } else if cursor_moved || !output.response.has_focus() {
                    completion_state.armed = false;
                    completion_state.open = false;
                }
                completion_state.cursor_position = cursor_position;
                if let Some(range) = cursor_range.as_ref() {
                    let position = lsp_position(&document.text, range.primary.index.0);
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::unique("cairn.lsp-cursor-position"), position);
                    });
                }
                let query = cursor_range
                    .and_then(|range| {
                        if !range.is_empty() {
                            return None;
                        }
                        completion_query(&document.text, range.primary.index.0)
                    })
                    .filter(|(range, word)| {
                        !word.is_empty() || is_member_completion(&document.text, range.start)
                    });
                let cursor_symbol = query
                    .as_ref()
                    .map(|(_, word)| word.clone())
                    .unwrap_or_default();
                let symbol_id = egui::Id::unique("cairn.editor-symbol-at-cursor");
                ui.ctx()
                    .data_mut(|data| data.insert_temp(symbol_id, cursor_symbol));

                let search_id = egui::Id::unique("cairn.find-in-files-state");
                let mut search_state = ui
                    .ctx()
                    .data(|data| data.get_temp::<SearchPanelState>(search_id))
                    .unwrap_or_default();
                if let (Some((relative_path, line)), Some(project_root)) =
                    (search_state.pending_line.as_ref(), project_root.as_ref())
                    && document.path.as_deref() == Some(project_root.join(relative_path).as_path())
                {
                    let line_start = document
                        .text
                        .lines()
                        .take(line.saturating_sub(1))
                        .map(|line| line.chars().count() + 1)
                        .sum::<usize>()
                        .min(document.text.chars().count());
                    let line_text = document.text.lines().nth(line.saturating_sub(1)).unwrap_or("");
                    let cursor = (line_start
                        + search_state.pending_column.unwrap_or(1).saturating_sub(1).min(line_text.chars().count()))
                        .min(document.text.chars().count());
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(cursor),
                        )));
                    output.state.clone().store(ui.ctx(), output.response.id);
                    output.response.request_focus();
                    output.response.scroll_to_me(None);
                    search_state.pending_line = None;
                    search_state.pending_column = None;
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(search_id, search_state));
                }
                if !debugging_active && let Some((_, word)) = query.as_ref() {
                    if completion_state.last_requested_word.as_deref() != Some(word.as_str()) {
                        completion_state.last_requested_word = Some(word.clone());
                        if let (Some(path), Some(range)) =
                            (document.path.as_deref(), cursor_range.as_ref())
                        {
                            let (line, character) =
                                lsp_position(&document.text, range.primary.index.0);
                            if let Some(id) =
                                language_server.request_completion(path, line, character)
                            {
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::unique("cairn.lsp-pending-completion"),
                                        (id, word.clone()),
                                    );
                                });
                            }
                        }
                    }
                } else {
                    completion_state.last_requested_word = None;
                }
                let (word_range, word, member_access) = query.map_or_else(
                    || (None, String::new(), false),
                    |(range, word)| {
                        let member_access = is_member_completion(&document.text, range.start);
                        (Some(range), word, member_access)
                    },
                );
                let mut suggestions = if completion_state.armed
                    && output.response.has_focus()
                    && !debugging_active
                    && !dismiss_requested
                {
                    word_range.as_ref().map_or_else(Vec::new, |range| {
                        let typed = if member_access {
                            member_suggestions(&language, &document.text, range.start, &word)
                        } else {
                            None
                        };
                        match typed {
                            Some(typed) if !typed.is_empty() => typed,
                            _ => completion_suggestions(
                                &language,
                                &word,
                                &project_symbols,
                                member_access,
                            ),
                        }
                    })
                } else {
                    Vec::new()
                };
                if let Some((_remote_word, remote_items)) = ui
                    .ctx()
                    .data(|data| {
                        data.get_temp::<(String, Vec<(String, String)>)>(egui::Id::unique(
                            "cairn.lsp-completions",
                        ))
                    })
                    .filter(|(remote_word, _)| remote_word == &word)
                {
                    for (insert, detail) in remote_items {
                        if !suggestions
                            .iter()
                            .any(|suggestion| suggestion.insert == insert)
                        {
                            suggestions.push(
                                egui::Suggestion::new(insert)
                                    .description(format!("Language server · {detail}")),
                            );
                        }
                    }
                }
                if !word.is_empty()
                    && suggestions
                        .iter()
                        .any(|suggestion| suggestion.insert.eq_ignore_ascii_case(&word))
                {
                    completion_state.armed = false;
                    completion_state.open = false;
                }
                if completion_state.word != word || completion_state.member_access != member_access
                {
                    completion_state.selected = 0;
                    if completion_state.dismissed_word.as_deref() != Some(word.as_str()) {
                        completion_state.dismissed_word = None;
                    }
                }
                completion_state.word.clone_from(&word);
                completion_state.member_access = member_access;
                if dismiss_requested {
                    completion_state.dismissed_word = Some(word.clone());
                    completion_state.armed = false;
                }

                let mut accepted_index = if accept_requested && !suggestions.is_empty() {
                    Some(completion_state.selected.min(suggestions.len() - 1))
                } else {
                    None
                };
                let mut current_signature: Option<(usize, SignatureHelpInfo, egui::Pos2)> = None;
                if !debugging_active
                    && output.response.has_focus()
                    && let (Some(path), Some(cursor_range), Some(active_parameter)) = (
                        document.path.as_deref(),
                        cursor_range.as_ref(),
                        argument_index,
                    )
                {
                    let (line, character) =
                        lsp_position(&document.text, cursor_range.primary.index.0);
                    let expected_length = document.text.chars().count();
                    let help = ui.ctx().data(|data| {
                        data.get_temp::<(
                            std::path::PathBuf,
                            usize,
                            usize,
                            usize,
                            usize,
                            SignatureHelpInfo,
                        )>(egui::Id::unique("cairn.lsp-signature-help"))
                    });
                    let remote_info = help.filter(|(
                        stored_path,
                        stored_line,
                        stored_character,
                        stored_parameter,
                        stored_length,
                        _,
                    )| {
                        stored_path == path
                            && *stored_line == line
                            && *stored_character == character
                            && *stored_parameter == active_parameter
                            && *stored_length == expected_length
                    }).map(|(_, _, _, _, _, info)| info);
                    let info = remote_info.or_else(|| {
                        let (callee, _) = call_context.as_ref()?;
                        let cache_id = egui::Id::unique("cairn.local-signature-cache");
                        let cached = ui.ctx().data(|data| {
                            data.get_temp::<(
                                std::path::PathBuf,
                                String,
                                SignatureHelpInfo,
                            )>(cache_id)
                        });
                        let active_path = document.path.as_ref()?;
                        if let Some((path, cached_callee, info)) = cached
                            && path == *active_path
                            && cached_callee == *callee
                        {
                            Some(info)
                        } else {
                            let info = local_signature_help(
                                &project_symbols,
                                project_root.as_deref(),
                                document.path.as_deref(),
                                &document.text,
                                callee,
                            )?;
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(
                                    cache_id,
                                    (active_path.clone(), callee.clone(), info.clone()),
                                )
                            });
                            Some(info)
                        }
                    });
                    if let Some(info) = info {
                        let cursor = visual_cursor_range.unwrap_or(*cursor_range).primary;
                        let rect = output.galley.pos_from_cursor(cursor);
                        let anchor = output.galley_pos + rect.left_bottom().to_vec2();
                        current_signature = Some((active_parameter, info, anchor));
                    }
                }
                if !debugging_active
                    && completion_state.armed
                    && !dismiss_requested
                    && output.response.has_focus()
                    && !suggestions.is_empty()
                    && completion_state.dismissed_word.as_deref() != Some(word.as_str())
                    && let (Some(_), Some(cursor_range)) = (word_range.as_ref(), visual_cursor_range)
                {
                    let cursor_rect = output.galley.pos_from_cursor(cursor_range.primary);
                    let cursor_position = output.galley_pos + cursor_rect.left_bottom().to_vec2();
                    let popup_width = suggestions
                        .iter()
                        .map(|suggestion| {
                            suggestion.insert.chars().count() as f32 * 8.5 + 300.0
                        })
                        .fold(380.0_f32, f32::max)
                        .min(680.0);
                    let popup = egui::Popup::new(
                        completion_id,
                        ui.ctx().clone(),
                        cursor_position,
                        output.response.layer_id,
                    )
                    .kind(egui::PopupKind::Popup)
                    .open(true)
                    .align(egui::RectAlign::TOP_START)
                    .align_alternatives(&[egui::RectAlign::BOTTOM_START])
                    .width(popup_width)
                    .layout(egui::Layout::top_down(egui::Align::Min))
                    .close_behavior(egui::PopupCloseBehavior::IgnoreClicks);
                    let popup_result = popup.show(|ui| {
                        ui.set_min_width(popup_width);
                        ui.set_max_width(popup_width);
                        ui.set_min_height(72.0);
                        egui::ScrollArea::vertical()
                            .id_salt(("completion-suggestions", document.title.as_str()))
                            .max_height(280.0)
                            .show(ui, |ui| {
                                for (index, suggestion) in suggestions.iter().enumerate() {
                                    let selected = index == completion_state.selected;
                                    let response = ui.add(
                                        egui::Button::selectable(
                                            selected,
                                            suggestion.content.clone(),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Extend)
                                        .min_size(egui::vec2(ui.available_width(), 24.0)),
                                    );
                                    if response.clicked() {
                                        accepted_index = Some(index);
                                    }
                                }
                            });
                    });
                    completion_state.open = popup_result.is_some();
                } else {
                    completion_state.open = false;
                }
                if let (Some(index), Some(range)) = (accepted_index, word_range.as_ref()) {
                    if let Some(suggestion) = suggestions.get(index) {
                        completion_state.dismissed_word = Some(suggestion.insert.clone());
                        completion_state.armed = false;
                        let start = char_to_byte_index(&document.text, range.start);
                        let end = char_to_byte_index(&document.text, range.end);
                        document.text.replace_range(start..end, &suggestion.insert);
                        let mut cursor = range.start + suggestion.insert.chars().count();
                        if language == "cs"
                            && let Some(namespace) = project_symbols
                                .iter()
                                .find(|symbol| symbol.name == suggestion.insert)
                                .and_then(|symbol| symbol.namespace.as_deref())
                                .filter(|namespace| {
                                    declared_namespace(&document.text).as_deref()
                                        != Some(*namespace)
                                })
                        {
                            let directive = format!("using {namespace};");
                            if !document.text.lines().any(|line| line.trim() == directive) {
                                let using_line = format!("{directive}\n");
                                document.text.insert_str(0, &using_line);
                                cursor += using_line.chars().count();
                            }
                        } else if language == "cpp"
                            && let Some(include) = project_root
                                .as_deref()
                                .zip(active_document_path.as_deref())
                                .and_then(|(root, active_path)| {
                                    let active_relative = active_path.strip_prefix(root).ok()?;
                                    project_symbols
                                        .iter()
                                        .find(|symbol| {
                                            symbol.name == suggestion.insert
                                                && symbol.path != active_relative
                                                && matches!(
                                                    symbol
                                                        .path
                                                        .extension()
                                                        .and_then(|ext| ext.to_str()),
                                                    Some("h" | "hpp")
                                                )
                                        })
                                        .and_then(|symbol| {
                                            relative_include_path(active_relative, &symbol.path)
                                        })
                                })
                        {
                            let directive = format!("#include \"{include}\"");
                            if !document.text.lines().any(|line| line.trim() == directive) {
                                let include_line = format!("{directive}\n");
                                document.text.insert_str(0, &include_line);
                                cursor += include_line.chars().count();
                            }
                        }
                        let updated_display = editor_display_buffer(
                            &document.text,
                            &active_reference_lenses,
                            reference_lens_path.as_deref(),
                            &folded_ranges,
                        );
                        output
                            .state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(display_index_from_source(
                                    cursor,
                                    &updated_display.display_to_source,
                                )),
                            )));
                        editor_mapping_after_edit = updated_display.display_to_source;
                        output.state.clone().store(ui.ctx(), output.response.id);
                        output.response.request_focus();
                        document.dirty = true;
                        refresh_symbols = true;
                    }
                    completion_state.open = false;
                } else {
                    completion_state.open = completion_state.armed
                        && output.response.has_focus()
                        && !suggestions.is_empty()
                        && !dismiss_requested;
                    if !suggestions.is_empty() {
                        completion_state.selected =
                            completion_state.selected.min(suggestions.len() - 1);
                    }
                }
                ui.ctx()
                    .data_mut(|memory| memory.insert_temp(completion_id, completion_state));

                if output.response.changed()
                    && accepted_index.is_none()
                    && let Some(range) = output.cursor_range
                    && range.is_empty()
                    && let Some(cursor) = editor_settings
                        .auto_indent
                        .then(|| {
                            auto_indent_edit(
                                &previous_text,
                                &mut document.text,
                                range.primary.index.0,
                                editor_settings.tab_width,
                                editor_settings.use_tabs,
                            )
                        })
                        .flatten()
                        .or_else(|| {
                            editor_settings
                                .auto_pairs
                                .then(|| {
                                    auto_pair_edit(
                                        &previous_text,
                                        &mut document.text,
                                        range.primary.index.0,
                                    )
                                })
                                .flatten()
                        })
                {
                    let updated_display = editor_display_buffer(
                        &document.text,
                        &active_reference_lenses,
                        reference_lens_path.as_deref(),
                        &folded_ranges,
                    );
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(display_index_from_source(
                                cursor,
                                &updated_display.display_to_source,
                            )),
                        )));
                    editor_mapping_after_edit = updated_display.display_to_source;
                    output.state.clone().store(ui.ctx(), output.response.id);
                }
                let line_color = Color32::from_rgb(104, 119, 139);
                let painter = ui.painter().with_clip_rect(output.response.rect);
                let is_debug_file = document.path.as_ref().is_some_and(|path| {
                    debug_location.as_ref().is_some_and(|location| {
                        path == &location.file
                            || project_root
                                .as_ref()
                                .is_some_and(|root| root.join(path) == location.file)
                    })
                });
                let mut source_chars = editor_display_text.chars();
                let mut display_line_number = 1usize;
                let mut line_number = 1;
                let mut line_column = 1;
                let mut last_breakpoint_line = 0;
                let mut hovered_diagnostics = Vec::<Diagnostic>::new();
                let mut diagnostic_anchor = None;
                let mut hovered_symbol: Option<(std::path::PathBuf, usize, usize, usize)> = None;
                let unified_card_rect_id = egui::Id::unique("cairn.editor-unified-hover-card-rect");
                let pointer_position = ui.ctx().pointer_interact_pos();
                let pointer_over_card = ui
                    .ctx()
                    .data(|data| data.get_temp::<egui::Rect>(unified_card_rect_id))
                    .is_some_and(|rect| pointer_position.is_some_and(|position| rect.contains(position)))
                    || pointer_position.is_some_and(|position| {
                        ui.ctx().layer_id_at(position)
                            == Some(egui::LayerId::new(
                                egui::Order::Tooltip,
                                egui::Id::unique("cairn.editor-unified-hover-card"),
                            ))
                    });
                let diagnostic_cache_id = egui::Id::unique("cairn.editor-unified-hover-card-diagnostics");
                let signature_cache_id = egui::Id::unique("cairn.editor-unified-hover-card-signature");
                if !output.response.changed() {
                    for cursor in &secondary_cursors {
                        let display_index = display_index_from_source(
                            *cursor,
                            &editor_mapping_after_edit,
                        );
                        let cursor_rect = output
                            .galley
                            .pos_from_cursor(egui::text::CCursor::new(display_index));
                        let x = output.galley_pos.x + cursor_rect.left();
                        let top = output.galley_pos.y + cursor_rect.top();
                        let bottom = output.galley_pos.y + cursor_rect.bottom();
                        painter.line_segment(
                            [egui::pos2(x, top), egui::pos2(x, bottom)],
                            egui::Stroke::new(1.4, theme::accent_hover()),
                        );
                    }
                }
                for row in &output.galley.rows {
                    let display_line = editor_display_text
                        .split_inclusive('\n')
                        .nth(display_line_number.saturating_sub(1))
                        .unwrap_or_default();
                    let display_line_content = display_line
                        .strip_suffix('\n')
                        .unwrap_or(display_line)
                        .strip_suffix('\r')
                        .unwrap_or_else(|| display_line.strip_suffix('\n').unwrap_or(display_line));
                    let virtual_lens_row = display_line_content
                        .chars()
                        .eq([REFERENCE_LENS_ROW_MARKER])
                        && active_reference_lenses
                            .iter()
                            .any(|lens| lens.line == line_number);
                    if virtual_lens_row {
                        if let Some(lens) = active_reference_lenses
                            .iter()
                            .find(|lens| lens.line == line_number)
                        {
                            let label = format!(
                                "{} {}",
                                lens.count,
                                if lens.count == 1 { "reference" } else { "references" }
                            );
                            let label_position =
                                egui::pos2(output.galley_pos.x, output.galley_pos.y + row.pos.y);
                            let reference_font_size = line_font.size * (11.0 / 14.5);
                            let galley = painter.layout_no_wrap(
                                label,
                                egui::FontId::proportional(reference_font_size),
                                theme::accent(),
                            );
                            let row_rect = egui::Rect::from_min_size(
                                label_position,
                                galley.size(),
                            );
                            let lens_response = ui.interact(
                                row_rect,
                                ui.make_persistent_id(("reference-lens", &lens.path, lens.line)),
                                egui::Sense::click(),
                            );
                            painter.galley(
                                label_position,
                                galley,
                                if lens_response.hovered() {
                                    theme::text()
                                } else {
                                    theme::accent()
                                },
                            );
                            if lens_response.clicked() {
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::unique("cairn.lsp-cursor-position"),
                                        (lens.line - 1, lens.column),
                                    );
                                    data.remove::<ReferenceResults>(egui::Id::unique(
                                        "cairn.lsp-references",
                                    ));
                                    data.remove::<usize>(egui::Id::unique(
                                        "cairn.lsp-reference-usage-count",
                                    ));
                                });
                                if let Some(references_id) = language_server.request_references(
                                    &lens.path,
                                    lens.line - 1,
                                    lens.column,
                                    false,
                                ) {
                                    ui.ctx().data_mut(|data| {
                                        data.insert_temp(
                                            egui::Id::unique("cairn.lsp-reference-declaration-filter"),
                                            (
                                                lens.path.clone(),
                                                lens.line,
                                                lens.protocol_column,
                                            ),
                                        );
                                        data.insert_temp(
                                            egui::Id::unique("cairn.lsp-pending-references"),
                                            (references_id, None::<u64>),
                                        );
                                    });
                                }
                            }
                        }
                        for _ in display_line.chars() {
                            let _ = source_chars.next();
                        }
                        if row.ends_with_newline {
                            display_line_number += 1;
                        }
                        line_column = 1;
                        continue;
                    }
                    if display_line_content.starts_with(FOLD_ROW_MARKER)
                        && let Some(range) = folded_ranges
                            .iter()
                            .find(|range| range.start_line.saturating_add(1) == line_number)
                    {
                        let position = egui::pos2(
                            output.galley_pos.x,
                            output.galley_pos.y + row.pos.y,
                        );
                        let label = display_line_content
                            .trim_start_matches(FOLD_ROW_MARKER)
                            .trim();
                        let rect = egui::Rect::from_min_size(
                            position,
                            egui::vec2(
                                (output.response.rect.right() - position.x).max(180.0),
                                row.size.y.max(line_font.size),
                            ),
                        );
                        let response = ui.interact(
                            rect,
                            ui.make_persistent_id(("fold-placeholder", &document.title, range.start_line)),
                            egui::Sense::click(),
                        );
                        painter.rect_filled(rect, 2.0, theme::panel());
                        painter.text(
                            position + egui::vec2(5.0, 0.0),
                            egui::Align2::LEFT_TOP,
                            label,
                            egui::FontId::proportional(12.0),
                            theme::muted(),
                        );
                        if response.clicked()
                            && let Some(path) = fold_path.as_deref()
                        {
                            toggle_editor_fold(ui.ctx(), path, *range);
                        }
                        for _ in display_line.chars() {
                            let _ = source_chars.next();
                        }
                        display_line_number += 1;
                        line_number = range.end_line.saturating_add(1);
                        line_column = 1;
                        continue;
                    }
                    // Wrapped rows share one source line. Give each logical
                    // line exactly one breakpoint hit target and marker.
                    if last_breakpoint_line != line_number {
                        last_breakpoint_line = line_number;
                        if let Some(range) = foldable.iter().find(|range| range.start_line == line_number) {
                            let fold_rect = egui::Rect::from_min_size(
                                egui::pos2(output.galley_pos.x - 14.0, output.galley_pos.y + row.pos.y),
                                egui::vec2(12.0, line_font.size + 2.0),
                            );
                            let fold_response = ui.interact(
                                fold_rect,
                                ui.make_persistent_id(("fold-gutter", &document.title, line_number)),
                                egui::Sense::click(),
                            );
                            let folded = folded_ranges.contains(range);
                            painter.text(
                                fold_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                if folded { "▸" } else { "⌄" },
                                egui::FontId::proportional(line_font.size * 0.9),
                                if fold_response.hovered() { theme::accent_hover() } else { line_color },
                            );
                            let fold_clicked = fold_response.clicked();
                            fold_response.on_hover_text(if folded { "Expand this code block" } else { "Collapse this code block" });
                            if fold_clicked
                                && let Some(path) = fold_path.as_deref()
                            {
                                toggle_editor_fold(ui.ctx(), path, *range);
                            }
                        }
                        let line_number_width = digit_width * line_number.to_string().len() as f32;
                        let line_number_left = output.galley_pos.x - 20.0 - line_number_width;
                        let marker_size = 12.0 * gutter_scale;
                        let marker_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                line_number_left - 14.0 * gutter_scale - marker_size * 0.5,
                                output.galley_pos.y + row.pos.y,
                            ),
                            egui::vec2(marker_size, line_font.size + 2.0),
                        );
                        let marker_response = ui.interact(
                                marker_rect,
                                ui.make_persistent_id(("breakpoint", &document.title, line_number)),
                                egui::Sense::click(),
                            );
                        if marker_response.clicked() {
                            if let Some(position) = document
                                .breakpoints
                                .iter()
                                .position(|&line| line == line_number)
                            {
                                document.breakpoints.remove(position);
                            } else {
                                document.breakpoints.push(line_number);
                                document.breakpoints.sort_unstable();
                            }
                            breakpoints_changed = true;
                        }
                        if document.breakpoints.contains(&line_number) {
                            painter.circle_filled(
                                marker_rect.center(),
                                4.2 * gutter_scale,
                                Color32::from_rgb(239, 68, 68),
                            );
                        } else if marker_response.hovered() {
                            painter.circle_filled(
                                marker_rect.center(),
                                4.0 * gutter_scale,
                                Color32::from_rgba_unmultiplied(255, 95, 95, 150),
                            );
                        }
                    }
                    let line_diagnostics = editor_diagnostics
                        .iter()
                        .filter(|diagnostic| {
                            diagnostic.line.is_some_and(|start| {
                                if let Some(end) = diagnostic.end_line {
                                    (start..=end).contains(&line_number)
                                } else {
                                    start == line_number
                                }
                            })
                        })
                        .collect::<Vec<_>>();
                        if !line_diagnostics.is_empty() {
                        let severity = line_diagnostics
                            .iter()
                            .map(|diagnostic| diagnostic.severity)
                            .min_by_key(|severity| match severity {
                                DiagnosticSeverity::Error => 0,
                                DiagnosticSeverity::Warning => 1,
                                DiagnosticSeverity::Information => 2,
                                DiagnosticSeverity::Hint => 3,
                            })
                            .unwrap_or(DiagnosticSeverity::Information);
                        let color = diagnostic_color(severity);
                        // egui's multiline editor has no native overview ruler, so paint
                        // a compact severity tick at the text viewport's right edge.
                        let ruler_y = output.galley_pos.y + row.pos.y;
                        let ruler_rect = egui::Rect::from_min_size(
                            egui::pos2(output.response.rect.right() - 4.0 * gutter_scale, ruler_y),
                            egui::vec2(3.0 * gutter_scale, row.size.y.max(line_font.size) * 0.65),
                        );
                        painter.rect_filled(ruler_rect, 1.0, color);
                        let marker_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                output.galley_pos.x
                                    - 20.0
                                    - digit_width * line_number.to_string().len() as f32
                                    - 8.0 * gutter_scale,
                                output.galley_pos.y + row.pos.y,
                            ),
                            egui::vec2(5.0 * gutter_scale, row.size.y.max(line_font.size)),
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_size(marker_rect.left_top(), egui::vec2(3.0 * gutter_scale, marker_rect.height())),
                            1.0,
                            color,
                        );
                        let diagnostic_response = ui.interact(
                                marker_rect,
                                ui.make_persistent_id(("diagnostic-gutter", &document.title, line_number)),
                                egui::Sense::hover(),
                            );
                        if diagnostic_response.hovered() && !pointer_over_card {
                            diagnostic_anchor = Some(marker_rect.right_top());
                            for diagnostic in &line_diagnostics {
                                if !hovered_diagnostics.contains(diagnostic) {
                                    hovered_diagnostics.push((*diagnostic).clone());
                                }
                            }
                        }
                    }
                    if is_debug_file
                        && debug_location
                            .as_ref()
                            .is_some_and(|location| location.line == line_number)
                    {
                        let line_number_left = output.galley_pos.x
                            - 20.0
                            - digit_width * line_number.to_string().len() as f32;
                        let center = egui::pos2(
                            line_number_left - 35.0 * gutter_scale,
                            output.galley_pos.y + row.pos.y + line_font.size * 0.62,
                        );
                        painter.text(
                            center,
                            egui::Align2::CENTER_CENTER,
                            "▶",
                            egui::FontId::proportional(14.5 * gutter_scale * 0.75),
                            Color32::from_rgb(70, 155, 255),
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(
                                    line_number_left - 27.0 * gutter_scale,
                                    output.galley_pos.y + row.pos.y,
                                ),
                                egui::vec2(3.0 * gutter_scale, row.size.y.max(line_font.size)),
                            ),
                            1.0,
                            Color32::from_rgb(70, 155, 255),
                        );
                    }
                    if editor_settings.line_numbers {
                        painter.text(
                            egui::pos2(output.galley_pos.x - 20.0, output.galley_pos.y + row.pos.y),
                            egui::Align2::RIGHT_TOP,
                            line_number.to_string(),
                            line_font.clone(),
                            line_diagnostics
                                .first()
                                .map_or(line_color, |diagnostic| diagnostic_color(diagnostic.severity)),
                        );
                    }
                    for glyph in &row.glyphs {
                        if glyph.chr == REFERENCE_LENS_ROW_MARKER {
                            let _ = source_chars.next();
                            continue;
                        }
                        if (glyph.chr.is_alphanumeric() || glyph.chr == '_')
                            && let Some(path) = document.path.as_ref()
                            && let Some(line_text) = document.text.lines().nth(line_number - 1)
                            && let Some(token) = identifier_range(line_text, line_column - 1)
                        {
                            let word_rect = egui::Rect::from_min_size(
                                egui::pos2(
                                    output.galley_pos.x + glyph.pos.x,
                                    output.galley_pos.y + row.pos.y,
                                ),
                                egui::vec2(glyph.advance_width.max(3.0), row.size.y.max(line_font.size)),
                            );
                            let word_response = ui.interact(
                                word_rect,
                                ui.make_persistent_id((
                                    "lsp-symbol-hover",
                                    path,
                                    line_number,
                                    token.start,
                                    line_column,
                                )),
                                egui::Sense::hover(),
                            );
                            if word_response.hovered() && !pointer_over_card {
                                hovered_symbol = Some((
                                    path.clone(),
                                    line_number - 1,
                                    token.start,
                                    token.end,
                                ));
                                let cached = ui.ctx().data(|data| {
                                    data.get_temp::<(
                                        std::path::PathBuf,
                                        usize,
                                        usize,
                                        String,
                                    )>(egui::Id::unique("cairn.lsp-hover-tooltip"))
                                });
                                if let Some((cached_path, cached_line, cached_column, text)) = cached
                                    && cached_path == *path
                                    && cached_line == line_number - 1
                                    && cached_column == token.start
                                {
                                    let pointer = ui.ctx().pointer_latest_pos().unwrap_or(word_rect.center());
                                    let screen = ui.ctx().content_rect();
                                    let anchor = egui::pos2(
                                        if pointer.x + 480.0 > screen.right() { pointer.x - 475.0 } else { pointer.x + 12.0 },
                                        if pointer.y + 220.0 > screen.bottom() { pointer.y - 220.0 } else { pointer.y + 16.0 },
                                    );
                                    ui.ctx().data_mut(|data| {
                                        data.insert_temp(
                                            egui::Id::unique("cairn.lsp-hover-popup"),
                                            HoverPopupState {
                                                path: cached_path,
                                                line: cached_line,
                                                token_start: cached_column,
                                                text,
                                                anchor,
                                                last_active: Instant::now(),
                                            },
                                        )
                                    });
                                }
                            }
                        }
                        let marked = editor_diagnostics
                            .iter()
                            .filter(|diagnostic| diagnostic.contains_position(line_number, line_column))
                            .collect::<Vec<_>>();
                        if !marked.is_empty() {
                            let severity = marked
                                .iter()
                                .map(|diagnostic| diagnostic.severity)
                                .min_by_key(|severity| match severity {
                                    DiagnosticSeverity::Error => 0,
                                    DiagnosticSeverity::Warning => 1,
                                    DiagnosticSeverity::Information => 2,
                                    DiagnosticSeverity::Hint => 3,
                                })
                                .unwrap_or(DiagnosticSeverity::Information);
                            let color = diagnostic_color(severity);
                            let x1 = output.galley_pos.x + glyph.pos.x;
                            let x2 = x1 + glyph.advance_width.max(2.0);
                            let y = output.galley_pos.y + row.pos.y + row.size.y - 2.0;
                            let mut x = x1;
                            let mut up = true;
                            while x < x2 {
                                let next = (x + 3.0).min(x2);
                                painter.line_segment(
                                    [egui::pos2(x, y + if up { 1.0 } else { -1.0 }),
                                     egui::pos2(next, y + if up { -1.0 } else { 1.0 })],
                                    egui::Stroke::new(1.2, color),
                                );
                                x = next;
                                up = !up;
                            }
                            let hover_rect = egui::Rect::from_min_max(
                                egui::pos2(x1, y - 5.0),
                                egui::pos2(x2.max(x1 + 3.0), y + 2.0),
                            );
                            let diagnostic_response = ui.interact(
                                    hover_rect,
                                    ui.make_persistent_id(("diagnostic-underline", &document.title, line_number, line_column)),
                                    egui::Sense::hover(),
                                );
                            if diagnostic_response.hovered() && !pointer_over_card {
                                diagnostic_anchor = Some(hover_rect.right_top());
                                for diagnostic in marked {
                                    if !hovered_diagnostics.contains(diagnostic) {
                                        hovered_diagnostics.push((*diagnostic).clone());
                                    }
                                }
                            }
                        }
                        if editor_settings.show_whitespace_spaces && glyph.chr == ' ' {
                            painter.text(
                                egui::pos2(
                                    output.galley_pos.x + glyph.pos.x + glyph.advance_width * 0.5,
                                    output.galley_pos.y + row.pos.y + glyph.pos.y,
                                ),
                                egui::Align2::CENTER_CENTER,
                                "·",
                                line_font.clone(),
                                line_color,
                            );
                        } else if editor_settings.show_whitespace_tabs && glyph.chr == '\t' {
                            painter.text(
                                egui::pos2(
                                    output.galley_pos.x + glyph.pos.x + glyph.advance_width * 0.5,
                                    output.galley_pos.y + row.pos.y + glyph.pos.y,
                                ),
                                egui::Align2::CENTER_CENTER,
                                "→",
                                line_font.clone(),
                                line_color,
                            );
                        }
                        let _ = source_chars.next();
                        line_column += 1;
                    }
                    if row.ends_with_newline {
                        let _ = source_chars.next();
                        line_number += 1;
                        display_line_number += 1;
                        line_column = 1;
                    }
                }
                if !pointer_over_card
                    && let Some((_, line, token_start, _)) = hovered_symbol.as_ref()
                {
                    for diagnostic in editor_diagnostics.iter().filter(|diagnostic| {
                        diagnostic.contains_position(line + 1, token_start + 1)
                    }) {
                        if !hovered_diagnostics.contains(diagnostic) {
                            hovered_diagnostics.push(diagnostic.clone());
                        }
                    }
                }
                if let Some((path, line, token_start, _token_end)) = hovered_symbol.as_ref() {
                    let tracker_id = egui::Id::unique("cairn.lsp-hover-debounce");
                    let tracker = ui.ctx().data(|data| data.get_temp::<HoverDebounce>(tracker_id));
                    let mut tracker = match tracker {
                        Some(tracker)
                            if tracker.path == *path
                                && tracker.line == *line
                                && tracker.token_start == *token_start => tracker,
                        _ => HoverDebounce {
                            path: path.clone(),
                            line: *line,
                            token_start: *token_start,
                            since: Instant::now(),
                            requested: false,
                        },
                    };
                    if !tracker.requested && tracker.since.elapsed() >= Duration::from_millis(180)
                        && let Some(request_id) =
                            language_server.request_hover(path, *line, *token_start)
                    {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                egui::Id::unique("cairn.lsp-pending-mouse-hover"),
                                (request_id, path.clone(), *line, *token_start),
                            )
                        });
                        tracker.requested = true;
                    }
                    if !tracker.requested {
                        ui.ctx().request_repaint_after(Duration::from_millis(50));
                    }
                    ui.ctx().data_mut(|data| data.insert_temp(tracker_id, tracker));
                }
                let popup_id = egui::Id::unique("cairn.lsp-hover-popup");
                let mut hover_state = ui.ctx().data(|data| data.get_temp::<HoverPopupState>(popup_id));
                if hovered_symbol.is_none() && !pointer_over_card {
                    hover_state = None;
                    ui.ctx().data_mut(|data| {
                        data.remove::<HoverPopupState>(popup_id);
                        data.remove::<HoverDebounce>(egui::Id::unique("cairn.lsp-hover-debounce"));
                        data.remove::<(u64, std::path::PathBuf, usize, usize)>(egui::Id::unique(
                            "cairn.lsp-pending-mouse-hover",
                        ));
                        data.remove::<(
                            std::path::PathBuf,
                            usize,
                            usize,
                            String,
                        )>(egui::Id::unique("cairn.lsp-hover-tooltip"));
                    });
                }
                if let (Some((path, line, token_start, _)), Some(state)) =
                    (hovered_symbol.as_ref(), hover_state.as_mut())
                    && !pointer_over_card
                    && (state.path != *path || state.line != *line || state.token_start != *token_start)
                {
                    hover_state = None;
                }
                let mut hover_state = hover_state.filter(|state| {
                    state.last_active.elapsed() < Duration::from_millis(120) || pointer_over_card
                });
                let signature = if current_signature.is_some() {
                    current_signature
                } else if pointer_over_card {
                    ui.ctx().data(|data| {
                        data.get_temp::<(usize, SignatureHelpInfo, egui::Pos2)>(signature_cache_id)
                    })
                } else {
                    None
                };
                if !hovered_diagnostics.is_empty() {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(diagnostic_cache_id, hovered_diagnostics.clone())
                    });
                } else if pointer_over_card {
                    hovered_diagnostics = ui
                        .ctx()
                        .data(|data| data.get_temp::<Vec<Diagnostic>>(diagnostic_cache_id))
                        .unwrap_or_default();
                }
                if let Some(signature) = signature.as_ref() {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(signature_cache_id, signature.clone())
                    });
                }
                let symbol_text = hover_state.as_ref().map(|state| state.text.as_str());
                let has_card = signature.is_some()
                    || symbol_text.is_some()
                    || !hovered_diagnostics.is_empty();
                if has_card {
                    let mut scroll_identity = String::new();
                    if let Some((active_parameter, info, _)) = signature.as_ref() {
                        scroll_identity.push_str(&info.label);
                        scroll_identity.push_str(&format!("|argument:{active_parameter}"));
                        for overload in &info.overloads {
                            scroll_identity.push_str("|");
                            scroll_identity.push_str(&overload.label);
                        }
                    }
                    if let Some(text) = symbol_text {
                        scroll_identity.push_str("|symbol:");
                        scroll_identity.push_str(text);
                    }
                    for diagnostic in &hovered_diagnostics {
                        scroll_identity.push('|');
                        scroll_identity.push_str(&diagnostic.message);
                        scroll_identity.push_str(diagnostic.source.as_deref().unwrap_or_default());
                    }
                    let anchor = signature
                        .as_ref()
                        .map(|(_, _, anchor)| *anchor)
                        .or_else(|| hover_state.as_ref().map(|state| state.anchor))
                        .or(diagnostic_anchor)
                        .unwrap_or_else(|| ui.ctx().pointer_latest_pos().unwrap_or(output.response.rect.center()));
                    let screen = ui.ctx().content_rect();
                    // Remember the last measured full content height so the
                    // next frame can allocate enough room to show it without
                    // making the user scroll. Scrolling is only needed when
                    // the card's contents exceed the available screen height.
                    let card_height_id =
                        egui::Id::unique(("editor-unified-card-content-height", &scroll_identity));
                    let measured_content_height = ui
                        .ctx()
                        .data(|data| data.get_temp::<f32>(card_height_id))
                        .unwrap_or(64.0);
                    let card_max_height = (screen.height() - 32.0).max(64.0);
                    let expected_card_height =
                        (measured_content_height + 20.0).min(card_max_height + 20.0);
                    let card_width = (screen.width() - 24.0).clamp(240.0, 500.0);
                    let below_space = (screen.bottom() - anchor.y - 26.0).max(0.0);
                    let above_space = (anchor.y - screen.top() - 26.0).max(0.0);
                    let position_y = if expected_card_height <= below_space {
                        anchor.y + 18.0
                    } else if expected_card_height <= above_space {
                        anchor.y - expected_card_height - 18.0
                    } else {
                        // Neither side of the pointer has enough room. Put a
                        // tall card at the top so it can use nearly the full
                        // screen instead of being clipped to a short strip.
                        screen.top() + 8.0
                    };
                    let position = egui::pos2(
                        anchor.x.min(screen.right() - card_width - 8.0).max(screen.left() + 4.0),
                        position_y,
                    );
                    let shown = egui::Area::new(egui::Id::unique("cairn.editor-unified-hover-card"))
                        .order(egui::Order::Tooltip)
                        .fixed_pos(position)
                        .interactable(true)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style())
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_max_width(card_width);
                                    let scroll_output = egui::ScrollArea::vertical()
                                        .id_salt(("editor-unified-card-scroll", &scroll_identity))
                                        .max_width(card_width)
                                        .max_height(card_max_height)
                                        .min_scrolled_height(
                                            measured_content_height
                                                .max(64.0)
                                                .min(card_max_height),
                                        )
                                        .auto_shrink([true, true])
                                        .show(ui, |ui| {
                                            if let Some((active_parameter, info, _)) = signature.as_ref() {
                                                let labels = info.overloads.iter()
                                                    .map(|overload| overload.label.clone())
                                                    .collect::<Vec<_>>();
                                                let selection_id = egui::Id::unique("cairn.editor-signature-overload-selection");
                                                let selection = ui.ctx().data(|data| {
                                                    data.get_temp::<OverloadSelection>(selection_id)
                                                });
                                                let overload_index = selection.as_ref()
                                                    .filter(|selection| selection.labels == labels)
                                                    .map_or(info.active_signature, |selection| selection.selected)
                                                    .min(info.overloads.len().saturating_sub(1));
                                                let overload = info.overloads.get(overload_index);
                                                let label = overload.map_or(info.label.as_str(), |overload| overload.label.as_str());
                                                let parameters = overload.map_or(info.parameters.as_slice(), |overload| overload.parameters.as_slice());
                                                let documentation = overload
                                                    .and_then(|overload| overload.parameters.get(*active_parameter))
                                                    .and_then(|parameter| parameter.documentation.as_deref())
                                                    .or_else(|| overload.and_then(|overload| overload.documentation.as_deref()))
                                                    .or_else(|| info.parameters.get(*active_parameter)
                                                        .and_then(|parameter| parameter.documentation.as_deref()))
                                                    .or(info.documentation.as_deref());
                                                ui.horizontal(|ui| {
                                                    ui.label(RichText::new("ƒ").strong().color(theme::accent()));
                                                    ui.label(RichText::new("Signature").strong());
                                                    if info.overloads.len() > 1 {
                                                        ui.label(RichText::new(format!(
                                                            "{} of {} overloads",
                                                            overload_index + 1,
                                                            info.overloads.len()
                                                        )).weak());
                                                    }
                                                });
                                                ui.add_space(3.0);
                                                let range = signature_parameter_range(label, *active_parameter)
                                                    .or_else(|| parameters.get(*active_parameter)
                                                        .and_then(|parameter| parameter.label_range.clone()));
                                                if let Some(range) = range.filter(|range| range.end <= label.len()) {
                                                    let parameter_text = &label[range.clone()];
                                                    let start = range.start + parameter_text.len() - parameter_text.trim_start().len();
                                                    let end = start + parameter_text.trim().len();
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.label(RichText::new(&label[..start]).monospace());
                                                        ui.label(RichText::new(&label[start..end]).monospace().strong().color(theme::accent()));
                                                        ui.label(RichText::new(&label[end..]).monospace());
                                                    });
                                                } else {
                                                    ui.label(RichText::new(label).monospace().strong());
                                                }
                                                if let Some(documentation) = documentation {
                                                    ui.add_space(4.0);
                                                    ui.label(documentation);
                                                }
                                                if info.overloads.len() > 1 {
                                                    ui.add_space(4.0);
                                                    ui.label(RichText::new("Move the pointer over this card and use ↑ / ↓ to browse overloads").weak().size(11.0));
                                                }
                                            }
                                            if let Some(text) = symbol_text {
                                                if signature.is_some() || !hovered_diagnostics.is_empty() {
                                                    ui.separator();
                                                }
                                                ui.horizontal(|ui| {
                                                    ui.label(RichText::new("ⓘ").strong().color(theme::accent()));
                                                    ui.label(RichText::new("Symbol information").strong());
                                                });
                                                ui.add_space(3.0);
                                                ui.label(RichText::new(text).monospace().size(12.5));
                                            }
                                            for diagnostic in &hovered_diagnostics {
                                                ui.separator();
                                                ui.horizontal(|ui| {
                                                    let (badge, color) = match diagnostic.severity {
                                                        DiagnosticSeverity::Error => ("ERROR", diagnostic_color(diagnostic.severity)),
                                                        DiagnosticSeverity::Warning => ("WARNING", diagnostic_color(diagnostic.severity)),
                                                        DiagnosticSeverity::Information => ("INFO", diagnostic_color(diagnostic.severity)),
                                                        DiagnosticSeverity::Hint => ("HINT", diagnostic_color(diagnostic.severity)),
                                                    };
                                                    ui.label(RichText::new(badge).strong().color(color));
                                                    if let Some(source) = diagnostic.source.as_deref() {
                                                        ui.label(RichText::new(source).weak());
                                                    }
                                                });
                                                ui.label(&diagnostic.message);
                                            }
                                        });
                                    ui.ctx().data_mut(|data| {
                                        data.insert_temp(card_height_id, scroll_output.content_size.y)
                                    });
                                    if (scroll_output.content_size.y - measured_content_height).abs()
                                        > 1.0
                                    {
                                        ui.ctx().request_repaint();
                                    }
                                });
                        });
                    let pointer_over_rendered_card = pointer_position
                        .is_some_and(|position| shown.response.rect.contains(position));
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(unified_card_rect_id, shown.response.rect)
                    });
                    if let Some(state) = hover_state.as_mut()
                        && (pointer_over_rendered_card
                            || hovered_symbol.as_ref().is_some_and(|(path, line, token, _)| {
                                state.path == *path && state.line == *line && state.token_start == *token
                            }))
                    {
                        state.last_active = Instant::now();
                        ui.ctx().data_mut(|data| data.insert_temp(popup_id, state.clone()));
                    }
                } else {
                    ui.ctx().data_mut(|data| {
                        data.remove::<HoverPopupState>(popup_id);
                        data.remove::<egui::Rect>(unified_card_rect_id);
                        data.remove::<Vec<Diagnostic>>(diagnostic_cache_id);
                        data.remove::<(usize, SignatureHelpInfo, egui::Pos2)>(signature_cache_id);
                    });
                }
                if document.text != previous_text
                    && let Some(path) = fold_path.as_ref()
                {
                    folded_ranges = adjust_fold_ranges_after_edit(
                        &previous_text,
                        &document.text,
                        &folded_ranges,
                    );
                    let valid_ranges = foldable_ranges(&document.text);
                    folded_ranges.retain(|fold| valid_ranges.contains(fold));
                    folds_by_path.insert(path.clone(), folded_ranges.clone());
                    ui.ctx().data_mut(|data| data.insert_temp(fold_state_id, folds_by_path));
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    document.text.hash(&mut hasher);
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(foldable_cache_id, (hasher.finish(), valid_ranges))
                    });
                }
                let (_, mapping_removed_indices) =
                    source_text_without_reference_lens_rows(&editor_display_text);
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        editor_mapping_id,
                        ReferenceLensEditorMapping {
                            path: document.path.clone(),
                            source_text: document.text.clone(),
                            removed_display_indices: mapping_removed_indices,
                            display_to_source: editor_mapping_after_edit.clone(),
                        },
                    )
                });
                if editor_text_changed {
                    document.dirty = true;
                    refresh_symbols = true;
                }
            });
    } else {
        ui.centered_and_justified(|ui| {
            ui.label(
                "No open tabs. Open a project file or use the Project tree to add a new item.",
            );
        });
    }
    if let Some((path, line)) = ui.ctx().data_mut(|data| {
        let id = egui::Id::unique("cairn.ctrl-click-definition-target");
        let target = data.get_temp::<(std::path::PathBuf, usize)>(id);
        data.remove::<(std::path::PathBuf, usize)>(id);
        target
    }) {
        navigate_to_project_line(ui.ctx(), workspace, &path, line);
    }
    if let Some(message) = ui.ctx().data_mut(|data| {
        let id = egui::Id::unique("cairn.ctrl-click-definition-message");
        let message = data.get_temp::<String>(id);
        data.remove::<String>(id);
        message
    }) {
        workspace.message = message;
    }
    if breakpoints_changed {
        if let Err(error) = workspace.persist_breakpoints() {
            workspace.message = error;
        }
        let breakpoints = workspace.all_breakpoints();
        if let Err(error) = process.sync_breakpoints(&breakpoints) {
            workspace.message = format!("Could not update debugger breakpoints: {error}");
        }
    }
    // Re-indexing reads every project file, so do it at most ~3 times a second.
    let pending_id = egui::Id::unique("symbol-refresh-pending");
    let stamp_id = egui::Id::unique("symbol-refresh-stamp");
    let last_edit_id = egui::Id::unique("symbol-refresh-last-edit");
    if refresh_symbols {
        let now = ui.input(|input| input.time);
        ui.ctx().data_mut(|data| {
            data.insert_temp(pending_id, true);
            data.insert_temp(last_edit_id, now);
        });
    }
    if ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(pending_id))
        .unwrap_or(false)
    {
        let now = ui.input(|input| input.time);
        let last = ui
            .ctx()
            .data_mut(|data| data.get_temp::<f64>(stamp_id))
            .unwrap_or(0.0);
        let last_edit = ui
            .ctx()
            .data_mut(|data| data.get_temp::<f64>(last_edit_id))
            .unwrap_or(last);
        if now - last_edit > 0.75 && now - last > 0.35 {
            ui.ctx().data_mut(|data| {
                data.insert_temp(pending_id, false);
                data.insert_temp(stamp_id, now);
            });
            if let Err(error) = workspace.refresh_active_document_symbols() {
                workspace.message = error;
            }
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}

fn navigate_to_project_line(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    relative_path: &std::path::Path,
    line: usize,
) {
    navigate_to_project_location(ctx, workspace, relative_path, line, 1);
}

fn navigate_to_project_location(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    relative_path: &std::path::Path,
    line: usize,
    column: usize,
) {
    match workspace.open_file(relative_path) {
        Ok(()) => {
            let id = egui::Id::unique("cairn.find-in-files-state");
            let mut state = ctx
                .data(|data| data.get_temp::<SearchPanelState>(id))
                .unwrap_or_default();
            state.pending_line = Some((relative_path.to_path_buf(), line.max(1)));
            state.pending_column = Some(column.max(1));
            ctx.data_mut(|data| data.insert_temp(id, state));
        }
        Err(error) => workspace.message = error,
    }
}

fn show_search_panel(ui: &mut egui::Ui) {
    ui.label(
        RichText::new("SEARCH IN PROJECT")
            .small()
            .strong()
            .color(theme::muted()),
    );
    let id = egui::Id::unique("cairn.find-in-files-state");
    let mut state = ui
        .ctx()
        .data(|data| data.get_temp::<SearchPanelState>(id))
        .unwrap_or_default();
    ui.label("Search project files and open a result at its line.");
    if !state.results.is_empty() {
        ui.label(format!(
            "{} matches in the latest search",
            state.results.len()
        ));
    }
    if ui.button("Open Find in Files…").clicked() {
        state.open = true;
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, state));
}

fn draw_find_in_files(ui: &mut egui::Ui, workspace: &mut Workspace) {
    let ctx = ui.ctx().clone();
    let state_id = egui::Id::unique("cairn.find-in-files-state");
    let mut state = ctx
        .data(|data| data.get_temp::<SearchPanelState>(state_id))
        .unwrap_or_default();
    if !state.open {
        return;
    }

    let mut selected = None;
    egui::Window::new("Find in Files")
        .open(&mut state.open)
        .default_width(680.0)
        .show(&ctx, |ui| {
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.query)
                        .hint_text("Search current project")
                        .desired_width(440.0),
                );
                let search_clicked = ui.button("Search").clicked()
                    || (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                if search_clicked {
                    state.results = search_project_files(workspace, &state.query);
                    state.message = Some(format!("{} matches", state.results.len()));
                }
            });
            if let Some(message) = &state.message {
                ui.label(RichText::new(message).small().color(theme::muted()));
            }
            egui::ScrollArea::vertical()
                .id_salt("find-files-results")
                .max_height(360.0)
                .show(ui, |ui| {
                    for result in &state.results {
                        let label = format!(
                            "{}:{}  {}",
                            result.path.display(),
                            result.line,
                            result.preview
                        );
                        if ui.selectable_label(false, label).clicked() {
                            selected = Some((result.path.clone(), result.line));
                        }
                    }
                });
        });

    if let Some((path, line)) = selected {
        match workspace.open_file(&path) {
            Ok(()) => {
                state.pending_line = Some((path, line));
                state.pending_column = None;
            }
            Err(error) => workspace.message = error,
        }
    }
    ctx.data_mut(|data| data.insert_temp(state_id, state));
}

fn search_project_files(workspace: &Workspace, query: &str) -> Vec<ProjectSearchResult> {
    let Some(project) = workspace.current.as_ref() else {
        return Vec::new();
    };
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    for relative_path in &workspace.files {
        let full_path = project.path.join(relative_path);
        let contents = workspace
            .documents
            .iter()
            .find(|document| document.path.as_deref() == Some(full_path.as_path()))
            .map(|document| document.text.clone())
            .or_else(|| {
                std::fs::metadata(&full_path)
                    .ok()
                    .filter(|metadata| metadata.len() <= 2 * 1024 * 1024)
                    .and_then(|_| std::fs::read_to_string(&full_path).ok())
            });
        let Some(contents) = contents else {
            continue;
        };
        for (line_index, line) in contents.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                let preview = line.trim().chars().take(180).collect::<String>();
                results.push(ProjectSearchResult {
                    path: relative_path.clone(),
                    line: line_index + 1,
                    preview,
                });
                if results.len() >= 500 {
                    return results;
                }
            }
        }
    }
    results
}

#[derive(Clone, Default)]
struct CompletionUiState {
    open: bool,
    armed: bool,
    selected: usize,
    cursor_position: Option<usize>,
    word: String,
    dismissed_word: Option<String>,
    member_access: bool,
    last_requested_word: Option<String>,
}

fn parse_completion_items(response: &serde_json::Value) -> Vec<(String, String)> {
    let result = response.get("result");
    let items = result
        .and_then(|value| value.get("items"))
        .or_else(|| result.filter(|value| value.is_array()))
        .and_then(serde_json::Value::as_array);
    items
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let label = item.get("label")?.as_str()?.to_owned();
            let insert = item
                .get("textEdit")
                .and_then(|edit| edit.get("newText"))
                .and_then(serde_json::Value::as_str)
                .or_else(|| item.get("insertText").and_then(serde_json::Value::as_str))
                .unwrap_or(&label)
                .to_owned();
            let detail = item
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    item.pointer("/documentation/value")
                        .and_then(serde_json::Value::as_str)
                })
                .or_else(|| {
                    item.get("documentation")
                        .and_then(serde_json::Value::as_str)
                })
                .unwrap_or_default()
                .to_owned();
            Some((insert, if detail.is_empty() { label } else { detail }))
        })
        .take(80)
        .collect()
}

#[derive(Clone, Debug)]
struct ProjectSearchResult {
    path: std::path::PathBuf,
    line: usize,
    preview: String,
}

#[derive(Clone, Default)]
struct SearchPanelState {
    open: bool,
    query: String,
    results: Vec<ProjectSearchResult>,
    pending_line: Option<(std::path::PathBuf, usize)>,
    pending_column: Option<usize>,
    message: Option<String>,
}

fn draw_quick_open_file(ui: &mut egui::Ui, workspace: &mut Workspace) {
    let open_id = egui::Id::unique("cairn.quick-open-file");
    let mut open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(open_id))
        .unwrap_or(false);
    if !open {
        return;
    }
    let query_id = egui::Id::unique("cairn.quick-open-file-query");
    let mut query = ui
        .ctx()
        .data(|data| data.get_temp::<String>(query_id))
        .unwrap_or_default();
    let focus_id = egui::Id::unique("cairn.quick-open-file-focus");
    let focus = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(focus_id).unwrap_or(false));
    let mut selected = None;
    egui::Window::new("Quick Open File")
        .id(egui::Id::unique("cairn.quick-open-file-window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .desired_width(420.0)
                    .hint_text("Type a project file name…"),
            );
            if focus {
                response.request_focus();
            }
            let folded = query.to_ascii_lowercase();
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for path in &workspace.files {
                        let display = path.to_string_lossy().replace('\\', "/");
                        if folded.is_empty() || display.to_ascii_lowercase().contains(&folded) {
                            if ui.selectable_label(false, display).clicked() {
                                selected = Some(path.clone());
                            }
                        }
                    }
                });
        });
    ui.ctx().data_mut(|data| {
        data.insert_temp(open_id, open);
        data.insert_temp(query_id, query);
        data.insert_temp(focus_id, false);
    });
    if let Some(path) = selected {
        if let Err(error) = workspace.open_file(&path) {
            workspace.message = error;
        }
        ui.ctx().data_mut(|data| data.insert_temp(open_id, false));
    }
}

/// Operator in front of the word being typed: `.`, `->` or `::`.
fn member_operator(text: &str, word_start: usize) -> Option<&'static str> {
    let before = text.chars().take(word_start).collect::<Vec<_>>();
    match before.as_slice() {
        [.., '-', '>'] => Some("->"),
        [.., ':', ':'] => Some("::"),
        [.., '.'] => Some("."),
        _ => None,
    }
}

fn is_member_completion(text: &str, word_start: usize) -> bool {
    member_operator(text, word_start).is_some()
}

/// The identifier written before the member operator (`Console` in `Console.Wri`).
fn member_receiver(text: &str, word_start: usize) -> Option<String> {
    let operator = member_operator(text, word_start)?;
    let before = text.chars().take(word_start).collect::<Vec<_>>();
    let end = before.len() - operator.chars().count();
    let start = before[..end]
        .iter()
        .rposition(|character| !character.is_alphanumeric() && *character != '_')
        .map_or(0, |index| index + 1);
    (start < end).then(|| before[start..end].iter().collect())
}

/// Finds `Type name` / `var name = new Type(...)` declarations for a variable.
fn infer_variable_type(text: &str, name: &str) -> Option<String> {
    const NOT_TYPES: [&str; 9] = [
        "return", "new", "in", "is", "as", "out", "ref", "else", "case",
    ];
    for line in text.lines() {
        let line = line.trim();
        for (position, _) in line.match_indices(name) {
            let after = line[position + name.len()..].trim_start();
            let before = line[..position].trim_end();
            let boundary_before = line[..position]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
            if !boundary_before || !after.starts_with(['=', ';', ',', ')', ':']) {
                continue;
            }
            let token = before
                .rsplit(|character: char| character.is_whitespace() || character == '(')
                .next()
                .unwrap_or_default();
            if token.is_empty()
                || NOT_TYPES.contains(&token)
                || !token.starts_with(|character: char| character.is_alphabetic())
            {
                continue;
            }
            if token == "var" || token == "auto" {
                let value = after.strip_prefix('=').unwrap_or_default().trim_start();
                if let Some(constructed) = value.strip_prefix("new ") {
                    let ty = constructed
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '<' | '>' | ','))
                        .collect::<String>();
                    if !ty.is_empty() {
                        return Some(ty);
                    }
                } else if value.starts_with('"') {
                    return Some("string".to_owned());
                }
                continue;
            }
            return Some(token.trim_end_matches('?').to_owned());
        }
    }
    None
}

const CS_STRING: &[(&str, &str)] = &[
    ("Length", "int"),
    ("Contains", "bool"),
    ("StartsWith", "bool"),
    ("EndsWith", "bool"),
    ("IndexOf", "int"),
    ("LastIndexOf", "int"),
    ("Substring", "string"),
    ("Replace", "string"),
    ("Split", "string[]"),
    ("Trim", "string"),
    ("TrimStart", "string"),
    ("TrimEnd", "string"),
    ("ToUpper", "string"),
    ("ToLower", "string"),
    ("PadLeft", "string"),
    ("PadRight", "string"),
    ("Insert", "string"),
    ("Remove", "string"),
    ("Equals", "bool"),
    ("ToString", "string"),
    ("CompareTo", "int"),
    ("ToCharArray", "char[]"),
    ("IsNullOrEmpty", "static bool"),
    ("IsNullOrWhiteSpace", "static bool"),
    ("Join", "static string"),
    ("Format", "static string"),
    ("Concat", "static string"),
];
const CS_LIST: &[(&str, &str)] = &[
    ("Add", "void"),
    ("AddRange", "void"),
    ("Remove", "bool"),
    ("RemoveAt", "void"),
    ("Clear", "void"),
    ("Contains", "bool"),
    ("Count", "int"),
    ("Length", "int"),
    ("IndexOf", "int"),
    ("Insert", "void"),
    ("Sort", "void"),
    ("Reverse", "void"),
    ("ToArray", "T[]"),
    ("Find", "T"),
    ("FindAll", "List<T>"),
    ("Exists", "bool"),
    ("ForEach", "void"),
    ("First", "LINQ"),
    ("FirstOrDefault", "LINQ"),
    ("Select", "LINQ"),
    ("Where", "LINQ"),
    ("Any", "LINQ"),
    ("All", "LINQ"),
    ("ToList", "LINQ"),
    ("OrderBy", "LINQ"),
    ("Sum", "LINQ"),
    ("Max", "LINQ"),
    ("Min", "LINQ"),
];
const CS_DICTIONARY: &[(&str, &str)] = &[
    ("Add", "void"),
    ("Remove", "bool"),
    ("ContainsKey", "bool"),
    ("ContainsValue", "bool"),
    ("TryGetValue", "bool"),
    ("Keys", "collection"),
    ("Values", "collection"),
    ("Count", "int"),
    ("Clear", "void"),
];
const CS_CONSOLE: &[(&str, &str)] = &[
    ("WriteLine", "void"),
    ("Write", "void"),
    ("ReadLine", "string?"),
    ("ReadKey", "ConsoleKeyInfo"),
    ("Read", "int"),
    ("Clear", "void"),
    ("Beep", "void"),
    ("Title", "string"),
    ("ForegroundColor", "ConsoleColor"),
    ("BackgroundColor", "ConsoleColor"),
    ("ResetColor", "void"),
    ("SetCursorPosition", "void"),
    ("CursorVisible", "bool"),
    ("Out", "TextWriter"),
    ("In", "TextReader"),
    ("Error", "TextWriter"),
];
const CS_MATH: &[(&str, &str)] = &[
    ("Abs", "static"),
    ("Sqrt", "static"),
    ("Pow", "static"),
    ("Min", "static"),
    ("Max", "static"),
    ("Floor", "static"),
    ("Ceiling", "static"),
    ("Round", "static"),
    ("Truncate", "static"),
    ("Sin", "static"),
    ("Cos", "static"),
    ("Tan", "static"),
    ("Log", "static"),
    ("Log10", "static"),
    ("Exp", "static"),
    ("Sign", "static"),
    ("Clamp", "static"),
    ("PI", "const"),
    ("E", "const"),
];
const CS_FILE: &[(&str, &str)] = &[
    ("ReadAllText", "string"),
    ("WriteAllText", "void"),
    ("ReadAllLines", "string[]"),
    ("WriteAllLines", "void"),
    ("AppendAllText", "void"),
    ("ReadLines", "IEnumerable<string>"),
    ("Exists", "bool"),
    ("Delete", "void"),
    ("Copy", "void"),
    ("Move", "void"),
    ("OpenRead", "FileStream"),
    ("Create", "FileStream"),
];
const CS_DIRECTORY: &[(&str, &str)] = &[
    ("Exists", "bool"),
    ("CreateDirectory", "DirectoryInfo"),
    ("GetFiles", "string[]"),
    ("GetDirectories", "string[]"),
    ("EnumerateFiles", "IEnumerable<string>"),
    ("Delete", "void"),
    ("GetCurrentDirectory", "string"),
];
const CS_PATH: &[(&str, &str)] = &[
    ("Combine", "string"),
    ("GetFileName", "string"),
    ("GetExtension", "string"),
    ("GetDirectoryName", "string"),
    ("GetFileNameWithoutExtension", "string"),
    ("GetFullPath", "string"),
    ("GetTempPath", "string"),
    ("DirectorySeparatorChar", "char"),
];
const CS_DATETIME: &[(&str, &str)] = &[
    ("Now", "DateTime"),
    ("UtcNow", "DateTime"),
    ("Today", "DateTime"),
    ("Year", "int"),
    ("Month", "int"),
    ("Day", "int"),
    ("Hour", "int"),
    ("Minute", "int"),
    ("Second", "int"),
    ("DayOfWeek", "DayOfWeek"),
    ("AddDays", "DateTime"),
    ("AddHours", "DateTime"),
    ("AddMinutes", "DateTime"),
    ("ToString", "string"),
    ("Parse", "static"),
    ("TryParse", "static"),
];
const CS_ENVIRONMENT: &[(&str, &str)] = &[
    ("NewLine", "string"),
    ("GetEnvironmentVariable", "string?"),
    ("Exit", "void"),
    ("MachineName", "string"),
    ("UserName", "string"),
    ("CurrentDirectory", "string"),
    ("GetCommandLineArgs", "string[]"),
    ("OSVersion", "OperatingSystem"),
    ("ProcessorCount", "int"),
];
const CS_CONVERT: &[(&str, &str)] = &[
    ("ToInt32", "int"),
    ("ToInt64", "long"),
    ("ToDouble", "double"),
    ("ToBoolean", "bool"),
    ("ToString", "string"),
    ("ToChar", "char"),
    ("ToBase64String", "string"),
    ("FromBase64String", "byte[]"),
];
const CS_TASK: &[(&str, &str)] = &[
    ("Run", "Task"),
    ("Delay", "Task"),
    ("WhenAll", "Task"),
    ("WhenAny", "Task"),
    ("FromResult", "Task<T>"),
    ("CompletedTask", "Task"),
    ("Wait", "void"),
    ("Result", "T"),
];
const CS_STRING_BUILDER: &[(&str, &str)] = &[
    ("Append", "StringBuilder"),
    ("AppendLine", "StringBuilder"),
    ("Insert", "StringBuilder"),
    ("Remove", "StringBuilder"),
    ("Replace", "StringBuilder"),
    ("Clear", "StringBuilder"),
    ("Length", "int"),
    ("ToString", "string"),
];
const CS_NUMBER: &[(&str, &str)] = &[
    ("ToString", "string"),
    ("Parse", "static"),
    ("TryParse", "static"),
    ("CompareTo", "int"),
    ("Equals", "bool"),
    ("MaxValue", "const"),
    ("MinValue", "const"),
];
const CS_RANDOM: &[(&str, &str)] = &[
    ("Next", "int"),
    ("NextDouble", "double"),
    ("NextInt64", "long"),
    ("Shared", "Random"),
];
const CS_CONTROL: &[(&str, &str)] = &[
    ("Text", "control text"),
    ("Content", "button content"),
    ("IsEnabled", "bool"),
    ("IsVisible", "bool"),
    ("Width", "double"),
    ("Height", "double"),
    ("Foreground", "brush"),
    ("Background", "brush"),
    ("FontSize", "double"),
    ("Margin", "Thickness"),
    ("Tag", "object"),
    ("Watermark", "TextBox hint"),
    ("Focus", "void"),
    ("Name", "string"),
];
const CPP_STD: &[(&str, &str)] = &[
    ("cout", "stream"),
    ("cin", "stream"),
    ("cerr", "stream"),
    ("endl", "manipulator"),
    ("string", "type"),
    ("vector", "container"),
    ("map", "container"),
    ("unordered_map", "container"),
    ("set", "container"),
    ("array", "container"),
    ("optional", "type"),
    ("unique_ptr", "smart pointer"),
    ("shared_ptr", "smart pointer"),
    ("make_unique", "function"),
    ("make_shared", "function"),
    ("move", "function"),
    ("sort", "algorithm"),
    ("find", "algorithm"),
    ("min", "algorithm"),
    ("max", "algorithm"),
    ("to_string", "function"),
    ("getline", "function"),
    ("size_t", "type"),
    ("thread", "type"),
    ("mutex", "type"),
    ("chrono", "namespace"),
    ("filesystem", "namespace"),
];
const CPP_CONTAINER: &[(&str, &str)] = &[
    ("at", "access"),
    ("begin", "iterator"),
    ("end", "iterator"),
    ("back", "access"),
    ("front", "access"),
    ("clear", "modifier"),
    ("empty", "bool"),
    ("size", "size_t"),
    ("push_back", "modifier"),
    ("pop_back", "modifier"),
    ("insert", "modifier"),
    ("erase", "modifier"),
    ("reserve", "capacity"),
    ("resize", "capacity"),
    ("data", "pointer"),
    ("c_str", "string"),
    ("length", "string"),
    ("substr", "string"),
    ("find", "string"),
    ("append", "string"),
    ("compare", "string"),
];
const QT_WIDGET: &[(&str, &str)] = &[
    ("setText", "QString"),
    ("text", "QString"),
    ("clear", "void"),
    ("setEnabled", "bool"),
    ("isEnabled", "bool"),
    ("setVisible", "bool"),
    ("show", "void"),
    ("hide", "void"),
    ("setGeometry", "x, y, w, h"),
    ("move", "x, y"),
    ("resize", "w, h"),
    ("setStyleSheet", "QString"),
    ("setPlaceholderText", "QLineEdit"),
    ("setToolTip", "QString"),
    ("setFocus", "void"),
    ("setFont", "QFont"),
    ("close", "bool"),
    ("update", "void"),
    ("setReadOnly", "QLineEdit"),
    ("setMaxLength", "QLineEdit"),
    ("setChecked", "QPushButton"),
];

fn members_for_type(language: &str, ty: &str) -> Option<&'static [(&'static str, &'static str)]> {
    let base = ty.split(['<', '[']).next().unwrap_or(ty).trim();
    if ty.ends_with("[]") {
        return Some(CS_LIST);
    }
    match (language, base) {
        ("cs", "string" | "String") => Some(CS_STRING),
        (
            "cs",
            "List" | "IEnumerable" | "ICollection" | "HashSet" | "Queue" | "Stack" | "IList",
        ) => Some(CS_LIST),
        ("cs", "Dictionary" | "IDictionary") => Some(CS_DICTIONARY),
        ("cs", "Console") => Some(CS_CONSOLE),
        ("cs", "Math" | "MathF") => Some(CS_MATH),
        ("cs", "File") => Some(CS_FILE),
        ("cs", "Directory") => Some(CS_DIRECTORY),
        ("cs", "Path") => Some(CS_PATH),
        ("cs", "DateTime" | "DateTimeOffset") => Some(CS_DATETIME),
        ("cs", "Environment") => Some(CS_ENVIRONMENT),
        ("cs", "Convert") => Some(CS_CONVERT),
        ("cs", "Task") => Some(CS_TASK),
        ("cs", "StringBuilder") => Some(CS_STRING_BUILDER),
        (
            "cs",
            "int" | "long" | "double" | "float" | "decimal" | "byte" | "short" | "bool" | "char"
            | "Int32" | "Int64" | "Double",
        ) => Some(CS_NUMBER),
        ("cs", "Random") => Some(CS_RANDOM),
        ("cs", "TextBox" | "TextBlock" | "Button" | "Label" | "Window" | "CheckBox") => {
            Some(CS_CONTROL)
        }
        ("cpp", "std") => Some(CPP_STD),
        ("cpp", "string" | "vector" | "map" | "array") => Some(CPP_CONTAINER),
        ("cpp", "QPushButton" | "QLabel" | "QLineEdit" | "QWidget") => Some(QT_WIDGET),
        _ => None,
    }
}

/// Members for the receiver in front of the cursor, when its type is known.
fn member_suggestions(
    language: &str,
    text: &str,
    word_start: usize,
    word: &str,
) -> Option<Vec<egui::Suggestion>> {
    let receiver = member_receiver(text, word_start)?;
    let operator = member_operator(text, word_start)?;
    let table = if operator == "::" && receiver == "std" {
        Some(CPP_STD)
    } else if operator == "->" && language == "cpp" {
        Some(QT_WIDGET)
    } else {
        members_for_type(language, &receiver).or_else(|| {
            infer_variable_type(text, &receiver).and_then(|ty| members_for_type(language, &ty))
        })
    };
    let table = table?;
    let word = word.to_ascii_lowercase();
    let mut matches = table
        .iter()
        .filter(|(name, _)| name.to_ascii_lowercase().starts_with(&word))
        .collect::<Vec<_>>();
    matches.sort_by_key(|(name, _)| name.to_ascii_lowercase());
    Some(
        matches
            .into_iter()
            .map(|(name, detail)| egui::Suggestion::new(*name).description(*detail))
            .collect(),
    )
}

fn lsp_position(text: &str, cursor: usize) -> (usize, usize) {
    let prefix = text.chars().take(cursor).collect::<String>();
    let line = prefix.matches('\n').count();
    let character = prefix
        .rsplit_once('\n')
        .map_or(prefix.as_str(), |(_, current)| current)
        .encode_utf16()
        .count();
    (line, character)
}

fn completion_query(text: &str, cursor: usize) -> Option<(std::ops::Range<usize>, String)> {
    let chars = text.chars().collect::<Vec<_>>();
    if cursor > chars.len() {
        return None;
    }
    let start = chars[..cursor]
        .iter()
        .rposition(|character| !character.is_alphanumeric() && *character != '_')
        .map_or(0, |index| index + 1);
    Some((start..cursor, chars[start..cursor].iter().collect()))
}

fn identifier_range(line: &str, cursor: usize) -> Option<std::ops::Range<usize>> {
    let chars = line.chars().collect::<Vec<_>>();
    let character = *chars.get(cursor)?;
    if !character.is_alphanumeric() && character != '_' {
        return None;
    }
    let start = chars[..=cursor]
        .iter()
        .rposition(|character| !character.is_alphanumeric() && *character != '_')
        .map_or(0, |index| index + 1);
    let end = chars[cursor + 1..]
        .iter()
        .position(|character| !character.is_alphanumeric() && *character != '_')
        .map_or(chars.len(), |offset| cursor + 1 + offset);
    Some(start..end)
}

fn signature_help_info(response: &serde_json::Value) -> Option<SignatureHelpInfo> {
    let result = response.get("result")?;
    let signatures = result.get("signatures")?.as_array()?;
    if signatures.is_empty() {
        return None;
    }
    let requested_active = result
        .get("activeSignature")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let overloads = signatures
        .iter()
        .filter_map(parse_signature_overload)
        .collect::<Vec<_>>();
    let active_signature = requested_active.min(overloads.len().saturating_sub(1));
    let active = overloads.get(active_signature)?.clone();
    Some(SignatureHelpInfo {
        label: active.label,
        documentation: active.documentation,
        parameters: active.parameters,
        overloads,
        active_signature,
    })
}

fn parse_signature_overload(signature: &serde_json::Value) -> Option<SignatureOverload> {
    let label = signature.get("label")?.as_str()?.to_owned();
    let documentation = signature.get("documentation").and_then(lsp_documentation);
    let parameters = signature
        .get("parameters")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, parameter)| {
            let label_value = parameter.get("label")?;
            let (_, label_range) = if let Some(text) = label_value.as_str() {
                let open = label.find('(').map_or(0, |position| position + 1);
                let range = label[open..]
                    .find(text)
                    .map(|offset| open + offset..open + offset + text.len());
                (text.to_owned(), range)
            } else if let Some(offsets) = label_value.as_array() {
                let start = offsets.first()?.as_u64()? as usize;
                let end = offsets.get(1)?.as_u64()? as usize;
                let start_byte = utf16_offset_to_byte(&label, start);
                let end_byte = utf16_offset_to_byte(&label, end);
                (
                    label.get(start_byte..end_byte)?.to_owned(),
                    Some(start_byte..end_byte),
                )
            } else {
                return None;
            };
            let fallback_range = signature_parameter_range(&label, index);
            Some(SignatureParameter {
                documentation: parameter.get("documentation").and_then(lsp_documentation),
                label_range: label_range.or(fallback_range),
            })
        })
        .collect::<Vec<_>>();
    Some(SignatureOverload {
        label,
        documentation,
        parameters,
    })
}

fn lsp_documentation(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_owned());
    }
    crate::language_server::hover_contents(&serde_json::json!({
        "result": {"contents": value}
    }))
}

fn utf16_offset_to_byte(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += character.len_utf16();
    }
    text.len()
}

fn signature_parameter_range(
    label: &str,
    parameter_index: usize,
) -> Option<std::ops::Range<usize>> {
    let open = label.find('(')?;
    let mut depth = 0usize;
    let mut start = open + 1;
    let mut index = 0usize;
    for (offset, character) in label[open + 1..].char_indices() {
        let position = open + 1 + offset;
        match character {
            '(' | '[' | '<' | '{' => depth += 1,
            ')' if depth == 0 => {
                return (index == parameter_index).then_some(start..position);
            }
            ')' | ']' | '>' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if index == parameter_index {
                    return Some(start..position);
                }
                index += 1;
                start = position + 1;
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
fn call_argument_context(text: &str, cursor: usize) -> Option<usize> {
    call_signature_context(text, cursor).map(|(_, argument)| argument)
}

fn call_signature_context(text: &str, cursor: usize) -> Option<(String, usize)> {
    #[derive(Clone)]
    struct CallFrame {
        comma_count: usize,
        callee: Option<String>,
        brace_depth: usize,
        bracket_depth: usize,
    }

    let chars = text.chars().take(cursor).collect::<Vec<_>>();
    let mut calls = Vec::<CallFrame>::new();
    let mut braces = 0usize;
    let mut brackets = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in chars.iter().copied().enumerate() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => quote = Some(character),
            '(' => {
                let callee = chars[..index]
                    .iter()
                    .rev()
                    .skip_while(|character| character.is_whitespace())
                    .take_while(|character| character.is_alphanumeric() || **character == '_')
                    .copied()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<String>();
                calls.push(CallFrame {
                    comma_count: 0,
                    callee: (!callee.is_empty()).then_some(callee),
                    brace_depth: braces,
                    bracket_depth: brackets,
                });
            }
            ')' => {
                calls.pop();
            }
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            '[' => brackets += 1,
            ']' => brackets = brackets.saturating_sub(1),
            ',' => {
                if let Some(call) = calls.last_mut()
                    && braces == call.brace_depth
                    && brackets == call.bracket_depth
                {
                    call.comma_count += 1;
                }
            }
            _ => {}
        }
    }
    calls
        .last()
        .and_then(|call| call.callee.clone().map(|callee| (callee, call.comma_count)))
}

fn local_signature_help(
    symbols: &[crate::project::ProjectSymbol],
    project_root: Option<&std::path::Path>,
    active_path: Option<&std::path::Path>,
    active_text: &str,
    callee: &str,
) -> Option<SignatureHelpInfo> {
    let mut overloads = Vec::new();
    for symbol in symbols.iter().filter(|symbol| {
        symbol.name == callee && symbol.kind.to_ascii_lowercase().contains("function")
    }) {
        if let Some(overload) =
            local_signature_overload(symbol, project_root, active_path, active_text, callee)
            && !overloads
                .iter()
                .any(|existing: &SignatureOverload| existing.label == overload.label)
        {
            overloads.push(overload);
        }
    }
    let active = overloads.first()?.clone();
    Some(SignatureHelpInfo {
        label: active.label.clone(),
        documentation: active.documentation.clone(),
        parameters: active.parameters.clone(),
        overloads,
        active_signature: 0,
    })
}

fn local_signature_overload(
    symbol: &crate::project::ProjectSymbol,
    project_root: Option<&std::path::Path>,
    active_path: Option<&std::path::Path>,
    active_text: &str,
    callee: &str,
) -> Option<SignatureOverload> {
    let path = if symbol.path.is_absolute() {
        symbol.path.clone()
    } else {
        project_root?.join(&symbol.path)
    };
    let text = if active_path.is_some_and(|active_path| active_path == path) {
        active_text.to_owned()
    } else {
        std::fs::read_to_string(&path).ok()?
    };
    let lines = text.lines().collect::<Vec<_>>();
    let start = symbol.line.saturating_sub(1);
    let declaration = lines
        .iter()
        .skip(start)
        .take(12)
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let callee_start = declaration.find(callee)?;
    let open = declaration[callee_start + callee.len()..]
        .find('(')
        .map(|offset| callee_start + callee.len() + offset)?;
    let mut depth = 0usize;
    let mut close = None;
    for (offset, character) in declaration[open..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let label = declaration[..=close].to_owned();
    let mut parameters = Vec::new();
    for index in 0.. {
        let Some(range) = signature_parameter_range(&label, index) else {
            break;
        };
        let start = label[range.clone()]
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(range.end, |(offset, _)| range.start + offset);
        let end = label[range.clone()]
            .char_indices()
            .rev()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(start, |(offset, character)| {
                range.start + offset + character.len_utf8()
            });
        if start >= end {
            break;
        }
        parameters.push(SignatureParameter {
            documentation: None,
            label_range: Some(start..end),
        });
    }
    Some(SignatureOverload {
        label,
        documentation: None,
        parameters,
    })
}

fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(byte_index, _)| byte_index)
}

fn auto_pair_edit(previous_text: &str, text: &mut String, cursor: usize) -> Option<usize> {
    let before = previous_text.chars().collect::<Vec<_>>();
    let after = text.chars().collect::<Vec<_>>();
    if after.len() != before.len() + 1 {
        return None;
    }
    let inserted_at = cursor.checked_sub(1)?;
    if inserted_at >= after.len()
        || after
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != inserted_at)
            .map(|(_, ch)| ch)
            .ne(before.iter())
    {
        return None;
    }

    let typed = after[inserted_at];
    let byte_index = text
        .char_indices()
        .nth(inserted_at)
        .map_or(text.len(), |(index, _)| index);
    if matches!(typed, ')' | ']' | '}' | '"' | '\'') && before.get(inserted_at) == Some(&typed) {
        text.replace_range(byte_index..byte_index + typed.len_utf8(), "");
        return Some(cursor);
    }
    let closing = match typed {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '"' => '"',
        '\'' => '\'',
        _ => return None,
    };
    let insert_byte_index = text
        .char_indices()
        .nth(cursor)
        .map_or(text.len(), |(index, _)| index);
    text.insert(insert_byte_index, closing);
    Some(cursor)
}

fn auto_indent_edit(
    previous_text: &str,
    text: &mut String,
    cursor: usize,
    tab_width: u8,
    use_tabs: bool,
) -> Option<usize> {
    let before = previous_text.chars().collect::<Vec<_>>();
    let after = text.chars().collect::<Vec<_>>();
    let inserted_at = cursor.checked_sub(1)?;
    if after.len() != before.len() + 1
        || inserted_at >= after.len()
        || after[inserted_at] != '\n'
        || after
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != inserted_at)
            .map(|(_, character)| character)
            .ne(before.iter())
    {
        return None;
    }

    let line_start = before[..inserted_at]
        .iter()
        .rposition(|character| *character == '\n')
        .map_or(0, |index| index + 1);
    let line_prefix = &before[line_start..inserted_at];
    let base_indent = line_prefix
        .iter()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .copied()
        .collect::<String>();
    let opens_block = line_prefix
        .iter()
        .copied()
        .collect::<String>()
        .trim_end()
        .ends_with('{');
    let next_char = after.get(inserted_at + 1).copied();
    let indent_unit = if use_tabs || base_indent.contains('\t') {
        "\t"
    } else {
        return auto_indent_with_spaces(
            text,
            cursor,
            &base_indent,
            opens_block,
            next_char,
            tab_width,
        );
    };
    let indentation = if opens_block {
        format!("{base_indent}{indent_unit}")
    } else {
        base_indent.clone()
    };
    let insertion = if opens_block && next_char == Some('}') {
        format!("{indentation}\n{base_indent}")
    } else {
        indentation.clone()
    };
    let insert_byte_index = char_to_byte_index(text, cursor);
    text.insert_str(insert_byte_index, &insertion);
    Some(cursor + indentation.chars().count())
}

fn auto_indent_with_spaces(
    text: &mut String,
    cursor: usize,
    base_indent: &str,
    opens_block: bool,
    next_char: Option<char>,
    tab_width: u8,
) -> Option<usize> {
    let indentation = if opens_block {
        format!(
            "{base_indent}{}",
            " ".repeat(usize::from(tab_width.clamp(1, 8)))
        )
    } else {
        base_indent.to_owned()
    };
    let insertion = if opens_block && next_char == Some('}') {
        format!("{indentation}\n{base_indent}")
    } else {
        indentation.clone()
    };
    let insert_byte_index = char_to_byte_index(text, cursor);
    text.insert_str(insert_byte_index, &insertion);
    Some(cursor + indentation.chars().count())
}

fn completion_suggestions(
    language: &str,
    word: &str,
    project_symbols: &[crate::project::ProjectSymbol],
    member_access: bool,
) -> Vec<egui::Suggestion> {
    if word.chars().count() < 2 && !member_access {
        return Vec::new();
    }
    if !member_access && language == "cs" && word.eq_ignore_ascii_case("cw") {
        return vec![egui::Suggestion::new("Console.WriteLine").description("C# output")];
    }
    let member_candidates: &[(&str, &str)] = match language {
        "cs" => &[
            ("Equals", "object member"),
            ("GetHashCode", "object member"),
            ("GetType", "object member"),
            ("ToString", "object member"),
            ("Contains", "string/collection member"),
            ("EndsWith", "string member"),
            ("IndexOf", "string/collection member"),
            ("Length", "string member"),
            ("Replace", "string member"),
            ("Split", "string member"),
            ("StartsWith", "string member"),
            ("Substring", "string member"),
            ("Write", "Console member"),
            ("WriteLine", "Console member"),
            ("Text", "control property"),
            ("Content", "control property"),
            ("IsEnabled", "control property"),
            ("IsVisible", "control property"),
        ],
        "cpp" => &[
            ("at", "string/container member"),
            ("begin", "container member"),
            ("c_str", "string member"),
            ("clear", "string/container member"),
            ("empty", "string/container member"),
            ("end", "container member"),
            ("find", "string/container member"),
            ("length", "string member"),
            ("pop_back", "container member"),
            ("push_back", "container member"),
            ("reserve", "string/container member"),
            ("size", "string/container member"),
            ("substr", "string member"),
            ("setText", "Qt widget"),
            ("text", "Qt widget"),
            ("setEnabled", "Qt widget"),
            ("setVisible", "Qt widget"),
        ],
        _ => &[],
    };
    let candidates: &[(&str, &str)] = match language {
        "cs" => &[
            ("async", "keyword"),
            ("await", "keyword"),
            ("bool", "type"),
            ("class", "keyword"),
            ("Console", ".NET API"),
            ("Console.WriteLine", ".NET API"),
            ("if", "keyword"),
            ("int", "type"),
            ("namespace", "keyword"),
            ("public", "keyword"),
            ("return", "keyword"),
            ("string", "type"),
            ("using", "keyword"),
            ("var", "keyword"),
        ],
        "cpp" => &[
            ("auto", "keyword"),
            ("bool", "type"),
            ("class", "keyword"),
            ("const", "keyword"),
            ("for", "keyword"),
            ("if", "keyword"),
            ("#include <iostream>", "snippet"),
            ("int", "type"),
            ("main", "entry point"),
            ("namespace", "keyword"),
            ("std::cout", "iostream"),
            ("std::string", "string"),
            ("std::vector", "container"),
            ("void", "type"),
        ],
        "xml" => &[
            ("Button", "Avalonia control"),
            ("Canvas", "Avalonia panel"),
            ("Canvas.Left", "layout property"),
            ("Canvas.Top", "layout property"),
            ("Click", "button event"),
            ("TextBlock", "Avalonia control"),
            ("TextBox", "Avalonia control"),
            ("Window", "Avalonia control"),
            ("x:Name", "named control"),
        ],
        _ => return Vec::new(),
    };
    let word = word.to_ascii_lowercase();
    let mut suggestions = Vec::new();
    for symbol in project_symbols {
        if (member_access && symbol.kind != "C# function" && symbol.kind != "C++ function")
            || !symbol.name.to_ascii_lowercase().starts_with(&word)
            || suggestions
                .iter()
                .any(|suggestion: &egui::Suggestion| suggestion.insert == symbol.name)
        {
            continue;
        }
        suggestions.push(
            egui::Suggestion::new(symbol.name.clone()).description(format!(
                "{} · {}",
                symbol.kind,
                symbol.path.display()
            )),
        );
    }
    if member_access {
        for (candidate, description) in member_candidates
            .iter()
            .filter(|(candidate, _)| candidate.to_ascii_lowercase().starts_with(&word))
        {
            if !suggestions
                .iter()
                .any(|suggestion: &egui::Suggestion| suggestion.insert == *candidate)
            {
                suggestions.push(egui::Suggestion::new(*candidate).description(*description));
            }
        }
        return suggestions;
    }
    for (candidate, description) in candidates
        .iter()
        .filter(|(candidate, _)| candidate.to_ascii_lowercase().starts_with(&word))
    {
        if !suggestions
            .iter()
            .any(|suggestion: &egui::Suggestion| suggestion.insert.eq_ignore_ascii_case(candidate))
        {
            suggestions.push(egui::Suggestion::new(*candidate).description(*description));
        }
    }
    suggestions.truncate(12);
    suggestions
}

fn relative_include_path(from_file: &std::path::Path, header: &std::path::Path) -> Option<String> {
    let from_directory = from_file
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));
    let from_components = from_directory
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let header_components = header
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let shared = from_components
        .iter()
        .zip(&header_components)
        .take_while(|(left, right)| left == right)
        .count();
    let mut include = std::path::PathBuf::new();
    for _ in shared..from_components.len() {
        include.push("..");
    }
    for component in &header_components[shared..] {
        include.push(component);
    }
    Some(include.to_string_lossy().replace('\\', "/"))
}

fn declared_namespace(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let declaration = line.trim().strip_prefix("namespace ")?;
        let name = declaration
            .split([';', '{', ' '])
            .next()
            .unwrap_or_default()
            .trim();
        (!name.is_empty()).then(|| name.to_owned())
    })
}

fn language_for_document(document: &crate::project::Document) -> &'static str {
    let extension = document
        .path
        .as_deref()
        .and_then(std::path::Path::extension)
        .and_then(|extension| extension.to_str())
        .or_else(|| {
            document
                .title
                .rsplit_once('.')
                .map(|(_, extension)| extension)
        });
    match extension {
        Some("cpp" | "hpp" | "h" | "cc" | "cxx") => "cpp",
        Some("cs") => "cs",
        Some("axaml" | "xaml") => "xml",
        Some("rs") => "rs",
        Some("toml") => "toml",
        Some("json") => "json",
        _ => "txt",
    }
}

fn draw_project_wizard(
    ui: &mut egui::Ui,
    is_open: &AtomicBool,
    form: &Mutex<ProjectForm>,
    workspace: &Mutex<Workspace>,
) -> bool {
    let mut form = form
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut created = false;

    egui::Panel::bottom("wizard_footer")
        .exact_size(64.0)
        .frame(bar_frame(theme::bg_dark()))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let destination = std::path::PathBuf::from(form.location.trim());
                let create_enabled =
                    crate::project::validate_project_destination(&destination, form.name.trim())
                        .is_ok();
                let create = ui.add_enabled(
                    create_enabled,
                    egui::Button::new(RichText::new("Create").strong().color(Color32::WHITE))
                        .fill(theme::accent())
                        .min_size(egui::vec2(120.0, 34.0)),
                );
                if create.clicked() {
                    let location = std::path::PathBuf::from(form.location.trim());
                    let result = {
                        let mut workspace = workspace
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if form.csharp_scaffold {
                            workspace.create_project_with_csharp_scaffold(
                                &location,
                                form.name.trim(),
                                form.language,
                                form.template,
                                &form.framework,
                                true,
                            )
                        } else {
                            workspace.create_project(
                                &location,
                                form.name.trim(),
                                form.language,
                                form.template,
                                &form.framework,
                            )
                        }
                    };
                    match result {
                        Ok(_) => {
                            is_open.store(false, Ordering::Relaxed);
                            created = true;
                        }
                        Err(error) => form.error = Some(error),
                    }
                }
                if ui
                    .add(egui::Button::new("Back").min_size(egui::vec2(120.0, 34.0)))
                    .clicked()
                {
                    is_open.store(false, Ordering::Relaxed);
                }
            });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::bg())
                .inner_margin(egui::Margin::symmetric(48, 28)),
        )
        .show(ui, |ui| {
            ui.label(RichText::new("Create a new project").size(26.0));
            ui.add_space(18.0);
            ui.columns(2, |columns| {
                columns[0].label(RichText::new("Choose a template").size(18.0));
                columns[0].add_space(10.0);
                let templates = [
                    (
                        ProjectLanguage::Cpp,
                        ProjectTemplate::Console,
                        "C++",
                        "Console App",
                        "CMake-based console app. Requires CMake and a C++ compiler.",
                    ),
                    (
                        ProjectLanguage::Cpp,
                        ProjectTemplate::CMakeProject,
                        "CMake",
                        "CMake Project",
                        "C++20 starter with compile_commands.json export. Requires CMake and a C++ compiler.",
                    ),
                    (
                        ProjectLanguage::Cpp,
                        ProjectTemplate::ClassLibrary,
                        "C++",
                        "Class Library",
                        "CMake static library with a public header and a small Greeter example.",
                    ),
                    (
                        ProjectLanguage::CSharp,
                        ProjectTemplate::Console,
                        "C#",
                        "Console App",
                        ".NET command-line app for Windows, Linux and macOS.",
                    ),
                    (
                        ProjectLanguage::CSharp,
                        ProjectTemplate::ClassLibrary,
                        "C#",
                        "Class Library",
                        "A reusable .NET class library.",
                    ),
                ];
                for (language, template, tag, title, description) in templates {
                    let selected = form.language == language && form.template == template;
                    if super::ui::card(
                        &mut columns[0],
                        tag,
                        &format!("{title}  ({tag})"),
                        description,
                        selected,
                    ) {
                        form.language = language;
                        form.template = template;
                    }
                }
                let selected = form.language == ProjectLanguage::CSharp
                    && form.template == ProjectTemplate::AvaloniaApp;
                if super::ui::card(
                    &mut columns[0],
                    "▣",
                    "Desktop app  (C# · Avalonia)",
                    "Cross-platform desktop app. Requires .NET SDK and NuGet package restore.",
                    selected,
                ) {
                    form.language = ProjectLanguage::CSharp;
                    form.template = ProjectTemplate::AvaloniaApp;
                }

                let ui = &mut columns[1];
                ui.label(RichText::new("Configure your project").size(18.0));
                ui.add_space(10.0);
                ui.label(RichText::new("Project name").color(theme::muted()));
                let name_changed = ui.add_sized(
                    [ui.available_width(), 34.0],
                    egui::TextEdit::singleline(&mut form.name).hint_text("e.g. MyApplication"),
                ).changed();
                if name_changed {
                    form.error = None;
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Location").color(theme::muted()));
                ui.horizontal(|ui| {
                    let location_changed = ui.add_sized(
                        [ui.available_width() - 100.0, 34.0],
                        egui::TextEdit::singleline(&mut form.location),
                    ).changed();
                    if location_changed {
                        form.error = None;
                    }
                    if ui.button("Browse…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_directory(&form.location)
                            .pick_folder()
                    {
                        form.location = path.display().to_string();
                    }
                });
                if form.language == ProjectLanguage::CSharp {
                    ui.add_space(8.0);
                    ui.label(RichText::new(".NET target framework").color(theme::muted()));
                    egui::ComboBox::from_id_salt("dotnet-framework")
                        .selected_text(&form.framework)
                        .show_ui(ui, |ui| {
                            for framework in ["net8.0", "net10.0"] {
                                ui.selectable_value(
                                    &mut form.framework,
                                    framework.to_owned(),
                                    framework,
                                );
                            }
                        });
                    if form.template == ProjectTemplate::Console {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Program.cs starting point").color(theme::muted()));
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut form.csharp_scaffold,
                                false,
                                "Minimal · Console.WriteLine",
                            );
                            ui.selectable_value(
                                &mut form.csharp_scaffold,
                                true,
                                "Class + Main method",
                            );
                        });
                    }
                }
                ui.add_space(12.0);
                egui::Frame::new()
                    .fill(theme::card())
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("PROJECT LOCATION")
                                .small()
                                .color(theme::muted()),
                        );
                        ui.label(format!(
                            "{}/{}",
                            form.location.trim_end_matches(['/', '\\']),
                            form.name
                        ));
                    });
                let destination = std::path::PathBuf::from(form.location.trim());
                match crate::project::validate_project_destination(&destination, form.name.trim()) {
                    Ok(path) => ui.colored_label(
                        Color32::from_rgb(133, 202, 156),
                        format!("Ready to create {}", path.display()),
                    ),
                    Err(message) if destination.join(form.name.trim()).exists() => {
                        ui.colored_label(Color32::from_rgb(232, 181, 86), message)
                    }
                    Err(message) => ui.colored_label(Color32::LIGHT_RED, message),
                };
                if let Some(error) = &form.error {
                    ui.add_space(8.0);
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
            });
        });
    created
}

fn show_toolchains_window(
    ctx: &egui::Context,
    is_open: Arc<AtomicBool>,
    toolchains: ToolchainManager,
    package_confirmation: Arc<Mutex<Option<PackageAction>>>,
) {
    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of("cairn.toolchain-window"),
        egui::ViewportBuilder::default()
            .with_title("Fugh · Toolchain Setup")
            .with_inner_size([760.0, 580.0])
            .with_min_inner_size([560.0, 400.0]),
        move |ui, _class| {
            {
                let user = crate::settings::current(ui.ctx());
                theme::apply(ui.ctx(), &user.appearance, &user.editor);
            }
            if ui.input(|input| input.viewport().close_requested()) {
                is_open.store(false, Ordering::Relaxed);
            }

            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("Toolchain setup");
                ui.label("Installed tool versions detected from PATH and Fugh's local tools directory.");
                ui.separator();
                let package_operation = toolchains.package_operation();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("TOOL").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !toolchains.is_refreshing() && !package_operation.running,
                                egui::Button::new("Refresh"),
                            )
                            .clicked()
                        {
                            toolchains.refresh(ui.ctx().clone());
                        }
                    });
                });
                ui.separator();
                if toolchains.is_refreshing() {
                    ui.spinner();
                    ui.label("Checking installed tools…");
                }
                for tool in toolchains.tools() {
                    ui.horizontal(|ui| {
                        let (status, color) = match &tool.version {
                            Some(version) => (version.as_str(), Color32::from_rgb(133, 202, 156)),
                            None => ("Missing", Color32::LIGHT_RED),
                        };
                        ui.label(tool.name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(status).color(color))
                                .on_hover_text(format!(
                                    "Executable searched {}: {}",
                                    if tool.name == "C# debugger" { "on PATH and in Fugh's user-local tool directory" } else { "on PATH" },
                                    tool.command,
                                ));
                        });
                    });
                }
                ui.add_space(12.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.heading("Tool installation");
                    ui.label("Install Fedora packages and approved tools from their official release feeds.");
                    let operation = package_operation;
                    let selection_id = egui::Id::unique("cairn-selected-tool-installs");
                    let mut selected_tools = ui
                        .ctx()
                        .data(|data| data.get_temp::<Vec<String>>(selection_id))
                        .unwrap_or_default();
                    ui.label(RichText::new("Install individually").strong());
                    for tool in toolchains.tools().into_iter().filter(|tool| {
                        tool.version.is_none() && ToolchainManager::tool_has_supported_installer(tool.name)
                    }) {
                        let mut selected = selected_tools.iter().any(|name| name == tool.name);
                        if ui.checkbox(&mut selected, tool.name).changed() {
                            if selected {
                                selected_tools.push(tool.name.to_owned());
                            } else {
                                selected_tools.retain(|name| name != tool.name);
                            }
                        }
                    }
                    let separately_installed = toolchains.tools().into_iter().filter(|tool| {
                        tool.version.is_none()
                            && !ToolchainManager::tool_has_supported_installer(tool.name)
                    }).collect::<Vec<_>>();
                    for tool in separately_installed {
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(Color32::LIGHT_RED, format!("{} is missing; Fedora package install is unavailable.", tool.name));
                            if tool.name == "C# debugger" {
                                ui.hyperlink_to("Get NetCoreDbg", "https://github.com/Samsung/netcoredbg/releases");
                            }
                        });
                    }
                    ui.ctx().data_mut(|data| data.insert_temp(selection_id, selected_tools.clone()));
                    ui.horizontal(|ui| {
                        let fedora_supported =
                            ToolchainManager::supports_fedora_package_management();
                        let roslyn_missing = toolchains.tools().iter().any(|tool| {
                            tool.name == "Roslyn language server" && tool.version.is_none()
                        });
                        let netcoredbg_missing = toolchains.tools().iter().any(|tool| {
                            tool.name == "C# debugger" && tool.version.is_none()
                        });
                        let install_actions_enabled = (fedora_supported || roslyn_missing || netcoredbg_missing)
                            && !operation.running
                            && !toolchains.is_refreshing();
                        if ui
                            .add_enabled(install_actions_enabled && !selected_tools.is_empty(), egui::Button::new(PackageAction::InstallSelected(Vec::new()).label()))
                            .clicked()
                        {
                            *package_confirmation
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(PackageAction::InstallSelected(selected_tools.clone()));
                        }
                        if ui
                            .add_enabled(install_actions_enabled, egui::Button::new(PackageAction::InstallMissing.label()))
                            .clicked()
                        {
                            *package_confirmation
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(PackageAction::InstallMissing);
                        }
                        if ui
                            .add_enabled(fedora_supported && !operation.running && !toolchains.is_refreshing(), egui::Button::new(PackageAction::UpdateInstalled.label()))
                            .clicked()
                        {
                            *package_confirmation
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(PackageAction::UpdateInstalled);
                        }
                    });
                    if !ToolchainManager::supports_fedora_package_management() {
                        ui.label("System package installation is Fedora only. The Roslyn server can be installed through the .NET tool feed.");
                    }
                    if operation.running {
                        ui.spinner();
                        ui.label("Installing selected tools. System packages may request administrator approval…");
                    }
                    if let Some(result) = &operation.result {
                        match result {
                            Ok(()) => {
                                ui.colored_label(
                                    Color32::from_rgb(133, 202, 156),
                                    "Tool installation completed.",
                                );
                            }
                            Err(error) => {
                                ui.colored_label(Color32::LIGHT_RED, error);
                            }
                        }
                    }
                    if !operation.output.is_empty() {
                        egui::ScrollArea::vertical()
                            .id_salt("tool-install-output")
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for line in &operation.output {
                                    ui.label(RichText::new(line).monospace());
                                }
                            });
                    }
                });
            });

            let action = package_confirmation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            if let Some(action) = action {
                egui::Modal::new(egui::Id::unique("cairn-package-confirmation"))
                    .show(ui.ctx(), |ui| {
                        ui.set_min_width(360.0);
                        ui.heading(action.label());
                        let requests_tool = |name: &str| {
                            matches!(&action, PackageAction::InstallMissing)
                                || matches!(&action, PackageAction::InstallSelected(names) if names.iter().any(|selected| selected == name))
                        };
                        let has_roslyn = requests_tool("Roslyn language server")
                            && toolchains.tools().iter().any(|tool| tool.name == "Roslyn language server" && tool.version.is_none());
                        let has_netcoredbg = requests_tool("C# debugger")
                            && toolchains.tools().iter().any(|tool| tool.name == "C# debugger" && tool.version.is_none());
                        if ToolchainManager::supports_fedora_package_management()
                            && !toolchains.planned_packages(&action).is_empty()
                        {
                            ui.label("Fugh will ask Fedora's PolicyKit authorization service before changing system packages.");
                            ui.label("Package manager command: pkexec dnf");
                            ui.label(format!("Packages: {}", toolchains.planned_packages(&action).join(", ")));
                        }
                        if has_roslyn {
                            ui.label("Install Roslyn with: dotnet tool install --global roslyn-language-server --prerelease");
                        }
                        if has_netcoredbg {
                            ui.label("Fugh will download the latest Linux NetCoreDbg release from Samsung/netcoredbg on GitHub and install it under your user data directory. It will not require administrator access.");
                            ui.label("Download: https://github.com/Samsung/netcoredbg/releases/latest");
                        }
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                *package_confirmation
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                            }
                            if ui
                                .button(RichText::new("Continue").strong())
                                .clicked()
                            {
                                let result =
                                    toolchains.start_tool_install_action(action, ui.ctx().clone());
                                if let Err(error) = result {
                                    toolchains.record_package_error(error);
                                    *package_confirmation
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                                } else {
                                    *package_confirmation
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                                }
                            }
                        });
                    });
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionUiState, Diagnostic, DiagnosticSeverity, WindowState, apply_lsp_workspace_edit,
        auto_indent_edit, auto_pair_edit, call_argument_context, call_signature_context,
        close_tabs, completion_query, completion_suggestions, declared_namespace, identifier_range,
        is_member_completion, language_for_document, local_signature_help, lsp_position_byte,
        member_suggestions, parse_build_diagnostic, parse_completion_items, relative_include_path,
        search_project_files, signature_help_info, signature_parameter_range,
        unsupported_project_action,
    };
    use crate::app::layout::EditorLayout;
    use crate::process::ProcessAction;
    use crate::project::{Document, ProjectInfo, ProjectLanguage, ProjectTemplate, Workspace};
    use std::sync::atomic::Ordering;

    #[test]
    fn project_action_availability_matches_supported_templates() {
        let library = ProjectInfo {
            name: "Library".to_owned(),
            path: std::path::PathBuf::new(),
            language: ProjectLanguage::Cpp,
            template: ProjectTemplate::ClassLibrary,
            entry_file: std::path::PathBuf::from("src/library.cpp"),
            project_file: None,
            properties: Default::default(),
        };
        assert!(unsupported_project_action(&library, ProcessAction::Build).is_none());
        assert!(unsupported_project_action(&library, ProcessAction::Run).is_some());
        assert!(unsupported_project_action(&library, ProcessAction::Debug).is_some());

        let imported = ProjectInfo {
            template: ProjectTemplate::VisualStudioCpp,
            ..library
        };
        assert!(unsupported_project_action(&imported, ProcessAction::Build).is_some());
    }

    #[test]
    fn signature_help_tracks_the_current_call_argument() {
        let end = |text: &str| text.chars().count();
        assert_eq!(call_argument_context("write(", end("write(")), Some(0));
        assert_eq!(
            call_argument_context("write(first, second", end("write(first, second")),
            Some(1)
        );
        assert_eq!(
            call_argument_context(
                "write(first, nested(x, y), third",
                end("write(first, nested(x, y), third")
            ),
            Some(2)
        );
        assert_eq!(
            call_argument_context("write(first, nested(x, y", end("write(first, nested(x, y")),
            Some(1)
        );
        assert_eq!(
            call_argument_context("left + right", end("left + right")),
            None
        );
        assert_eq!(
            call_signature_context("object.send(first", end("object.send(first")),
            Some(("send".to_owned(), 0))
        );
        let source = "MainWindow.a(\"Hello World\", \"Hi!\");";
        let cursor = source.find("Hi!").unwrap() + "Hi!".len();
        assert_eq!(
            call_argument_context(source, source[..cursor].chars().count()),
            Some(1)
        );
        let inside_method = "void Run() { MainWindow.a(\"Hello World\", \"Hi!\"); }";
        let second_argument = inside_method.find("Hi!").unwrap() + "Hi!".len();
        assert_eq!(
            call_argument_context(
                inside_method,
                inside_method[..second_argument].chars().count()
            ),
            Some(1)
        );
        let after_comma = "void Run() { MainWindow.a(\"Hello World\", ";
        assert_eq!(
            call_argument_context(after_comma, after_comma.chars().count()),
            Some(1)
        );
        let second = signature_parameter_range("void MainWindow.a(string a, string b)", 1).unwrap();
        assert_eq!(
            &"void MainWindow.a(string a, string b)"[second],
            " string b"
        );
    }

    #[test]
    fn signature_help_uses_server_parameter_ranges_and_documentation() {
        let response = serde_json::json!({
            "result": {
                "signatures": [{
                    "label": "fn send(name: &str, count: usize)",
                    "documentation": {"kind": "markdown", "value": "Sends a message"},
                    "parameters": [
                        {"label": [8, 18], "documentation": "Recipient name"},
                        {"label": [20, 33]}
                    ]
                }]
            }
        });
        let info = signature_help_info(&response).expect("signature response parses");
        assert_eq!(info.label, "fn send(name: &str, count: usize)");
        assert_eq!(info.documentation.as_deref(), Some("Sends a message"));
        assert_eq!(info.parameters[0].label_range, Some(8..18));
        assert_eq!(
            info.parameters[0].documentation.as_deref(),
            Some("Recipient name")
        );
        assert_eq!(info.parameters[1].label_range, Some(20..33));
    }

    #[test]
    fn signature_help_preserves_all_overloads_for_navigation() {
        let signatures = (0..55)
            .map(|index| {
                serde_json::json!({
                    "label": format!("void Send(string value, int mode{index})"),
                    "parameters": [
                        {"label": "value"},
                        {"label": format!("mode{index}")}
                    ]
                })
            })
            .collect::<Vec<_>>();
        let response = serde_json::json!({
            "result": {
                "signatures": signatures,
                "activeSignature": 37
            }
        });
        let info = signature_help_info(&response).expect("overload response parses");
        assert_eq!(info.overloads.len(), 55);
        assert_eq!(info.active_signature, 37);
        assert!(info.overloads[37].label.contains("mode37"));
    }

    #[test]
    fn local_signature_help_finds_project_function_parameters() {
        let path = std::env::temp_dir().join("project/src/example.cpp");
        let symbols = vec![crate::project::ProjectSymbol {
            name: "send_message".to_owned(),
            kind: "C++ function".to_owned(),
            path: path.clone(),
            namespace: None,
            line: 1,
        }];
        let source = "bool send_message(const std::string& name, int count) { return true; }";
        let info = local_signature_help(&symbols, None, Some(&path), source, "send_message")
            .expect("local function signature is available");
        assert_eq!(
            info.label,
            "bool send_message(const std::string& name, int count)"
        );
        assert_eq!(info.parameters.len(), 2);
        let second = info.parameters[1].label_range.clone().unwrap();
        assert_eq!(&info.label[second], "int count");
    }

    #[test]
    fn local_signature_help_collects_same_name_overloads() {
        let path = std::env::temp_dir().join("project/src/overloads.cpp");
        let symbols = vec![
            crate::project::ProjectSymbol {
                name: "send_message".to_owned(),
                kind: "C++ function".to_owned(),
                path: path.clone(),
                namespace: None,
                line: 1,
            },
            crate::project::ProjectSymbol {
                name: "send_message".to_owned(),
                kind: "C++ function".to_owned(),
                path: path.clone(),
                namespace: None,
                line: 2,
            },
        ];
        let source = "void send_message(const std::string& name) {}\nvoid send_message(const std::string& name, int retries) {}";
        let info = local_signature_help(&symbols, None, Some(&path), source, "send_message")
            .expect("local signatures are available");
        assert_eq!(info.overloads.len(), 2);
        assert!(info.overloads[0].label.contains("name)"));
        assert!(info.overloads[1].label.contains("retries"));
    }

    #[test]
    fn language_server_completion_items_keep_insert_text_and_details() {
        let response = serde_json::json!({
            "result": {"items": [
                {"label": "WriteLine", "insertText": "WriteLine", "detail": "Console method"},
                {"label": "Write", "textEdit": {"newText": "Write"}, "documentation": {"value": "Writes text"}}
            ]}
        });
        let items = parse_completion_items(&response);
        assert_eq!(
            items[0],
            ("WriteLine".to_owned(), "Console method".to_owned())
        );
        assert_eq!(items[1], ("Write".to_owned(), "Writes text".to_owned()));
    }

    #[test]
    fn diagnostics_respect_single_line_and_multiline_ranges() {
        let diagnostic = Diagnostic {
            severity: DiagnosticSeverity::Error,
            message: "problem".to_owned(),
            file: None,
            line: Some(3),
            column: Some(4),
            end_line: Some(4),
            end_column: Some(2),
            source: None,
        };
        assert!(!diagnostic.contains_position(2, 10));
        assert!(!diagnostic.contains_position(3, 3));
        assert!(diagnostic.contains_position(3, 4));
        assert!(diagnostic.contains_position(4, 1));
        assert!(!diagnostic.contains_position(4, 2));
        assert!(!diagnostic.contains_position(5, 1));

        let line_only = Diagnostic {
            end_line: None,
            end_column: None,
            ..diagnostic
        };
        assert!(line_only.contains_position(3, 100));
        assert!(!line_only.contains_position(4, 1));
    }

    #[test]
    fn wizard_opens_inline_without_extra_windows() {
        let windows = WindowState::new(Workspace::default(), EditorLayout::default());
        assert!(!windows.wizard_active());
        windows.open_project_wizard();
        assert!(windows.wizard_active());
        assert!(!windows.toolchains_open.load(Ordering::Relaxed));
    }

    #[test]
    fn window_close_actions_preserve_unsaved_tabs() {
        let mut workspace = Workspace::default();
        workspace.documents[0].dirty = true;
        close_tabs(&mut workspace, vec![0]);
        assert_eq!(workspace.documents.len(), 1);
        assert!(workspace.message.contains("unsaved changes"));

        workspace.documents[0].dirty = false;
        close_tabs(&mut workspace, vec![0]);
        assert!(workspace.documents.is_empty());
    }

    #[test]
    fn lsp_positions_convert_utf16_columns_without_splitting_surrogate_pairs() {
        let text = "a😀b\r\nnext";
        assert_eq!(lsp_position_byte(text, 0, 3), Some(5));
        assert_eq!(lsp_position_byte(text, 0, 2), None);
        assert_eq!(lsp_position_byte(text, 1, 2), Some(10));
    }

    #[test]
    fn rename_workspace_edit_stages_multiple_project_files_as_unsaved_buffers() {
        let root = std::env::temp_dir().join(format!(
            "fugh-rename-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("rename test project created");
        let first = root.join("main.cpp");
        let second = root.join("helper.cpp");
        std::fs::write(&first, "int value = 1;\n").expect("first source written");
        std::fs::write(&second, "return value;\n").expect("second source written");
        let mut workspace = Workspace::default();
        workspace.current = Some(ProjectInfo {
            name: "RenameTest".to_owned(),
            path: root.clone(),
            language: ProjectLanguage::Cpp,
            template: ProjectTemplate::Console,
            entry_file: std::path::PathBuf::from("main.cpp"),
            project_file: None,
            properties: Default::default(),
        });
        let edit = serde_json::json!({"changes": {
            url::Url::from_file_path(&first).unwrap().to_string(): [{
                "range": {"start": {"line": 0, "character": 4}, "end": {"line": 0, "character": 9}},
                "newText": "count"
            }],
            url::Url::from_file_path(&second).unwrap().to_string(): [{
                "range": {"start": {"line": 0, "character": 7}, "end": {"line": 0, "character": 12}},
                "newText": "count"
            }]
        }});

        assert_eq!(apply_lsp_workspace_edit(&mut workspace, &edit).unwrap(), 2);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "int value = 1;\n");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "return value;\n");
        let first_buffer = workspace
            .documents
            .iter()
            .find(|doc| doc.path.as_deref() == Some(first.as_path()))
            .unwrap();
        let second_buffer = workspace
            .documents
            .iter()
            .find(|doc| doc.path.as_deref() == Some(second.as_path()))
            .unwrap();
        assert_eq!(first_buffer.text, "int count = 1;\n");
        assert_eq!(second_buffer.text, "return count;\n");
        assert!(first_buffer.dirty && second_buffer.dirty);
        std::fs::remove_dir_all(root).expect("rename test project removed");
    }

    #[test]
    fn project_search_finds_saved_and_unsaved_text_and_returns_line_numbers() {
        let root = std::env::temp_dir().join(format!(
            "cairn-search-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("search test project created");
        std::fs::write(root.join("saved.cpp"), "int main() {\n  return 0;\n}\n")
            .expect("saved test source written");
        let unsaved_path = root.join("unsaved.cpp");
        let mut workspace = Workspace::default();
        workspace.current = Some(ProjectInfo {
            name: "SearchTest".to_owned(),
            path: root.clone(),
            language: ProjectLanguage::Cpp,
            template: ProjectTemplate::Console,
            entry_file: std::path::PathBuf::from("saved.cpp"),
            project_file: None,
            properties: Default::default(),
        });
        workspace.files = vec![
            std::path::PathBuf::from("saved.cpp"),
            std::path::PathBuf::from("unsaved.cpp"),
        ];
        workspace.documents.push(Document {
            path: Some(unsaved_path),
            title: "unsaved.cpp".to_owned(),
            text: "// needle in unsaved text\nvoid run();".to_owned(),
            dirty: true,
            breakpoints: Vec::new(),
            pinned: false,
        });

        let results = search_project_files(&workspace, "needle");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, std::path::PathBuf::from("unsaved.cpp"));
        assert_eq!(results[0].line, 1);

        let results = search_project_files(&workspace, "return");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, std::path::PathBuf::from("saved.cpp"));
        assert_eq!(results[0].line, 2);
        std::fs::remove_dir_all(root).expect("search test project removed");
    }

    #[test]
    fn editor_detects_supported_source_languages_from_file_names() {
        for (title, language) in [
            ("main.cpp", "cpp"),
            ("Program.cs", "cs"),
            ("MainWindow.axaml", "xml"),
            ("lib.rs", "rs"),
            ("settings.toml", "toml"),
            ("data.json", "json"),
            ("notes.txt", "txt"),
        ] {
            let document = Document {
                path: None,
                title: title.to_owned(),
                text: String::new(),
                dirty: false,
                breakpoints: Vec::new(),
                pinned: false,
            };
            assert_eq!(language_for_document(&document), language);
        }
    }

    #[test]
    fn code_completion_filters_language_keywords_and_avalonia_controls() {
        assert!(
            completion_suggestions("cs", "con", &[], false)
                .iter()
                .any(|suggestion| suggestion.insert == "Console")
        );
        assert!(
            completion_suggestions("cpp", "std::", &[], false)
                .iter()
                .any(|suggestion| suggestion.insert == "std::cout")
        );
        assert!(
            completion_suggestions("xml", "text", &[], false)
                .iter()
                .any(|suggestion| suggestion.insert == "TextBlock")
        );
        assert!(completion_suggestions("cs", "x", &[], false).is_empty());
    }

    #[test]
    fn project_completion_shows_symbol_source_and_builds_relative_cpp_includes() {
        let symbols = [crate::project::ProjectSymbol {
            name: "CustomerService".to_owned(),
            kind: "class".to_owned(),
            path: std::path::PathBuf::from("Models/CustomerService.cs"),
            namespace: Some("Starter.Models".to_owned()),
            line: 4,
        }];
        let suggestions = completion_suggestions("cs", "cus", &symbols, false);
        assert!(
            suggestions
                .iter()
                .any(|suggestion| { suggestion.insert == "CustomerService" })
        );
        assert_eq!(
            relative_include_path(
                std::path::Path::new("src/main.cpp"),
                std::path::Path::new("include/widgets.hpp")
            )
            .as_deref(),
            Some("../include/widgets.hpp")
        );
        assert_eq!(
            relative_include_path(
                std::path::Path::new("src/main.cpp"),
                std::path::Path::new("src/models/widget.hpp")
            )
            .as_deref(),
            Some("models/widget.hpp")
        );
        assert_eq!(
            declared_namespace("using System;\nnamespace Starter.Models;\n"),
            Some("Starter.Models".to_owned())
        );
    }

    #[test]
    fn typed_member_completion_uses_the_receiver_type() {
        let text = "string name = \"x\";\nname.";
        let start = text.chars().count();
        let items = member_suggestions("cs", text, start, "").expect("string members");
        assert!(items.iter().any(|item| item.insert == "Substring"));
        let items = member_suggestions("cs", "Console.Wr", 8, "Wr").expect("Console members");
        assert!(items.iter().any(|item| item.insert == "WriteLine"));
        assert!(items.iter().all(|item| item.insert.starts_with("Wr")));
        let items = member_suggestions("cpp", "std::co", 5, "co").expect("std members");
        assert!(items.iter().any(|item| item.insert == "cout"));
        let items = member_suggestions("cpp", "btn->se", 5, "se").expect("widget members");
        assert!(items.iter().any(|item| item.insert == "setText"));
        assert!(member_suggestions("cs", "mystery.", 8, "").is_none());
    }

    #[test]
    fn dot_with_an_empty_word_is_not_treated_as_dismissed() {
        let state = CompletionUiState::default();
        assert!(state.dismissed_word.as_deref() != Some(""));
    }

    #[test]
    fn member_completion_after_dot_lists_all_project_functions() {
        let symbols = (0..16)
            .map(|index| crate::project::ProjectSymbol {
                name: format!("DoWork{index}"),
                kind: "C# function".to_owned(),
                path: std::path::PathBuf::from("Services/Worker.cs"),
                namespace: Some("Demo.Services".to_owned()),
                line: index + 1,
            })
            .collect::<Vec<_>>();
        let text = "worker.";
        let (range, word) = completion_query(text, text.chars().count()).unwrap();
        assert!(word.is_empty());
        assert!(is_member_completion(text, range.start));
        let suggestions = completion_suggestions("cs", &word, &symbols, true);
        assert!(suggestions.iter().any(|item| item.insert == "WriteLine"));
        for index in 0..16 {
            assert!(
                suggestions
                    .iter()
                    .any(|item| item.insert == format!("DoWork{index}"))
            );
        }
    }

    #[test]
    fn parses_build_errors_for_common_dotnet_and_cpp_formats() {
        let dotnet =
            parse_build_diagnostic("Program.cs(8,14): error CS0103: The name was not found");
        assert_eq!(
            dotnet,
            Some(super::Diagnostic {
                severity: DiagnosticSeverity::Error,
                message: "error CS0103: The name was not found".to_owned(),
                file: Some("Program.cs".to_owned()),
                line: Some(8),
                column: Some(14),
                end_line: None,
                end_column: None,
                source: Some("build".to_owned()),
            })
        );
        let cpp = parse_build_diagnostic("src/main.cpp:12:4: error: expected ';' before '}'");
        assert_eq!(cpp.and_then(|diagnostic| diagnostic.line), Some(12));
    }

    #[test]
    fn editor_pairs_delimiters_and_skips_over_existing_closers() {
        let mut paired = "call(".to_owned();
        assert_eq!(auto_pair_edit("call", &mut paired, 5), Some(5));
        assert_eq!(paired, "call()");

        let mut skipped = "call))".to_owned();
        assert_eq!(auto_pair_edit("call)", &mut skipped, 5), Some(5));
        assert_eq!(skipped, "call)");
    }

    #[test]
    fn editor_indents_new_lines_and_formats_empty_brace_blocks() {
        let mut indented = "    int value;\n".to_owned();
        assert_eq!(
            auto_indent_edit("    int value;", &mut indented, 15, 4, false),
            Some(19)
        );
        assert_eq!(indented, "    int value;\n    ");

        let mut block = "{\n}".to_owned();
        assert_eq!(auto_indent_edit("{}", &mut block, 2, 4, false), Some(6));
        assert_eq!(block, "{\n    \n}");

        let mut two_spaces = "if (ready) {\n}".to_owned();
        assert_eq!(
            auto_indent_edit("if (ready) {}", &mut two_spaces, 13, 2, false),
            Some(15)
        );
        assert_eq!(two_spaces, "if (ready) {\n  \n}");

        let mut tabbed = "if (ready) {\n}".to_owned();
        assert_eq!(
            auto_indent_edit("if (ready) {}", &mut tabbed, 13, 2, true),
            Some(14)
        );
        assert_eq!(tabbed, "if (ready) {\n\t\n}");
    }

    #[test]
    fn completion_query_tracks_the_word_at_the_cursor() {
        assert_eq!(
            completion_query("var count", 6),
            Some((4..6, "co".to_owned()))
        );
        assert_eq!(
            completion_query("Console.WriteLine", 7),
            Some((0..7, "Console".to_owned()))
        );
        assert!(
            completion_suggestions("cs", "cw", &[], false)
                .iter()
                .any(|suggestion| suggestion.insert == "Console.WriteLine")
        );
    }

    #[test]
    fn hover_identifier_range_selects_the_complete_symbol() {
        assert_eq!(
            identifier_range("let velocity = distance / time", 7),
            Some(4..12)
        );
        assert_eq!(identifier_range("let velocity = distance / time", 3), None);
        assert_eq!(identifier_range("αβ_value + 2", 1), Some(0..8));
    }
}
