//! `aristo session decide --item <ref> --bucket <accepted|rejected|pending>`.
//!
//! Records a decision on one item in the active session. Per-kind
//! side effects (mutating a `.critique` file, etc.) plug in via the
//! `SessionKind` trait in step 5 — for step 2 the handler updates
//! only the substrate's state: the bucket on the item, plus a
//! rejection-log / backlog entry with a NULL per-kind payload.

use crate::session::backlog::BacklogEntry;
use crate::session::kind::kind_for;
use crate::session::rejections::RejectionEntry;
use crate::session::types::{Item, ItemRef, ItemStatus, Session};
use crate::session::{backlog, rejections, storage};
use crate::{BucketArg, CliError, CliResult};

use super::{item_status_from_bucket, load_active, now_rfc3339, workspace_or_error};

pub(crate) fn run(item_ref_str: &str, bucket: BucketArg, note: Option<String>) -> CliResult<()> {
    let ws = workspace_or_error()?;
    let Some(mut session) = load_active(&ws)? else {
        return Err(CliError::Other {
            message:
                "no active session — start one with `aristo session start <kind> --subject <...>`"
                    .into(),
            exit_code: 1,
        });
    };

    let item_ref = ItemRef::from_opaque(item_ref_str.to_string());
    let new_status = item_status_from_bucket(bucket);
    let now = now_rfc3339();

    // Look up the per-kind handler. Unknown kinds get substrate-only
    // treatment (no per-kind callbacks fire, fingerprint and data are
    // null) — handy for tests and for future kinds before their impl
    // lands.
    let kind = kind_for(&session.kind);
    let note_opt = note.as_deref();

    // Per-kind side effects FIRST. If they fail (e.g. .critique
    // missing for a critique-review session), we want to refuse
    // before mutating substrate state so the user can retry.
    let (fingerprint, backlog_data) = match (new_status, kind.as_ref()) {
        (ItemStatus::Accepted, Some(k)) => {
            k.on_accept(&item_ref, note_opt, &ws)?;
            (serde_json::Value::Null, serde_json::Value::Null)
        }
        (ItemStatus::Rejected, Some(k)) => {
            let fp = k.on_reject(&item_ref, note_opt, &ws)?;
            (fp, serde_json::Value::Null)
        }
        (ItemStatus::Pending, Some(k)) => {
            let data = k.on_pending(&item_ref, note_opt, &ws)?;
            (serde_json::Value::Null, data)
        }
        (_, None) => (serde_json::Value::Null, serde_json::Value::Null),
        (ItemStatus::Open, _) => unreachable!("BucketArg cannot map to Open"),
    };

    update_or_insert_item(&mut session, &item_ref, new_status, note.clone(), &now);
    storage::write_active_session(&ws, &session)?;

    // Substrate-level records.
    if matches!(new_status, ItemStatus::Rejected) {
        rejections::append(
            &ws,
            &RejectionEntry {
                ts: now.clone(),
                kind: session.kind.clone(),
                item_ref: item_ref.clone(),
                note: note.clone(),
                fingerprint,
            },
        )?;
    }
    if matches!(new_status, ItemStatus::Pending) {
        backlog::append_entry(
            &ws,
            &session.kind,
            BacklogEntry {
                item_ref: item_ref.clone(),
                deferred_at: now.clone(),
                deferred_from_session: session.id.clone(),
                note: note.clone(),
                data: backlog_data,
            },
        )?;
    }

    println!("ok: {item_ref} → {bucket:?}");
    Ok(())
}

/// In-place upsert of `item` on `session`. If an item with the same
/// ref already exists, replace its status / note / closed_at; else
/// append a new entry. The substrate makes re-decision idempotent —
/// the user can change their mind on an item mid-session.
fn update_or_insert_item(
    session: &mut Session,
    item_ref: &ItemRef,
    status: ItemStatus,
    note: Option<String>,
    now: &str,
) {
    if let Some(existing) = session.items.iter_mut().find(|i| i.item_ref == *item_ref) {
        existing.status = status;
        existing.note = note;
        existing.closed_at = Some(now.to_string());
    } else {
        session.items.push(Item {
            item_ref: item_ref.clone(),
            status,
            note,
            closed_at: Some(now.to_string()),
        });
    }
}
