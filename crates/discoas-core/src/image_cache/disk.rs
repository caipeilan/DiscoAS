use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::SystemTime,
};

const MARKER: &str = ".discoas-cache";
const SIGNATURE: &[u8] = b"DiscoAS cover cache v1\n";

struct State {
    entries: HashMap<PathBuf, (u64, SystemTime)>,
    bytes: u64,
    epoch: u64,
    active: bool,
}

/// A regenerable, size-bounded image store inside an explicitly owned child directory.
pub struct CoverDiskCache {
    directory: PathBuf,
    limit: AtomicU64,
    state: Mutex<State>,
}

impl CoverDiskCache {
    pub fn open(directory: &Path, limit: u64) -> Result<Self, String> {
        if !directory.is_absolute() {
            return Err("缓存地址必须为绝对路径".into());
        }
        let directory = directory.join("covers-v1");
        std::fs::create_dir_all(&directory).map_err(|_| "缓存地址无法写入")?;
        let marker = directory.join(MARKER);
        if marker.exists() {
            if std::fs::read(&marker).map_err(|_| "缓存地址无法读取")? != SIGNATURE {
                return Err("缓存地址已被其他文件占用".into());
            }
        } else {
            if std::fs::read_dir(&directory)
                .map_err(|_| "缓存地址无法读取")?
                .next()
                .is_some()
            {
                return Err("缓存地址已被其他文件占用".into());
            }
            crate::storage::atomic_write(&marker, SIGNATURE).map_err(|_| "缓存地址无法写入")?;
        }
        let directory = directory.canonicalize().map_err(|_| "缓存地址无法读取")?;
        let mut state = State {
            entries: HashMap::new(),
            bytes: 0,
            epoch: 0,
            active: true,
        };
        for entry in std::fs::read_dir(&directory).map_err(|_| "缓存地址无法读取")? {
            let entry = entry.map_err(|_| "缓存地址无法读取")?;
            if !owned_name(&entry.file_name().to_string_lossy())
                || !entry.file_type().map_err(|_| "缓存地址无法读取")?.is_file()
            {
                continue;
            }
            let metadata = entry.metadata().map_err(|_| "缓存地址无法读取")?;
            state.bytes += metadata.len();
            state.entries.insert(
                entry.path(),
                (
                    metadata.len(),
                    metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                ),
            );
        }
        let cache = Self {
            directory,
            limit: AtomicU64::new(limit),
            state: Mutex::new(state),
        };
        cache.prune(&mut cache.state.lock().unwrap())?;
        Ok(cache)
    }

    pub fn epoch(&self) -> u64 {
        self.state.lock().unwrap().epoch
    }
    pub fn directory(&self) -> &Path {
        self.directory.parent().unwrap()
    }
    pub fn set_limit(&self, limit: u64) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        self.limit.store(limit, Ordering::Release);
        self.prune(&mut state)
    }
    pub fn deactivate(&self) {
        let mut state = self.state.lock().unwrap();
        state.active = false;
        state.epoch += 1;
    }
    pub fn bytes(&self) -> u64 {
        self.state.lock().unwrap().bytes
    }

    pub fn read(&self, key: &str) -> Option<Vec<u8>> {
        let mut state = self.state.lock().unwrap();
        if !state.active || self.limit.load(Ordering::Acquire) == 0 {
            return None;
        }
        let path = super::disk_path(&self.directory, key);
        let bytes = std::fs::read(&path).ok()?;
        super::image_mime(&bytes)?;
        if let Some((_, used)) = state.entries.get_mut(&path) {
            *used = SystemTime::now();
        }
        Some(bytes)
    }

    pub fn write(&self, key: &str, bytes: &[u8], epoch: u64) -> Result<(), String> {
        super::validate_cover(bytes)?;
        let mut state = self.state.lock().unwrap();
        if !state.active
            || epoch != state.epoch
            || bytes.len() as u64 > self.limit.load(Ordering::Acquire)
        {
            return Ok(());
        }
        let path = super::disk_path(&self.directory, key);
        crate::storage::atomic_write(&path, bytes).map_err(|_| "缓存写入失败")?;
        if let Some((old, _)) = state
            .entries
            .insert(path, (bytes.len() as u64, SystemTime::now()))
        {
            state.bytes -= old;
        }
        state.bytes += bytes.len() as u64;
        self.prune(&mut state)
    }

    fn prune(&self, state: &mut State) -> Result<(), String> {
        while state.bytes > self.limit.load(Ordering::Acquire) {
            let Some(path) = state
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| used)
                .map(|(p, _)| p.clone())
            else {
                break;
            };
            self.remove(state, &path)?;
        }
        Ok(())
    }

    fn remove(&self, state: &mut State, path: &Path) -> Result<(), String> {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("缓存清理失败".into()),
        }
        if let Some((bytes, _)) = state.entries.remove(path) {
            state.bytes -= bytes;
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.epoch += 1;
        for path in state.entries.keys().cloned().collect::<Vec<_>>() {
            self.remove(&mut state, &path)?;
        }
        Ok(())
    }
}

fn owned_name(name: &str) -> bool {
    name.strip_suffix(".cover")
        .is_some_and(|hash| hash.len() == 32 && hash.bytes().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistence_budget_clear_and_late_writes_preserve_unrelated_files() {
        let root =
            std::env::temp_dir().join(format!("discoas-disk-cache-{}", rand::random::<u64>()));
        let image = b"\xff\xd8\xffimage";
        let cache = CoverDiskCache::open(&root, image.len() as u64 * 2).unwrap();
        std::fs::write(root.join("keep.txt"), b"keep").unwrap();
        std::fs::write(root.join("covers-v1/keep.txt"), b"keep").unwrap();
        cache.write("a", image, cache.epoch()).unwrap();
        cache.write("b", image, cache.epoch()).unwrap();
        assert_eq!(cache.read("a").unwrap(), image);
        cache.write("c", image, cache.epoch()).unwrap();
        assert!(cache.read("b").is_none());
        let reopened = CoverDiskCache::open(&root, 100).unwrap();
        assert_eq!(reopened.read("a").unwrap(), image);
        let old = reopened.epoch();
        reopened.clear().unwrap();
        reopened.write("late", image, old).unwrap();
        assert_eq!(reopened.bytes(), 0);
        assert!(reopened.read("late").is_none());
        assert!(root.join("keep.txt").is_file());
        assert!(root.join("covers-v1/keep.txt").is_file());
        reopened.deactivate();
        reopened.write("closed", image, reopened.epoch()).unwrap();
        assert_eq!(reopened.bytes(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn occupied_directory_and_invalid_images_are_rejected() {
        let root =
            std::env::temp_dir().join(format!("discoas-disk-cache-{}", rand::random::<u64>()));
        std::fs::create_dir_all(root.join("covers-v1")).unwrap();
        std::fs::write(root.join("covers-v1/keep.txt"), b"keep").unwrap();
        assert!(CoverDiskCache::open(&root, 100).is_err());
        assert!(root.join("covers-v1/keep.txt").exists());
        std::fs::remove_file(root.join("covers-v1/keep.txt")).unwrap();
        let cache = CoverDiskCache::open(&root, 0).unwrap();
        assert!(cache.write("html", b"<html>", cache.epoch()).is_err());
        cache
            .write("off", b"\xff\xd8\xffimage", cache.epoch())
            .unwrap();
        assert_eq!(cache.bytes(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }
}
