use uuid::Uuid;

use crate::{
    db::{Pool, RlsTransaction},
    domain::schedule::{repository, types::*},
    error::{AppError, AppResult},
};

const MAX_TITLE_LEN: usize = 200;
const MAX_NOTES_LEN: usize = 4000;
const MAX_LOCATION_LEN: usize = 200;

pub async fn list_personal(pool: &Pool, user_id: Uuid) -> AppResult<Vec<PersonalItem>> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let items = repository::list_by_user(tx.conn(), user_id).await?;
    tx.commit().await?;
    Ok(items)
}

pub async fn create_personal(
    pool: &Pool,
    user_id: Uuid,
    req: CreatePersonalItemRequest,
) -> AppResult<PersonalItem> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(AppError::bad_request("title required"));
    }
    if title.len() > MAX_TITLE_LEN {
        return Err(AppError::bad_request("title too long"));
    }
    if req.ends_at < req.starts_at {
        return Err(AppError::bad_request(
            "ends_at must be on or after starts_at",
        ));
    }
    if let Some(n) = req.notes.as_deref() {
        if n.len() > MAX_NOTES_LEN {
            return Err(AppError::bad_request("notes too long"));
        }
    }
    if let Some(l) = req.location.as_deref() {
        if l.len() > MAX_LOCATION_LEN {
            return Err(AppError::bad_request("location too long"));
        }
    }

    let notes = req
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let location = req
        .location
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let item = repository::insert(
        tx.conn(),
        user_id,
        title,
        notes,
        req.starts_at,
        req.ends_at,
        req.is_all_day,
        location,
    )
    .await?;
    tx.commit().await?;
    Ok(item)
}

pub async fn update_personal(
    pool: &Pool,
    user_id: Uuid,
    id: Uuid,
    req: UpdatePersonalItemRequest,
) -> AppResult<PersonalItem> {
    if let Some(t) = req.title.as_deref() {
        let trimmed = t.trim();
        if trimmed.is_empty() {
            return Err(AppError::bad_request("title cannot be blank"));
        }
        if trimmed.len() > MAX_TITLE_LEN {
            return Err(AppError::bad_request("title too long"));
        }
    }
    if let (Some(s), Some(e)) = (req.starts_at, req.ends_at) {
        if e < s {
            return Err(AppError::bad_request(
                "ends_at must be on or after starts_at",
            ));
        }
    }
    if let Some(n) = req.notes.as_deref() {
        if n.len() > MAX_NOTES_LEN {
            return Err(AppError::bad_request("notes too long"));
        }
    }
    if let Some(l) = req.location.as_deref() {
        if l.len() > MAX_LOCATION_LEN {
            return Err(AppError::bad_request("location too long"));
        }
    }

    // Trimming: empty string → clear the field (Some(None)); else Some(Some(trimmed)).
    let title = req.title.as_deref().map(str::trim).map(str::to_string);
    let notes = req.notes.as_ref().map(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    let location = req.location.as_ref().map(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let updated = repository::update(
        tx.conn(),
        id,
        title.as_deref(),
        notes.as_ref().map(|o| o.as_deref()),
        req.starts_at,
        req.ends_at,
        req.is_all_day,
        location.as_ref().map(|o| o.as_deref()),
    )
    .await?;
    tx.commit().await?;
    updated.ok_or(AppError::NotFound)
}

pub async fn delete_personal(pool: &Pool, user_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let deleted = repository::delete(tx.conn(), id).await?;
    tx.commit().await?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(())
}
