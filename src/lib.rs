pub mod c_like;
pub mod formatter;
pub mod mcp;
pub mod python;
pub mod skeletonizer;
pub mod stats;
pub mod typescript;
pub mod walker;

pub use formatter::{format_context_summary, format_markdown, format_terminal_report};
pub use skeletonizer::skeletonize;
pub use stats::{compute_stats, estimate_tokens, FileResult, RepoStats};
pub use walker::{discover_files, DiscoveredFile};
