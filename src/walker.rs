use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const DEFAULT_IGNORES: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    "env",
    "dist",
    "build",
    "target",
    ".next",
    ".nuxt",
    ".cache",
    ".idea",
    ".vscode",
    "coverage",
    ".pytest_cache",
    ".mypy_cache",
    ".tox",
    "vendor",
];

pub fn get_language_by_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "py" => Some("python"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" => Some("javascript"),
        "go" => Some("go"),
        "rs" => Some("rust"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cpp" | "hpp" | "cc" | "cxx" => Some("cpp"),
        _ => None,
    }
}

pub fn should_ignore(path: &Path) -> bool {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if DEFAULT_IGNORES.contains(&name.as_ref()) {
            return true;
        }
        if name.starts_with('.') && name != "." && name != ".." {
            return true;
        }
    }
    false
}

pub fn build_focus_globset(patterns: &[String]) -> Result<Option<GlobSet>, globset::Error> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pat in patterns {
        // Normalize slashes
        let norm = pat.replace('\\', "/");
        builder.add(Glob::new(&norm)?);
    }
    builder.build().map(Some)
}

#[derive(Debug)]
pub struct DiscoveredFile {
    pub absolute_path: PathBuf,
    pub relative_path: String,
    pub lang: &'static str,
    pub is_focus: bool,
}

pub fn discover_files(
    root: &Path,
    focus_patterns: &[String],
) -> Result<Vec<DiscoveredFile>, DiscoveryError> {
    let discovery = discover_files_with_limit(root, focus_patterns, usize::MAX)?;
    if !discovery.warnings.is_empty() {
        return Err(DiscoveryError::Scan(discovery.warnings.join("\n")));
    }
    Ok(discovery.files)
}

#[derive(Debug)]
pub struct Discovery {
    pub files: Vec<DiscoveredFile>,
    pub excluded: Vec<ExcludedFile>,
    pub warnings: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ExcludedFile {
    #[serde(rename = "path")]
    pub relative_path: String,
    /// First matching exclude pattern, as supplied by the caller.
    pub pattern: String,
}

struct FileSelection<'a> {
    focus: Option<GlobSet>,
    exclude: Option<GlobSet>,
    exclude_patterns: &'a [String],
}

impl<'a> FileSelection<'a> {
    fn new(focus: &[String], exclude: &'a [String]) -> Result<Self, DiscoveryError> {
        Ok(Self {
            focus: build_focus_globset(focus).map_err(DiscoveryError::InvalidFocus)?,
            exclude: build_focus_globset(exclude).map_err(DiscoveryError::InvalidExclude)?,
            exclude_patterns: exclude,
        })
    }
}

#[derive(Debug)]
pub enum DiscoveryError {
    InvalidFocus(globset::Error),
    InvalidExclude(globset::Error),
    FileLimit { limit: usize },
    Scan(String),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFocus(err) => write!(f, "Invalid focus pattern: {err}"),
            Self::InvalidExclude(err) => write!(f, "Invalid exclude pattern: {err}"),
            Self::FileLimit { limit } => write!(
                f,
                "Source file count exceeds {limit}. Select a smaller directory."
            ),
            Self::Scan(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidFocus(err) | Self::InvalidExclude(err) => Some(err),
            _ => None,
        }
    }
}

/// Collect supported files, preserving scan failures and stopping at the limit.
pub fn discover_files_with_limit(
    root: &Path,
    focus_patterns: &[String],
    max_files: usize,
) -> Result<Discovery, DiscoveryError> {
    discover_files_with_filters(root, focus_patterns, &[], max_files)
}

