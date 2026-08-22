//! Runs happening right now, and the people watching them.
//!
//! The same shape as `domain::whyte::streams` and for the same reasons,
//! but carrying a person down a real street instead of a drawing down a
//! fake one.
//!
//! Entirely in memory. **Nothing about a run is ever written down.** A
//! run that ends leaves no row, no trail and no history — the moment the
//! socket closes the registry forgets it happened. That is not a corner
//! cut for v1; it is the difference between witnessing somebody and
//! tracking them. A stored trail of where a member has been is precisely
//! the object this platform spends every other page arguing against, and
//! there is no table here to grow into one.
//!
//! What is public while a run is live: a member number, a position, and
//! how far they have gone. A member is a number rather than a name, so
//! what is broadcast is a body moving through a street and not an
//! identity — and it is broadcast because somebody pressed start.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicI64, AtomicUsize, Ordering},
        Arc,
    },
};

use serde::Serialize;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// How many positions a slow watcher may fall behind before it is
/// dropped. Positions arrive every second or so rather than sixty times
/// a second, so this is a generous minute of hiccup.
const LAG: usize = 64;

/// The biggest frame accepted. A frame is two coordinates and a
/// distance; anything larger is not a run.
pub const MAX_FRAME: usize = 512;

pub struct Run {
    /// Who may push positions into it. Checked on every one — a run id
    /// is handed to a watcher in the public list, so it is a name and
    /// not a permission.
    pub owner: Uuid,
    /// The byline. Nothing public about a member is chosen by them.
    pub member_no: i32,
    /// Read for the list without touching the broadcast channel.
    pub metres: AtomicI64,
    pub watching: AtomicUsize,
    pub positions: broadcast::Sender<String>,
}

#[derive(Clone, Default)]
pub struct Runs {
    inner: Arc<RwLock<HashMap<Uuid, Arc<Run>>>>,
}

#[derive(Serialize)]
pub struct RunRow {
    pub id: Uuid,
    pub member_no: i32,
    pub metres: i64,
    pub watching: usize,
}

impl Runs {
    pub async fn open(&self, owner: Uuid, member_no: i32) -> (Uuid, Arc<Run>) {
        let (tx, _) = broadcast::channel(LAG);
        let run = Arc::new(Run {
            owner,
            member_no,
            metres: AtomicI64::new(0),
            watching: AtomicUsize::new(0),
            positions: tx,
        });
        let id = Uuid::new_v4();
        self.inner.write().await.insert(id, run.clone());
        (id, run)
    }

    pub async fn close(&self, id: Uuid) {
        self.inner.write().await.remove(&id);
    }

    pub async fn get(&self, id: Uuid) -> Option<Arc<Run>> {
        self.inner.read().await.get(&id).cloned()
    }

    /// One run per member. Starting a second one ends the first, because
    /// a person is in one place at a time and two live runs under the
    /// same number would be a lie about where somebody is.
    pub async fn close_for_owner(&self, owner: Uuid) {
        self.inner.write().await.retain(|_, r| r.owner != owner);
    }

    /// Who is out there now, furthest first.
    pub async fn list(&self) -> Vec<RunRow> {
        let mut rows: Vec<RunRow> = self
            .inner
            .read()
            .await
            .iter()
            .map(|(id, r)| RunRow {
                id: *id,
                member_no: r.member_no,
                metres: r.metres.load(Ordering::Relaxed),
                watching: r.watching.load(Ordering::Relaxed),
            })
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.metres));
        rows.truncate(40);
        rows
    }
}
