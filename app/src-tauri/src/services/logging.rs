//! Bounded diagnostic files containing categories only.
use std::path::Path;

static LOG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn log_event(root: &Path, context: &str, code: &str) {
    let Ok(_guard) = LOG_LOCK.lock() else {
        return;
    };
    let _ = append_log(root, context, code);
}

fn append_log(root: &Path, context: &str, code: &str) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(root)?;
    let path = root.join("errors.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() >= 256 * 1024) {
        let previous = root.join("errors.previous.log");
        if previous.exists() {
            std::fs::remove_file(&previous)?;
        }
        std::fs::rename(&path, previous)?;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Only event categories; never URLs, titles, credentials, paths or raw platform responses.
    let safe = |s: &str| {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .take(64)
            .collect::<String>()
    };
    writeln!(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?,
        "{stamp}\t{}\t{}",
        safe(context),
        safe(code)
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logging_rotates_and_uses_categories_only() {
        let root = std::env::temp_dir().join(format!("discoas-log-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("errors.log"), vec![0; 256 * 1024]).unwrap();
        append_log(&root, "import_playlist", "command_failed").unwrap();
        assert!(root.join("errors.previous.log").is_file());
        let line = std::fs::read_to_string(root.join("errors.log")).unwrap();
        assert!(line.contains("import_playlist\tcommand_failed"));
        assert!(line.len() < 128);
        std::fs::remove_file(root.join("errors.log")).unwrap();
        std::fs::remove_file(root.join("errors.previous.log")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
