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

pub struct DiscoveredFile {
    pub absolute_path: PathBuf,
    pub relative_path: String,
    pub lang: &'static str,
    pub is_focus: bool,
}

pub fn discover_files(
    root: &Path,
    focus_patterns: &[String],
) -> Result<Vec<DiscoveredFile>, globset::Error> {
    let focus_globset = build_focus_globset(focus_patterns)?;
    let mut files = Vec::new();

    if root.is_file() {
        let ext = root
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if let Some(lang) = get_language_by_extension(&ext) {
            let rel_str = root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .replace('\\', "/");
            let is_focus = focus_globset
                .as_ref()
                .map(|set| set.is_match(&rel_str))
                .unwrap_or(false);

            files.push(DiscoveredFile {
                absolute_path: root.to_path_buf(),
                relative_path: rel_str,
                lang,
                is_focus,
            });
        }
        return Ok(files);
    }

    let base_dir = root;
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            e.depth() == 0 || !should_ignore(e.path().strip_prefix(root).unwrap_or(e.path()))
        })
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if let Some(lang) = get_language_by_extension(&ext) {
                let rel = match path.strip_prefix(base_dir) {
                    Ok(r) => r.to_string_lossy().replace('\\', "/"),
                    Err(_) => path.to_string_lossy().replace('\\', "/"),
                };

                let is_focus = focus_globset
                    .as_ref()
                    .map(|set| set.is_match(&rel))
                    .unwrap_or(false);

                files.push(DiscoveredFile {
                    absolute_path: path.to_path_buf(),
                    relative_path: rel,
                    lang,
                    is_focus,
                });
            }
        }
    }

    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}
