//! Live streams of the game.
//!
//! Nobody streams video here. The game is flat shapes on a canvas, so
//! sending a picture of it would be paying for pixels to carry
//! information that is already numbers: a runner's height, where the
//! cars are, how far they have got. A viewer's browser runs the same
//! renderer and draws what it is told. Roughly forty bytes a frame
//! against a megabit for video, no media server, and nothing to
//! transcode.
//!
//! It also means the thing structurally cannot show anything nobody
//! drew. There is no camera and no microphone, so there is no
//! moderation surface — which is the only reason this can be open to
//! anybody with no account at all, the way the rest of the game is.
//!
//! Entirely in memory. A stream that ends leaves nothing behind: no
//! table, no recording, no row. That is right for something this
//! ephemeral and it means there is nothing to delete later.

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

/// How many frames a slow viewer may fall behind before it is dropped.
/// A second or so at sixty frames — long enough for a hiccup, short
/// enough that a stalled tab cannot hold memory open.
const LAG: usize = 64;

/// The biggest frame accepted. A frame is a runner, some cars and a
/// number; anything larger is not this game.
pub const MAX_FRAME: usize = 4096;

pub struct Live {
    pub initials: String,
    /// Read for the list without touching the broadcast channel.
    pub metres: AtomicI64,
    pub watching: AtomicUsize,
    pub frames: broadcast::Sender<String>,
}

#[derive(Clone, Default)]
pub struct Streams {
    inner: Arc<RwLock<HashMap<Uuid, Arc<Live>>>>,
}

#[derive(Serialize)]
pub struct StreamRow {
    pub id: Uuid,
    pub initials: String,
    pub metres: i64,
    pub watching: usize,
}

impl Streams {
    pub async fn open(&self, initials: String) -> (Uuid, Arc<Live>) {
        let (tx, _) = broadcast::channel(LAG);
        let live = Arc::new(Live {
            initials,
            metres: AtomicI64::new(0),
            watching: AtomicUsize::new(0),
            frames: tx,
        });
        let id = Uuid::new_v4();
        self.inner.write().await.insert(id, live.clone());
        (id, live)
    }

    pub async fn close(&self, id: Uuid) {
        self.inner.write().await.remove(&id);
    }

    pub async fn get(&self, id: Uuid) -> Option<Arc<Live>> {
        self.inner.read().await.get(&id).cloned()
    }

    /// Who is live, furthest first — the person worth watching is the
    /// one about to lose a good run.
    pub async fn list(&self) -> Vec<StreamRow> {
        let mut rows: Vec<StreamRow> = self
            .inner
            .read()
            .await
            .iter()
            .map(|(id, l)| StreamRow {
                id: *id,
                initials: l.initials.clone(),
                metres: l.metres.load(Ordering::Relaxed),
                watching: l.watching.load(Ordering::Relaxed),
            })
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.metres));
        rows.truncate(40);
        rows
    }
}
