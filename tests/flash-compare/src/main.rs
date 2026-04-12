mod flashplayer;
mod report;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

use crate::flashplayer::FlashPlayer;
use crate::report::{CompareResult, Report, TestResult, unified_like};

#[derive(Parser, Debug)]
#[command(about = "Run every SWF test under flashplayerdebugger and diff against output.txt")]
struct Cli {
    /// Root directory containing test.toml files.
    #[arg(long, default_value = "tests/tests/swfs")]
    swfs_dir: PathBuf,

    /// Where to write report files.
    #[arg(long, default_value = "tests/flash-compare/report")]
    report_dir: PathBuf,

    /// Only run tests whose name contains this substring.
    #[arg(long)]
    filter: Option<String>,

    /// Cap the number of tests run (for quick smoke tests).
    #[arg(long)]
    limit: Option<usize>,

    /// Override the per-test kill timeout in milliseconds.
    /// If unset, derived from num_frames * tick_rate with a floor of 3000ms.
    #[arg(long)]
    timeout_ms: Option<u64>,

    /// Run flashplayerdebugger under `xvfb-run -a` so no real display is needed.
    #[arg(long)]
    headless: bool,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(default)]
struct RawTestOptions {
    num_frames: Option<u32>,
    num_ticks: Option<u32>,
    tick_rate: Option<f64>,
    output_path: Option<String>,
    subtests: Option<toml::value::Table>,
}

#[derive(Debug, Clone)]
struct DiscoveredTest {
    /// Human-readable name (relative path from swfs_dir, plus optional subtest).
    name: String,
    /// Directory containing test.toml / test.swf.
    dir: PathBuf,
    /// Path to test.swf.
    swf: PathBuf,
    /// Path to the expected output file.
    expected: PathBuf,
    /// Frames to run (num_frames wins, else num_ticks, else 1).
    frames: u32,
    /// Frame duration in ms (tick_rate), default 1000/30.
    tick_ms: f64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let swfs_dir = cli.swfs_dir.canonicalize().with_context(|| {
        format!("swfs_dir not found: {}", cli.swfs_dir.display())
    })?;

    let player = FlashPlayer::from_env(cli.headless)?;
    player.verify_mm_cfg()?;

    let tests = discover_tests(&swfs_dir)?;
    eprintln!("Discovered {} test configurations", tests.len());

    let filtered: Vec<_> = tests
        .into_iter()
        .filter(|t| match &cli.filter {
            Some(f) => t.name.contains(f),
            None => true,
        })
        .take(cli.limit.unwrap_or(usize::MAX))
        .collect();

    eprintln!("Running {} tests", filtered.len());

    // Prepare report dirs up front so we can stream diffs as they happen.
    std::fs::create_dir_all(&cli.report_dir)?;
    let diffs_dir = cli.report_dir.join("diffs");
    std::fs::create_dir_all(&diffs_dir)?;

    let mut results = Vec::with_capacity(filtered.len());
    let total = filtered.len();
    for (i, test) in filtered.into_iter().enumerate() {
        eprint!("[{}/{}] {} ... ", i + 1, total, test.name);
        let result = run_one(&player, &test, cli.timeout_ms);
        eprintln!("{}", result.short_label());

        // Write diff immediately so it's visible during long runs.
        if let CompareResult::Diff { expected, actual, .. } = &result {
            let safe = test.name.replace(['/', '#'], "_");
            let diff_path = diffs_dir.join(format!("{safe}.diff"));
            if let Err(e) = std::fs::write(&diff_path, unified_like(expected, actual)) {
                eprintln!("warn: could not write {}: {e}", diff_path.display());
            }
        }

        results.push(TestResult {
            name: test.name,
            dir: test.dir.to_string_lossy().into_owned(),
            result,
        });
    }

    let report = Report { results };
    report.write(&cli.report_dir)?;
    report.print_summary();
    Ok(())
}

