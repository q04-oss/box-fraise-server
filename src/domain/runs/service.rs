//! Starting, feeding and ending a live run.
//!
//! Nothing here writes to the database, and there is no repository
//! module because there is no table. The only read is who the member is;
//! everything else lives in `live::Runs` until the run ends and then
//! stops existing. See that file for why.

use uuid::Uuid;

use super::{live::Runs, types::Started};
use crate::{
    db::{Pool, RlsTransaction},
    error::{AppError, AppResult},
};

pub async fn start(pool: &Pool, runs: &Runs, user_id: Uuid) -> AppResult<Started> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let row: Option<(i32, bool)> = sqlx::query_as(
        "SELECT member_no, bf_member_is_current(id)
           FROM users
          WHERE id = $1 AND member_no IS NOT NULL",
    )
    .bind(user_id)
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;

    let Some((member_no, current)) = row else {
        return Err(AppError::Unauthorized);
    };
    // Broadcasting is a kind of posting, and posting lapses when
    // somebody stops turning up. The same rule as submissions and the
    // inbox — a membership is kept, not got.
    if !current {
        return Err(AppError::bad_request(
            "your membership has lapsed — come for a run and it comes back",
        ));
    }

    // One live run per member. A person is in one place at a time, and
    // two runs under the same number would be a lie about where
    // somebody is.
    runs.close_for_owner(user_id).await;
    let (id, _) = runs.open(user_id, member_no).await;
    Ok(Started { id })
}
