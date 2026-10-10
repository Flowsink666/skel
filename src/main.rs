use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::process;

use skel::formatter::format_thousands;
use skel::walker::{discover_files_with_limit, DiscoveryError};
use skel::{
    compute_stats, estimate_tokens, format_markdown, format_terminal_report, skeletonize,
    FileResult,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "skel",
    version = VERSION,
    about = "AST-aware Code Skeletonizer: Reduce codebase context for LLMs."
)]
struct Args {
    /// Target directory or file
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Keep full code for matching paths (e.g. -f '*/auth*')
    #[arg(short, long, action = clap::ArgAction::Append)]
    focus: Vec<String>,

    /// Write output to file (e.g. -o context.md)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Print statistics summary only
    #[arg(short, long)]
    stats: bool,

    /// Serve read-only MCP tools over stdin/stdout, scoped to the target directory
    #[arg(long, conflicts_with_all = ["focus", "output", "stats"])]
    mcp: bool,
}

fn main() {
    let args = Args::parse();
    if args.mcp {
        if let Err(err) = skel::mcp::serve(&args.path) {
            eprintln!("MCP server error: {err}");
            process::exit(1);
        }
        return;
    }
    let root = match args.path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let p = args.path.clone();
            if !p.exists() {
                eprintln!("Error: Path '{}' does not exist.", args.path.display());
                process::exit(1);
            }
            p
        }
    };

    let discovery =
        discover_files_with_limit(&root, &args.focus, usize::MAX).unwrap_or_else(|err| {
            if let DiscoveryError::InvalidFocus(err) = err {
                eprintln!("Error: Invalid --focus pattern: {err}");
                process::exit(2);
            }
            eprintln!("Error: {err}");
            process::exit(1);
        });
    for warning in &discovery.warnings {
        eprintln!("Warning: {warning}");
    }
    if discovery.files.is_empty() && !discovery.warnings.is_empty() {
        process::exit(1);
    }
    let discovered = discovery.files;
    if discovered.is_empty() {
        println!("No supported source files found in target directory.");
        return;
    }

    let mut results = Vec::with_capacity(discovered.len());

    for file in discovered {
        let content = match fs::read_to_string(&file.absolute_path) {
            Ok(c) => c,
            Err(_) => continue, // Skip unreadable or non-UTF8 files
        };

        let orig_tokens = estimate_tokens(&content);
        let ext = file
            .absolute_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_jsx = ext == "tsx" || ext == "jsx";

        let (mode, final_content, skel_tokens) = if file.is_focus {
            ("FULL".to_string(), content, orig_tokens)
        } else {
            let pruned = skeletonize(&content, file.lang, is_jsx);
            let tokens = estimate_tokens(&pruned);
            ("SKELETON".to_string(), pruned, tokens)
        };

        results.push(FileResult {
            path: file.relative_path,
            lang: file.lang.to_string(),
            mode,
            orig_tokens,
            skel_tokens,
            content: final_content,
        });
    }

    let repo_stats = compute_stats(&results);

    // Print summary box to console
    print!("{}", format_terminal_report(&repo_stats, VERSION));

    if args.stats {
        return;
    }

    let md_content = format_markdown(&results, &repo_stats);

    if let Some(out_path) = args.output {
        if let Err(e) = fs::write(&out_path, &md_content) {
            eprintln!("Error writing to {}: {}", out_path.display(), e);
            process::exit(1);
        }
        println!(
            "[OK] Context written to: {} ({} bytes)",
            out_path.display(),
            format_thousands(md_content.len())
        );
    } else {
        let lines: Vec<&str> = md_content.lines().collect();
        let preview_count = std::cmp::min(lines.len(), 40);
        println!("--- Preview (first 40 lines, use -o context.md to save) ---");
        for line in &lines[..preview_count] {
            println!("{}", line);
        }
        if lines.len() > 40 {
            println!("...\n[Truncated. Use `-o context.md` to output full markdown]");
        }
    }
}
