use anyhow::{ Result, Context, ensure };
use serde_json;
use crate::filelist::FileEntry;
use crate::jbod;
use crate::cli::DownloadConfig;
use crate::disk_space::get_available_space;
use std::path::PathBuf;
use log::*;
use glob::glob;
use regex::Regex;
use rand::Rng;

use std::fs::File;
use std::collections::VecDeque;
use std::sync::{ Arc, Mutex };
use std::thread::JoinHandle;
use std::collections::HashMap;

struct SharedState {
    queue: VecDeque<FileEntry>,
    downloaded: u64,
    errors: u64,
    files_seen: u64,

    // group by
    index: HashMap<String, PathBuf>,
}

#[derive(Clone)]
struct WorkerSettings {
    endpoint: String,
    auth: String,
    dst_paths: Vec<String>,
    dry_run: bool,
    group_by: Option<Regex>,

    // group by (preload)
    index_preload: HashMap<String, PathBuf>,
}

enum DlStatus { NothingToDo, Completed }

pub fn run_client(args: DownloadConfig) -> Result<()> {
    info!("Fetching file list");
    let agent = ureq::agent();
    let list = agent
        .get(&format!("{}/list", args.url))
        .header("Authorization", &format!("Bearer {}", args.auth))
        .call()?.body_mut().with_config().limit(u64::MAX).read_to_string()?;
    let queue: VecDeque<FileEntry> = serde_json::from_str(&list)?;
    let files_matched = queue.len();

    let group_by = args.group_by.as_deref().map(Regex::new).transpose().context("regex compilation")?;
    let index = if let Some(regex) = &group_by {
        info!("Building directory index (--group-by)");
        jbod::index_by_regex(&args.dst_paths, regex)
    } else {
        HashMap::new()
    };

    let index_preload = if let Some(wildcard) = &args.group_by_preload {
        info!("Building directory index (--group-by-preload)");
        let dirs: Vec<PathBuf> = glob(wildcard)?.filter_map(Result::ok).collect();
        ensure!(dirs.len() == args.dst_paths.len(), "Glob passed into --group-by-preload doesn't matches specified dst_dir count");

        let dirs_str: Vec<String> = dirs.iter().map(|t| t.display().to_string()).collect();
        let dst_paths: Vec<_> = args.dst_paths.iter().map(PathBuf::from).collect();
        let lookup: HashMap<PathBuf, PathBuf> = dirs.into_iter().zip(dst_paths).collect();
        let index = jbod::index_by_regex(&dirs_str, &group_by.clone().unwrap());
        let index_len = index.len();
        let translated: HashMap<String, PathBuf> = index.into_iter().filter_map(|(k, v)| lookup.get(&v).cloned().map(|new_v| (k, new_v))).collect();
        ensure!(translated.len() == index_len, "Translation failed");
        translated
    } else {
        HashMap::new()
    };

    let shared_state = Arc::new(Mutex::new(SharedState { queue, downloaded: 0, errors: 0, files_seen: 0, index }));
    let worker_settings = WorkerSettings { endpoint: args.url.to_string(), auth: args.auth.to_string(), dst_paths: args.dst_paths, dry_run: args.dry_run, group_by: group_by, index_preload };

    let mut workers: VecDeque<JoinHandle<()>> = VecDeque::new();
    for _ in 0..args.threads {
        let shared_state = shared_state.clone();
        let worker_settings = worker_settings.clone();
        workers.push_back(std::thread::spawn(move || {
            Worker::new(shared_state, worker_settings).run();
        }));
    }
    while let Some(thread) = workers.pop_front() {
        thread.join().unwrap();
    }

    let state = shared_state.lock().unwrap();
    if state.files_seen != files_matched as u64 {
        warn!("Some files were ignored. Files seen: {} matched: {}", state.files_seen, files_matched);
    }
    if state.errors > 0 {
        warn!("Some transfers were completed with errors");
    }
    if args.dry_run {
        warn!("Dry run requested, so no downloads actually performed");
    }
    info!("Everything is done. Files seen: {} downloaded: {} errors: {}", state.files_seen, state.downloaded, state.errors);
    Ok(())
}

struct Worker {
    state: Arc<Mutex<SharedState>>,
    settings: WorkerSettings,
    agent: ureq::Agent,
}

type AbsPath = PathBuf;

