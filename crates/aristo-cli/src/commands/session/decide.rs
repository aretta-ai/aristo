//! `aristo session decide --item <ref> --bucket <accepted|rejected|pending>`.
//!
//! Records a decision on one item in the active session. Per-kind
//! side effects (mutating a `.critique` file, etc.) plug in via the
//! `SessionKind` trait in step 5 — for step 2 the handler updates
//! only the substrate's state: the bucket on the item, plus a
//! rejection-log / backlog entry with a NULL per-kind payload.

use crate::session::backlog::BacklogEntry;
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

    update_or_insert_item(&mut session, &item_ref, new_status, note.clone(), &now);
    storage::write_active_session(&ws, &session)?;

    // Substrate-only side effects. Per-kind effects (e.g. mutating
    // the .critique file on accept) plug in via SessionKind in step 5.
    match new_status {
        ItemStatus::Rejected => {
            rejections::append(
                &ws,
                &RejectionEntry {
                    ts: now.clone(),
                    kind: session.kind.clone(),
                    item_ref: item_ref.clone(),
                    note: note.clone(),
                    // Empty fingerprint until per-kind step 5 supplies one.
                    // `matches_prior_rejection` for any kind that hasn't
                    // landed yet will see no prior rejections (vacuous
                    // truth on empty fingerprints).
                    fingerprint: serde_json::Value::Null,
                },
            )?;
        }
        ItemStatus::Pending => {
            backlog::append_entry(
                &ws,
                &session.kind,
                BacklogEntry {
                    item_ref: item_ref.clone(),
                    deferred_at: now.clone(),
                    deferred_from_session: session.id.clone(),
                    note: note.clone(),
                    data: serde_json::Value::Null,
                },
            )?;
        }
        ItemStatus::Accepted => {
            // No substrate-level side effect. Per-kind on_accept lands
            // in step 5 (e.g. mutates .critique[i].disposition).
        }
        ItemStatus::Open => unreachable!("BucketArg cannot map to Open"),
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
