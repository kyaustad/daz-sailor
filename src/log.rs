use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub struct Logger {
    pub verbose: bool,
}

impl Logger {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    pub fn banner(&self, title: &str) {
        let line = "=".repeat(title.len() + 4);
        self.println(&line);
        self.println(&format!("  {title}  "));
        self.println(&line);
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.log("INFO", message.as_ref());
    }

    pub fn step(&self, message: impl AsRef<str>) {
        self.log("....", message.as_ref());
    }

    pub fn success(&self, message: impl AsRef<str>) {
        self.log(" OK ", message.as_ref());
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        self.log("WARN", message.as_ref());
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.log("FAIL", message.as_ref());
    }

    pub fn verbose(&self, message: impl AsRef<str>) {
        if self.verbose {
            self.log("    ", message.as_ref());
        }
    }

    pub fn progress(&self, current: usize, total: usize, message: impl AsRef<str>) {
        let pct = if total == 0 {
            0
        } else {
            (current * 100) / total
        };
        self.log(
            "PROG",
            &format!("[{current}/{total} {pct:>3}%] {}", message.as_ref()),
        );
    }

    pub fn config_line(&self, label: &str, path: &std::path::Path) {
        self.info(&format!("{label}: {}", path.display()));
    }

    fn log(&self, level: &str, message: &str) {
        let _guard = LOG_LOCK.lock().unwrap();
        let ts = timestamp();
        println!("[{ts}] [{level}] {message}");
        let _ = std::io::stdout().flush();
    }

    fn println(&self, message: &str) {
        let _guard = LOG_LOCK.lock().unwrap();
        println!("{message}");
        let _ = std::io::stdout().flush();
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