fn discover_tests(swfs_dir: &Path) -> Result<Vec<DiscoveredTest>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(swfs_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() == "test.toml")
    {
        let toml_path = entry.path();
        let dir = toml_path.parent().unwrap().to_path_buf();
        let rel = dir.strip_prefix(swfs_dir).unwrap_or(&dir);
        let base_name = rel.to_string_lossy().replace('\\', "/");

        let swf = dir.join("test.swf");
        if !swf.exists() {
            continue;
        }

        let contents = match std::fs::read_to_string(toml_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warn: cannot read {}: {}", toml_path.display(), e);
                continue;
            }
        };
        let raw: RawTestOptions = match toml::from_str(&contents) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("warn: cannot parse {}: {}", toml_path.display(), e);
                continue;
            }
        };

        let default_frames = raw.num_frames.or(raw.num_ticks).unwrap_or(1);
        let default_tick = raw.tick_rate.unwrap_or(1000.0 / 30.0);
        let default_output = raw
            .output_path
            .clone()
            .unwrap_or_else(|| "output.txt".to_string());

        if let Some(subtests) = raw.subtests {
            for (sub_name, sub_val) in subtests {
                let sub: RawTestOptions = match sub_val.try_into() {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("warn: subtest {} in {}: {}", sub_name, toml_path.display(), e);
                        continue;
                    }
                };
                let frames = sub.num_frames.or(sub.num_ticks).unwrap_or(default_frames);
                let tick_ms = sub.tick_rate.unwrap_or(default_tick);
                let output = sub.output_path.unwrap_or_else(|| default_output.clone());
                out.push(DiscoveredTest {
                    name: format!("{base_name}#{sub_name}"),
                    dir: dir.clone(),
                    swf: swf.clone(),
                    expected: dir.join(&output),
                    frames,
                    tick_ms,
                });
            }
        } else {
            out.push(DiscoveredTest {
                name: base_name,
                dir: dir.clone(),
                swf,
                expected: dir.join(&default_output),
                frames: default_frames,
                tick_ms: default_tick,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn run_one(player: &FlashPlayer, test: &DiscoveredTest, override_ms: Option<u64>) -> CompareResult {
    // Skip unsupported tests.
    if test.dir.join("input.json").exists() {
        return CompareResult::Skipped {
            reason: "input.json (needs Ruffle input injection)".into(),
        };
    }
    if test.dir.join("socket.json").exists() {
        return CompareResult::Skipped {
            reason: "socket.json (needs mock socket server)".into(),
        };
    }
    if !test.expected.exists() {
        return CompareResult::Skipped {
            reason: format!("missing expected file {}", test.expected.display()),
        };
    }

    let expected = match std::fs::read_to_string(&test.expected) {
        Ok(s) => s.replace("\r\n", "\n").replace('\0', ""),
        Err(e) => {
            return CompareResult::RunFailed {
                reason: format!("read expected: {e}"),
            };
        }
    };

    let timeout = match override_ms {
        Some(ms) => Duration::from_millis(ms),
        None => derive_timeout(test.frames, test.tick_ms),
    };

    let started = Instant::now();
    let actual = match player.run(&test.swf, timeout) {
        Ok(s) => strip_player_noise(&s.replace("\r\n", "\n").replace('\0', "")),
        Err(e) => {
            return CompareResult::RunFailed {
                reason: format!("flashplayerdebugger: {e}"),
            };
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;

    if actual.trim().is_empty() {
        return CompareResult::NoFlashOutput { elapsed_ms };
    }

    if actual == expected {
        return CompareResult::Match { elapsed_ms };
    }

    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let first_diff_line = expected_lines
        .iter()
        .zip(actual_lines.iter())
        .position(|(a, b)| a != b)
        .unwrap_or(expected_lines.len().min(actual_lines.len()));

    CompareResult::Diff {
        expected_lines: expected_lines.len(),
        actual_lines: actual_lines.len(),
        first_diff_line,
        elapsed_ms,
        expected,
        actual,
    }
}

/// Drop lines that flashplayerdebugger emits itself (e.g. runtime warnings
/// about undefined member access) and which never appear in the committed
/// `output.txt`. These are not `trace()` output but confuse the line diff.
fn strip_player_noise(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for line in input.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("Warning:") {
            continue;
        }
        out.push_str(line);
    }
    out
}

fn derive_timeout(frames: u32, tick_ms: f64) -> Duration {
    let base_ms = (frames as f64) * tick_ms;
    // 3s floor, 2x buffer + 2s startup, 60s ceiling.
    let total_ms = (base_ms * 2.0 + 2000.0).clamp(3000.0, 60_000.0);
    Duration::from_millis(total_ms as u64)
}
