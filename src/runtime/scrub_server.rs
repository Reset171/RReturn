use super::super::Game;
use super::capture::capture_timeline_snapshot_in_memory;
use super::snapshots::TimelineSnapshot;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

pub struct ServerScrubJournal {
    pub snapshots: VecDeque<Arc<TimelineSnapshot>>,
    pub max_snapshots: usize,
}

impl Default for ServerScrubJournal {
    fn default() -> Self {
        Self {
            snapshots: VecDeque::with_capacity(600),
            max_snapshots: 600,
        }
    }
}

static SERVER_JOURNAL: OnceLock<Mutex<ServerScrubJournal>> = OnceLock::new();

fn server_journal() -> &'static Mutex<ServerScrubJournal> {
    SERVER_JOURNAL.get_or_init(|| Mutex::new(ServerScrubJournal::default()))
}

pub fn record_server_tick(game: &Game<'_>) {
    let snap = capture_timeline_snapshot_in_memory(game);
    if let Ok(mut j) = server_journal().lock() {
        if j.snapshots.len() >= j.max_snapshots {
            j.snapshots.pop_front();
        }
        j.snapshots.push_back(snap);
    }
}

pub fn get_server_snapshot_for_tick(target_tick: i64) -> Option<Arc<TimelineSnapshot>> {
    let j = server_journal().lock().ok()?;
    if j.snapshots.is_empty() {
        return None;
    }
    let mut best: Option<&Arc<TimelineSnapshot>> = None;
    for snap in j.snapshots.iter().rev() {
        if snap.game_time <= target_tick {
            best = Some(snap);
            break;
        }
    }
    best.or_else(|| j.snapshots.front()).cloned()
}

pub fn server_scrub_time_range() -> Option<(i64, i64)> {
    let j = server_journal().lock().ok()?;
    let first = j.snapshots.front()?.game_time;
    let last = j.snapshots.back()?.game_time;
    Some((first, last))
}

pub fn clear_server_scrub() {
    if let Ok(mut j) = server_journal().lock() {
        j.snapshots.clear();
    }
}
