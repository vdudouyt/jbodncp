use ureq::Agent;
use anyhow::{ Result, Context, ensure };
use serde_json;
use crate::filelist::FileEntry;
use crate::jbod;
use std::path::PathBuf;
use log::*;

use std::fs::File;
use std::collections::VecDeque;
use std::sync::{ Arc, Mutex };
use std::thread::JoinHandle;

struct SharedState {
    counter: usize,
    queue: VecDeque<FileEntry>,
    downloaded: u64,
    errors: u64,
    files_seen: u64,
}

#[derive(Clone)]
struct WorkerSettings {
    endpoint: String,
    auth: String,
    dst_paths: Vec<String>,
    dry_run: bool,
}

enum DlStatus { NothingToDo, Completed }

pub fn run_client(url: &str, dst_paths: Vec<String>, auth: &str, threads: u16, dry_run: bool) -> Result<()> {
    info!("Fetching file list");
    let agent = ureq::agent();
    let list = agent
        .get(&format!("{url}/list"))
        .header("Authorization", &format!("Bearer {auth}"))
        .call()?.body_mut().with_config().limit(u64::MAX).read_to_string()?;
    let queue: VecDeque<FileEntry> = serde_json::from_str(&list)?;
    let files_matched = queue.len();
    let shared_state = Arc::new(Mutex::new(SharedState { counter: 0, queue, downloaded: 0, errors: 0, files_seen: 0 }));
    let worker_settings = WorkerSettings { endpoint: url.to_string(), auth: auth.to_string(), dst_paths, dry_run };
    let mut workers: VecDeque<JoinHandle<()>> = VecDeque::new();
    for _ in 0..threads {
        let shared_state = shared_state.clone();
        let worker_settings = worker_settings.clone();
        workers.push_back(std::thread::spawn(move || { worker(shared_state, &worker_settings); }));
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
    if dry_run {
        warn!("Dry run requested, so no downloads actually performed");
    }
    info!("Everything is done. Files seen: {} downloaded: {} errors: {}", state.files_seen, state.downloaded, state.errors);
    Ok(())
}

fn worker(state: Arc<Mutex<SharedState>>, settings: &WorkerSettings) {
    let next_item = || state.lock().unwrap().queue.pop_front();
    let next_path = || {
        let mut state = state.lock().unwrap();
        let idx = state.counter % settings.dst_paths.len();
        state.counter += 1;
        PathBuf::from(&settings.dst_paths[idx])
    };
    let agent = ureq::agent();
    while let Some(item) = next_item() {
        let download_url = format!("{}/download/{}", &settings.endpoint, item.relpath.display());
        let round_robin = || next_path().join(&item.relpath);
        let dst_path = jbod::find_file(&settings.dst_paths, &item.relpath).unwrap_or_else(round_robin);
        let result = download(&agent, &settings.auth, &download_url, &dst_path, item.size, settings.dry_run);
        if let Err(err) = &result {
            error!("File download failed: {} {:#}", dst_path.display(), err);
        }

        let mut state = state.lock().unwrap();
        state.files_seen += 1;
        match result {
            Ok(DlStatus::Completed) => state.downloaded+=1,
            Ok(DlStatus::NothingToDo) => {},
            Err(_) => state.errors+=1,
        }
    }
}

fn download(agent: &Agent, auth: &str, download_url: &str, dst_path: &PathBuf, expected_size: u64, dry_run: bool) -> Result<DlStatus> {
    let exists = std::fs::exists(&dst_path).unwrap_or(false);
    if exists && std::fs::metadata(&dst_path)?.len() == expected_size {
        info!("File already completed: {}", dst_path.display());
        return Ok(DlStatus::NothingToDo);
    }

    info!("Downloading URL: {} => {}", download_url, dst_path.display());

    if dry_run {
        return Ok(DlStatus::Completed);
    }

    if let Some(parent) = dst_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut response = agent.get(download_url)
        .header("Authorization", &format!("Bearer {auth}"))
        .call().context("HTTP Request failed")?;
    ensure!(response.status() == 200, "Wrong response status: {}", response.status());

    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(dst_path)?;
    std::io::copy(&mut reader, &mut file)?;

    let file_size = std::fs::metadata(&dst_path)?.len();
    ensure!(file_size == expected_size,  "Filesize check failed: {expected_size} bytes expected, {file_size} received");

    Ok(DlStatus::Completed)
}