/// Discover supported files, then exclude matching paths unless focused.
/// Patterns use paths relative to the selected directory (a basename for a file).
/// The scan limit counts retained and excluded files; neither requires reading source.
pub fn discover_files_with_filters(
    root: &Path,
    focus_patterns: &[String],
    exclude_patterns: &[String],
    max_files: usize,
) -> Result<Discovery, DiscoveryError> {
    let selection = FileSelection::new(focus_patterns, exclude_patterns)?;
    let metadata = match std::fs::metadata(root) {
        Ok(metadata) => metadata,
        Err(err) => {
            return Err(DiscoveryError::Scan(format!(
                "Cannot inspect '{}': {err}",
                root.display()
            )))
        }
    };
    if metadata.is_file() {
        let base = root.parent().unwrap_or(Path::new(""));
        return collect_files(
            base,
            std::iter::once(Ok(root.to_path_buf())),
            &selection,
            max_files,
        );
    }

    let entries = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            e.depth() == 0 || !should_ignore(e.path().strip_prefix(root).unwrap_or(e.path()))
        })
        .filter_map(|entry| match entry {
            Err(err) => Some(Err(format!("Cannot scan directory: {err}"))),
            Ok(entry) if entry.file_type().is_file() => Some(Ok(entry.into_path())),
            Ok(entry) if entry.file_type().is_symlink() => match std::fs::metadata(entry.path()) {
                Ok(metadata) if metadata.is_file() => Some(Ok(entry.into_path())),
                Ok(_) => None,
                Err(err) => Some(Err(format!(
                    "Cannot inspect '{}': {err}",
                    entry.path().display()
                ))),
            },
            Ok(_) => None,
        });
    collect_files(root, entries, &selection, max_files)
}

fn collect_files(
    root: &Path,
    entries: impl IntoIterator<Item = Result<PathBuf, String>>,
    selection: &FileSelection<'_>,
    max_files: usize,
) -> Result<Discovery, DiscoveryError> {
    let mut files = Vec::new();
    let mut excluded = Vec::new();
    let mut warnings = Vec::new();
    for entry in entries {
        let path = match entry {
            Ok(path) => path,
            Err(err) => {
                warnings.push(err);
                continue;
            }
        };
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if let Some(lang) = get_language_by_extension(&ext) {
            if files.len() + excluded.len() >= max_files {
                return Err(DiscoveryError::FileLimit { limit: max_files });
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let is_focus = selection
                .focus
                .as_ref()
                .map(|set| set.is_match(&rel))
                .unwrap_or(false);
            if !is_focus {
                if let Some(index) = selection
                    .exclude
                    .as_ref()
                    .and_then(|set| set.matches(&rel).into_iter().next())
                {
                    excluded.push(ExcludedFile {
                        relative_path: rel,
                        pattern: selection.exclude_patterns[index].clone(),
                    });
                    continue;
                }
            }
            files.push(DiscoveredFile {
                absolute_path: path,
                relative_path: rel,
                lang,
                is_focus,
            });
        }
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    excluded.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(Discovery {
        files,
        excluded,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_errors_are_preserved_with_partial_results() {
        let root = Path::new("project");
        let entries = [
            Ok(root.join("z.py")),
            Err("Cannot scan 'private': permission denied".to_owned()),
            Ok(root.join("a.ts")),
        ];
        let discovery =
            collect_files(root, entries, &FileSelection::new(&[], &[]).unwrap(), 10).unwrap();
        assert_eq!(discovery.files.len(), 2);
        assert_eq!(discovery.files[0].relative_path, "a.ts");
        assert_eq!(discovery.files[1].relative_path, "z.py");
        assert_eq!(
            discovery.warnings,
            ["Cannot scan 'private': permission denied"]
        );
    }

    #[test]
    fn file_limit_stops_consuming_entries_immediately() {
        let root = Path::new("project");
        let entries = [
            Ok(root.join("notes.txt")),
            Ok(root.join("a.py")),
            Ok(root.join("b.py")),
            Ok(root.join("c.py")),
        ]
        .into_iter()
        .chain(std::iter::from_fn(|| {
            panic!("Scanner consumed entries after exceeding its limit")
        }));
        let error =
            collect_files(root, entries, &FileSelection::new(&[], &[]).unwrap(), 2).unwrap_err();
        assert!(matches!(error, DiscoveryError::FileLimit { limit: 2 }));
    }

    #[test]
    fn file_limit_allows_exactly_the_limit() {
        let root = Path::new("project");
        let entries = [Ok(root.join("a.py")), Ok(root.join("b.py"))];
        let discovery =
            collect_files(root, entries, &FileSelection::new(&[], &[]).unwrap(), 2).unwrap();
        assert_eq!(discovery.files.len(), 2);
    }
}
