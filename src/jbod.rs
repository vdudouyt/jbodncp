use std::path::PathBuf;
use std::collections::HashMap;
use crate::filelist::{ list_files_bfs, FileEntry };

pub fn list_files(mount_points: &Vec<String>) -> Vec<FileEntry> {
    let mut file_sizes: HashMap<PathBuf, u64> = HashMap::new();
    let mut file_paths: Vec<PathBuf> = vec![];
    for path in mount_points {
        for item in list_files_bfs(std::path::Path::new(path)).unwrap() {
            if !file_sizes.contains_key(&item.relpath) {
                file_paths.push(item.relpath.clone());
                file_sizes.insert(item.relpath.clone(), item.size);
            }
            if file_sizes[&item.relpath] < item.size {
                file_sizes.insert(item.relpath, item.size);
            }
        }
    }

    file_paths.into_iter().map(|relpath| {
        let size = file_sizes[&relpath];
        FileEntry { relpath, size }
    }).collect()
}

pub fn find_file(mount_points: &Vec<String>, rel_path: &PathBuf) -> Option<PathBuf> {
    let file_exists = |path: &PathBuf| std::fs::exists(&path).unwrap_or(false);
    let mut candidates: Vec<_> = mount_points.into_iter().map(|path| PathBuf::from(path).join(rel_path)).filter(file_exists).collect();
    candidates.sort_by_key(|path| std::fs::metadata(&path).unwrap().len());
    candidates.pop()
}

#[cfg(test)]
mod tests {
    use tempfile::{ tempdir, TempDir };
    use anyhow::Result;
    use super::*;

    #[allow(dead_code)]
    struct Fixture {
        tempdir: TempDir,
        mount_point1: PathBuf,
        mount_point2: PathBuf,
        mount_points: Vec<String>,
    }

    impl Fixture {
        fn create() -> Result<Fixture> {
            let tempdir = tempdir()?;
            let tempdir_path = tempdir.path();
            let mount_point1 = tempdir_path.join("1");
            let mount_point2 = tempdir_path.join("2");
            std::fs::create_dir_all(mount_point1.join("somedir"))?;
            std::fs::create_dir_all(mount_point2.join("somedir"))?;
            std::fs::write(mount_point1.join("somedir/file.bin"), b"oneone")?;
            std::fs::write(mount_point2.join("somedir/file.bin"), b"oneoneone")?;
            std::fs::write(mount_point1.join("somedir/file2.bin"), b"twotwotwo")?;
            std::fs::write(mount_point2.join("somedir/file2.bin"), b"twotwo")?;
            let mount_points = vec![mount_point1.to_str().unwrap().to_owned(), mount_point2.to_str().unwrap().to_owned()];
            Ok(Fixture { tempdir, mount_point1, mount_point2, mount_points })
        }
    }

    #[test]
    fn test_list_files() {
        let f = Fixture::create().unwrap();
        let mut res = list_files(&f.mount_points);
        res.sort_by_key(|x| x.relpath.clone());

        assert_eq!(res, vec![
            FileEntry { relpath: "somedir/file.bin".into(), size: 9 },
            FileEntry { relpath: "somedir/file2.bin".into(), size: 9 },
        ]);
    }

    #[test]
    fn test_find_file() {
        let f = Fixture::create().unwrap();
        assert_eq!(find_file(&f.mount_points, &PathBuf::from("somedir/file.bin")), Some(f.mount_point2.join("somedir/file.bin")));
        assert_eq!(find_file(&f.mount_points, &PathBuf::from("somedir/file2.bin")), Some(f.mount_point1.join("somedir/file2.bin")));
        assert_eq!(find_file(&f.mount_points, &PathBuf::from("somedir/file.txt")), None);
    }
}
