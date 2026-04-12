use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub struct FlashPlayer {
    binary: PathBuf,
    home: PathBuf,
    headless: bool,
}

impl FlashPlayer {
    pub fn from_env(headless: bool) -> Result<Self> {
        let binary = std::env::var_os("FLASHPLAYER_DEBUGGER")
            .map(PathBuf::from)
            .context(
                "FLASHPLAYER_DEBUGGER is not set. \
                 Export it to the path of your flashplayerdebugger binary.",
            )?;
        if !binary.exists() {
            bail!(
                "FLASHPLAYER_DEBUGGER={} does not exist",
                binary.display()
            );
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set")?;
        if headless && which("xvfb-run").is_none() {
            bail!(
                "--headless requires `xvfb-run` on PATH. \
                 Install xvfb (e.g. `apt install xvfb`)."
            );
        }
        Ok(Self {
            binary,
            home,
            headless,
        })
    }

    fn mm_cfg_path(&self) -> PathBuf {
        self.home.join("mm.cfg")
    }

    fn flashlog_path(&self) -> PathBuf {
        self.home
            .join(".macromedia/Flash_Player/Logs/flashlog.txt")
    }

    pub fn verify_mm_cfg(&self) -> Result<()> {
        let path = self.mm_cfg_path();
        let contents = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "Cannot read {}. Create it with:\n  \
                 ErrorReportingEnable=1\n  \
                 TraceOutputFileEnable=1\n  \
                 MaxWarnings=0",
                path.display()
            )
        })?;
        let has = |k: &str| {
            contents
                .lines()
                .any(|l| l.trim().eq_ignore_ascii_case(&format!("{k}=1")))
        };
        let missing: Vec<&str> = ["ErrorReportingEnable", "TraceOutputFileEnable"]
            .into_iter()
            .filter(|k| !has(k))
            .collect();
        if !missing.is_empty() {
            bail!(
                "{} is missing required keys: {}. \
                 Each must be set to 1.",
                path.display(),
                missing.join(", ")
            );
        }

        // Ensure the log directory exists so flashplayerdebugger can write into it.
        if let Some(parent) = self.flashlog_path().parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("creating flashlog directory {}", parent.display())
            })?;
        }
        Ok(())
    }

    /// Run a single SWF and return the captured trace output.
    pub fn run(&self, swf: &Path, timeout: Duration) -> Result<String> {
        let log = self.flashlog_path();

        // Truncate any stale log before the run.
        let _ = std::fs::write(&log, b"");

        let abs_swf = swf
            .canonicalize()
            .with_context(|| format!("canonicalize {}", swf.display()))?;

        let mut cmd = if self.headless {
            let mut c = Command::new("xvfb-run");
            c.arg("-a").arg(&self.binary).arg(&abs_swf);
            c
        } else {
            let mut c = Command::new(&self.binary);
            c.arg(&abs_swf);
            c
        };
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawn {}", self.binary.display()))?;

        let started = Instant::now();
        let poll = Duration::from_millis(50);
        loop {
            match child.try_wait()? {
                Some(_) => break,
                None => {
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    thread::sleep(poll);
                }
            }
        }

        // Give flashplayerdebugger a beat to flush the log to disk.
        thread::sleep(Duration::from_millis(50));
        let trace = std::fs::read_to_string(&log).unwrap_or_default();
        Ok(trace)
    }
}

fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(cmd);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
