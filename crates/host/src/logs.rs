use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tracing_appender::non_blocking::{NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::{EnvFilter, prelude::*};

const FILE_NAME: &str = "lucent.log";
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const BACKUPS: u32 = 3;
const QUEUED_RECORDS: usize = 1024;
const MAX_SCRIPT_LINE_BYTES: usize = 16 * 1024;

pub struct Logs {
    _writer: Option<WorkerGuard>,
}

pub fn init(debug: bool) -> Logs {
    let file = log_dir().and_then(|dir| RotatingFile::open(dir, MAX_FILE_BYTES));
    let (writer, guard) = match file {
        Ok(file) => {
            let (writer, guard) = NonBlockingBuilder::default()
                .buffered_lines_limit(QUEUED_RECORDS)
                .thread_name("lucent-logs")
                .finish(file);
            (Ok(writer), Some(guard))
        }
        Err(error) => (Err(error), None),
    };
    tracing_subscriber::registry()
        .with(filter(debug, std::env::var("RUST_LOG").ok()))
        .with(tracing_subscriber::fmt::layer().with_writer(io::stderr))
        .with(writer.as_ref().ok().cloned().map(json_layer))
        .init();
    if let Err(error) = writer {
        tracing::warn!(%error, "couldn't open the log file; logging to the terminal only");
    }
    Logs { _writer: guard }
}

fn log_dir() -> io::Result<PathBuf> {
    let root = crate::data::roots::create(&crate::data::roots::system_app_data_roots()?.current)?;
    let dir = root.join("logs");
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    match builder.create(&dir) {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => Err(error),
        _ => Ok(dir),
    }
}

fn filter(debug: bool, env: Option<String>) -> EnvFilter {
    env.and_then(|directives| EnvFilter::try_new(directives).ok())
        .unwrap_or_else(|| {
            EnvFilter::new(if debug {
                "info,lucent=debug,lucent_host=debug,lucent_ui=debug"
            } else {
                "info,lucent_host::scripting::script=debug"
            })
        })
}

fn json_layer<S, W>(writer: W) -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    W: for<'w> tracing_subscriber::fmt::MakeWriter<'w> + 'static,
{
    tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_writer(writer)
}

pub(crate) fn script_line(mut text: String) -> String {
    if text.len() <= MAX_SCRIPT_LINE_BYTES {
        return text;
    }
    let mut end = MAX_SCRIPT_LINE_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let cut = text.len() - end;
    text.truncate(end);
    text.push_str(&format!("… ({cut} more bytes)"));
    text
}

struct RotatingFile {
    dir: PathBuf,
    file: File,
    size: u64,
    limit: u64,
}

impl RotatingFile {
    fn open(dir: PathBuf, limit: u64) -> io::Result<Self> {
        let file = append(&dir.join(FILE_NAME))?;
        let size = file.metadata()?.len();
        Ok(Self {
            dir,
            file,
            size,
            limit,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        let backup = |n: u32| self.dir.join(format!("{FILE_NAME}.{n}"));
        for n in (1..BACKUPS).rev() {
            match std::fs::rename(backup(n), backup(n + 1)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        std::fs::rename(self.dir.join(FILE_NAME), backup(1))?;
        self.file = append(&self.dir.join(FILE_NAME))?;
        Ok(())
    }
}

fn append(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

impl Write for RotatingFile {
    fn write(&mut self, record: &[u8]) -> io::Result<usize> {
        if self.size > 0 && self.size + record.len() as u64 > self.limit {
            if let Err(error) = self.rotate() {
                eprintln!("couldn't rotate {FILE_NAME}: {error}");
            }
            self.size = 0;
        }
        self.file.write_all(record)?;
        self.size += record.len() as u64;
        Ok(record.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests;