impl Worker {
    fn new(state: Arc<Mutex<SharedState>>, settings: WorkerSettings) -> Worker {
        Worker { state, settings, agent: ureq::agent() }
    }
    fn run(&mut self) {
       while let Some(item) = self.next_item() {
           let download_url = format!("{}/download/{}", &self.settings.endpoint, item.relpath.display());
           let Some(dst_path) = self.dst_file_path(&item) else {
               error!("No available disks left");
               return;
           };

           let result = self.download(&download_url, &dst_path, item.size);
           if let Err(err) = &result {
               error!("File download failed: {} {:#}", dst_path.display(), err);
           }

           let mut state = self.state.lock().unwrap();
           state.files_seen += 1;
           match result {
               Ok(DlStatus::Completed) => state.downloaded+=1,
               Ok(DlStatus::NothingToDo) => {},
               Err(_) => state.errors+=1,
           }
       }
    }
    fn download(&self, download_url: &str, dst_path: &PathBuf, expected_size: u64) -> Result<DlStatus> {
        let exists = std::fs::exists(&dst_path).unwrap_or(false);
        if exists && std::fs::metadata(&dst_path)?.len() == expected_size {
            info!("File already completed: {}", dst_path.display());
            return Ok(DlStatus::NothingToDo);
        }

        info!("Downloading URL: {} => {}", download_url, dst_path.display());

        if self.settings.dry_run {
            return Ok(DlStatus::Completed);
        }

        if let Some(parent) = dst_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut response = self.agent.get(download_url)
            .header("Authorization", &format!("Bearer {}", self.settings.auth))
            .call().context("HTTP Request failed")?;
        ensure!(response.status() == 200, "Wrong response status: {}", response.status());

        let mut reader = response.body_mut().as_reader();
        let mut file = File::create(dst_path)?;
        std::io::copy(&mut reader, &mut file)?;

        let file_size = std::fs::metadata(&dst_path)?.len();
        ensure!(file_size == expected_size,  "Filesize check failed: {expected_size} bytes expected, {file_size} received");

        Ok(DlStatus::Completed)
    }
    fn dst_file_path(&mut self, item: &FileEntry) -> Option<AbsPath> {
        // File already exists in one of partitions, so just return it's absolute path
        if let Some(abs_path) = jbod::find_file(&self.settings.dst_paths, &item.relpath) {
            return Some(abs_path);
        }

        // --group-by and --group-by-preload specified
        let group_key: Option<String> = self.settings.group_by.as_ref().and_then(|regex| Self::make_group_key(regex, &item.relpath));
        if let Some(group_key) = &group_key {
            if let Some(base) = self.settings.index_preload.get(group_key) {
                return Some(base.join(&item.relpath));
            }
        }

        let disk_spaces: Vec<_> = self.settings.dst_paths.iter()
            .map(|path| (path, get_available_space(&PathBuf::from(path)).unwrap_or(0)))
            .filter(|(_path, disk_space)| *disk_space > item.size)
            .collect();

        let mut state = self.state.lock().unwrap();

        // If --group-by is specified and this relpath is already indexed, use the same partition
        if let Some(group_key) = &group_key {
            if let Some(base) = state.index.get(group_key) {
                return Some(base.join(&item.relpath));
            }
        }

        // Roll dice if nothing above worked
        let dst_path = roll_weighed_dice(&disk_spaces)?;
        let dst_path = PathBuf::from(dst_path);

        if let Some(group_key) = group_key {
            state.index.entry(group_key).or_insert_with(|| dst_path.clone());
        }

        Some(dst_path.join(&item.relpath))
    }
    fn make_group_key(regex: &Regex, relpath: &PathBuf) -> Option<String> {
        let filename = relpath.file_name().unwrap().to_string_lossy();
        let captures = regex.captures(&filename)?;
        let key: &str = &captures[if captures.len() > 1 { 1 } else { 0 }];
        Some(key.into())
    }
    fn next_item(&mut self) -> Option<FileEntry> {
        self.state.lock().unwrap().queue.pop_front()
    }
}

fn roll_weighed_dice<'a>(input: &'a Vec<(&'a String, u64)>) -> Option<&'a str> {
    let total_space: u64 = input.iter().map(|(_, space)| space).sum();
    let mut rng = rand::rng();
    let mut choice = rng.random_range(0..total_space);
    for (mount, space) in input {
        if choice < *space {
            return Some(mount.as_ref());
        }
        choice -= space;
    }
    Some(input.first()?.0)
}
