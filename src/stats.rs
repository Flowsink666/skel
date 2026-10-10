#[derive(Debug, Clone)]
pub struct FileResult {
    pub path: String,
    pub lang: String,
    pub mode: String,
    pub orig_tokens: usize,
    pub skel_tokens: usize,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct RepoStats {
    pub file_count: usize,
    pub orig_tokens: usize,
    pub skel_tokens: usize,
    pub saved_tokens: usize,
    pub ratio: f64,
    pub savings_usd: f64,
}

/// Fast token estimation (~3.5 chars per token for source code).
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    std::cmp::max(1, (text.len() as f64 / 3.5).round() as usize)
}

pub fn compute_stats(results: &[FileResult]) -> RepoStats {
    let mut orig_tokens = 0;
    let mut skel_tokens = 0;

    for r in results {
        orig_tokens += r.orig_tokens;
        skel_tokens += r.skel_tokens;
    }

    let saved_tokens = orig_tokens.saturating_sub(skel_tokens);
    let ratio = if orig_tokens > 0 {
        (saved_tokens as f64 / orig_tokens as f64) * 100.0
    } else {
        0.0
    };

    // Claude 3.5 Sonnet / GPT-4o input tier: $3.00 / 1M tokens
    let savings_usd = (saved_tokens as f64 / 1_000_000.0) * 3.0;

    RepoStats {
        file_count: results.len(),
        orig_tokens,
        skel_tokens,
        saved_tokens,
        ratio,
        savings_usd,
    }
}
