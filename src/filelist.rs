use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::collections::VecDeque;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileEntry {
    pub relpath: PathBuf,
    pub size: u64,
}

pub fn list_files_bfs(base: &Path) -> io::Result<Vec<FileEntry>> {
    let mut results = Vec::new();
    let mut queue = VecDeque::new();

    let base = fs::canonicalize(base)?;
    queue.push_back(base.clone());

    while let Some(current_dir) = queue.pop_front() {
        for entry in fs::read_dir(&current_dir)? {
            let path = entry?.path();

            if path.is_dir() {
                queue.push_back(path);
            } else if path.is_file() {
                let size = fs::metadata(&path)?.len();
                let relpath = path.strip_prefix(&base).unwrap_or(&path).to_path_buf();
                results.push(FileEntry { relpath, size });
            }
        }
    }

    Ok(results)
}
