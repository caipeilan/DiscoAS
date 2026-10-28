use super::{ConsoleError, Result};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize)]
pub struct LogLine {
    pub file: String,
    pub line: String,
}
fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ConsoleError::new("LOG_READ_FAILED", error.to_string())),
    };
    let mut result = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| ConsoleError::new("LOG_READ_FAILED", error.to_string()))?;
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && entry.path().extension().is_some_and(|ext| ext == "log")
        {
            result.push(entry.path());
        }
    }
    result.sort();
    Ok(result)
}
pub fn tail(root: &Path, limit: usize, category: Option<&str>) -> Result<Vec<LogLine>> {
    let mut result = Vec::new();
    for path in files(root)? {
        let file = File::open(&path)
            .map_err(|error| ConsoleError::new("LOG_READ_FAILED", error.to_string()))?;
        let mut lines = VecDeque::new();
        for line in BufReader::new(file).lines() {
            let line =
                line.map_err(|error| ConsoleError::new("LOG_READ_FAILED", error.to_string()))?;
            if category.is_some_and(|category| !line.contains(category)) {
                continue;
            }
            lines.push_back(line);
            while lines.len() > limit.min(1000) {
                lines.pop_front();
            }
        }
        result.extend(lines.into_iter().map(|line| LogLine {
            file: path.file_name().unwrap().to_string_lossy().into(),
            line,
        }));
    }
    Ok(result)
}
pub struct Cursor {
    offsets: HashMap<PathBuf, u64>,
}
impl Cursor {
    pub fn at_end(root: &Path) -> Result<Self> {
        let mut offsets = HashMap::new();
        for path in files(root)? {
            let size = std::fs::metadata(&path)
                .map_err(|error| ConsoleError::new("LOG_READ_FAILED", error.to_string()))?
                .len();
            offsets.insert(path, size);
        }
        Ok(Self { offsets })
    }
    pub fn read(&mut self, root: &Path, category: Option<&str>) -> Result<Vec<LogLine>> {
        let mut result = Vec::new();
        for path in files(root)? {
            let mut file = File::open(&path)
                .map_err(|error| ConsoleError::new("LOG_READ_FAILED", error.to_string()))?;
            let size = file
                .metadata()
                .map_err(|error| ConsoleError::new("LOG_READ_FAILED", error.to_string()))?
                .len();
            let offset = self.offsets.entry(path.clone()).or_default();
            if size < *offset {
                *offset = 0;
            }
            if size == *offset {
                continue;
            }
            file.seek(SeekFrom::Start(*offset))
                .map_err(|error| ConsoleError::new("LOG_READ_FAILED", error.to_string()))?;
            let mut reader = BufReader::new(file);
            let mut bytes = Vec::new();
            for _ in 0..256 {
                bytes.clear();
                let count = reader
                    .read_until(b'\n', &mut bytes)
                    .map_err(|error| ConsoleError::new("LOG_READ_FAILED", error.to_string()))?;
                if count == 0 || !bytes.ends_with(b"\n") {
                    break;
                }
                *offset += count as u64;
                let line = String::from_utf8_lossy(&bytes)
                    .trim_end_matches(['\r', '\n'])
                    .to_string();
                if category.is_none_or(|category| line.contains(category)) {
                    result.push(LogLine {
                        file: path.file_name().unwrap().to_string_lossy().into(),
                        line,
                    });
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn follow_keeps_incomplete_utf8_lines_and_detects_truncation() {
        let root = std::env::temp_dir().join(format!(
            "discoas-console-log-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("events.log");
        std::fs::write(&path, "before\n").unwrap();
        let mut cursor = Cursor::at_end(&root).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all("中".as_bytes()).unwrap();
        assert!(cursor.read(&root, None).unwrap().is_empty());
        file.write_all("文\nsecond\n".as_bytes()).unwrap();
        let rows = cursor.read(&root, None).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.line.as_str()).collect::<Vec<_>>(),
            vec!["中文", "second"]
        );
        assert!(cursor.read(&root, None).unwrap().is_empty());
        drop(file);
        std::fs::write(&path, "new\n").unwrap();
        assert_eq!(cursor.read(&root, None).unwrap()[0].line, "new");
        assert_eq!(tail(&root, 1, None).unwrap()[0].line, "new");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
