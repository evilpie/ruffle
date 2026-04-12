use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompareResult {
    Match {
        elapsed_ms: u64,
    },
    Diff {
        expected_lines: usize,
        actual_lines: usize,
        first_diff_line: usize,
        elapsed_ms: u64,
        #[serde(skip)]
        expected: String,
        #[serde(skip)]
        actual: String,
    },
    NoFlashOutput {
        elapsed_ms: u64,
    },
    RunFailed {
        reason: String,
    },
    Skipped {
        reason: String,
    },
}

impl CompareResult {
    pub fn short_label(&self) -> &'static str {
        match self {
            CompareResult::Match { .. } => "MATCH",
            CompareResult::Diff { .. } => "DIFF",
            CompareResult::NoFlashOutput { .. } => "NO_OUTPUT",
            CompareResult::RunFailed { .. } => "FAILED",
            CompareResult::Skipped { .. } => "SKIP",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TestResult {
    pub name: String,
    pub dir: String,
    pub result: CompareResult,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub results: Vec<TestResult>,
}

impl Report {
    pub fn write(&self, report_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(report_dir).with_context(|| {
            format!("creating report dir {}", report_dir.display())
        })?;
        let diffs_dir = report_dir.join("diffs");
        std::fs::create_dir_all(&diffs_dir)?;

        // JSON (full machine-readable).
        let json_path = report_dir.join("results.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&json_path, json)
            .with_context(|| format!("writing {}", json_path.display()))?;

        // Per-test diff files are written inline as tests run (see main.rs),
        // so they are available during long runs. Nothing to do here.
        let _ = diffs_dir;

        // Markdown summary.
        let md = self.summary_md();
        std::fs::write(report_dir.join("summary.md"), md)?;
        Ok(())
    }

    pub fn print_summary(&self) {
        let (matches, diffs, no_out, failed, skipped) = self.counts();
        eprintln!();
        eprintln!("=== Summary ===");
        eprintln!("  total:      {}", self.results.len());
        eprintln!("  match:      {matches}");
        eprintln!("  diff:       {diffs}");
        eprintln!("  no output:  {no_out}");
        eprintln!("  run failed: {failed}");
        eprintln!("  skipped:    {skipped}");
    }

    fn counts(&self) -> (usize, usize, usize, usize, usize) {
        let mut m = 0;
        let mut d = 0;
        let mut n = 0;
        let mut f = 0;
        let mut s = 0;
        for r in &self.results {
            match &r.result {
                CompareResult::Match { .. } => m += 1,
                CompareResult::Diff { .. } => d += 1,
                CompareResult::NoFlashOutput { .. } => n += 1,
                CompareResult::RunFailed { .. } => f += 1,
                CompareResult::Skipped { .. } => s += 1,
            }
        }
        (m, d, n, f, s)
    }

    fn summary_md(&self) -> String {
        use std::fmt::Write as _;
        let (m, d, n, f, s) = self.counts();
        let mut out = String::new();
        let _ = writeln!(out, "# Flash compare results");
        let _ = writeln!(out);
        let _ = writeln!(out, "- total: {}", self.results.len());
        let _ = writeln!(out, "- match: {m}");
        let _ = writeln!(out, "- diff: {d}");
        let _ = writeln!(out, "- no output: {n}");
        let _ = writeln!(out, "- run failed: {f}");
        let _ = writeln!(out, "- skipped: {s}");
        let _ = writeln!(out);

        if d > 0 {
            let _ = writeln!(out, "## Diffs");
            let _ = writeln!(out);
            let _ = writeln!(out, "| test | expected lines | actual lines | first diff |");
            let _ = writeln!(out, "|------|---------------:|-------------:|-----------:|");
            for r in &self.results {
                if let CompareResult::Diff {
                    expected_lines,
                    actual_lines,
                    first_diff_line,
                    ..
                } = &r.result
                {
                    let _ = writeln!(
                        out,
                        "| {} | {} | {} | {} |",
                        r.name, expected_lines, actual_lines, first_diff_line
                    );
                }
            }
            let _ = writeln!(out);
        }

        if f > 0 {
            let _ = writeln!(out, "## Run failures");
            let _ = writeln!(out);
            for r in &self.results {
                if let CompareResult::RunFailed { reason } = &r.result {
                    let _ = writeln!(out, "- `{}` — {}", r.name, reason);
                }
            }
            let _ = writeln!(out);
        }

        out
    }
}

/// A minimal side-by-side-ish diff: expected lines with `-`, actual with `+`,
/// aligned naively by line index. Good enough for human inspection; not a real
/// LCS diff.
pub fn unified_like(expected: &str, actual: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "--- expected (flash output.txt)");
    let _ = writeln!(out, "+++ actual (flashplayerdebugger capture)");
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();
    let max = e.len().max(a.len());
    for i in 0..max {
        let el = e.get(i);
        let al = a.get(i);
        match (el, al) {
            (Some(x), Some(y)) if x == y => {
                let _ = writeln!(out, " {x}");
            }
            (Some(x), Some(y)) => {
                let _ = writeln!(out, "-{x}");
                let _ = writeln!(out, "+{y}");
            }
            (Some(x), None) => {
                let _ = writeln!(out, "-{x}");
            }
            (None, Some(y)) => {
                let _ = writeln!(out, "+{y}");
            }
            (None, None) => {}
        }
    }
    out
}
