use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Compress,
    Download,
    Slideshow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: u64,
    pub kind: JobKind,
    pub label: String,
    pub detail: String,
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub status: JobStatus,
    pub progress: f64,
    pub message: String,
    pub before: u64,
    pub after: u64,
}

impl Job {
    pub fn new(id: u64, kind: JobKind, label: String) -> Self {
        Self {
            id,
            kind,
            label,
            detail: String::new(),
            input: None,
            output: None,
            status: JobStatus::Queued,
            progress: 0.0,
            message: "Queued".to_string(),
            before: 0,
            after: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct JobStore {
    next_id: u64,
    jobs: VecDeque<Job>,
}

impl JobStore {
    pub fn add(&mut self, kind: JobKind, label: String) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.jobs.push_front(Job::new(id, kind, label));
        id
    }

    pub fn update(&mut self, id: u64, f: impl FnOnce(&mut Job)) {
        if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
            f(j);
        }
    }

    pub fn snapshot(&self) -> Vec<Job> {
        self.jobs.iter().cloned().collect()
    }

    pub fn clear_finished(&mut self) {
        self.jobs.retain(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running));
    }
}

pub type SharedStore = Arc<Mutex<JobStore>>;
pub fn new_store() -> SharedStore {
    Arc::new(Mutex::new(JobStore::default()))
}
