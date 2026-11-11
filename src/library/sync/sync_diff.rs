use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;
use serde_json::{Map, Value};
use similar::TextDiff;
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};

use crate::{error::AppError, library::sync::entitlement_models::SyncResult};

/// Serializes writes so overlapping syncs can't interleave appends.
static WRITE_LOCK: Mutex<()> = Mutex::const_new(());

/// One piece of a sync response, attributed to a book (or tag) key.
#[derive(Debug, Clone)]
pub struct DiffEntry {
    /// Directory key: the book id, or `tag_<id>` for tag results.
    pub key: String,
    /// Which `SyncResult` variant this came from, e.g. `ChangedEntitlement`.
    pub kind: &'static str,
    /// Snapshot section this value replaces: `Entitlement`, `ReadingState`, `Tag`.
    pub section: &'static str,
    pub value: Value,
}

/// Split sync results into per-book entries. Call this on the final merged
/// results (including any proxied Kobo-store ones).
pub fn entries_from_results(results: &[SyncResult]) -> Vec<DiffEntry> {
    let mut out = Vec::new();

    for r in results {
        if let Some(e) = &r.new_entitlement {
            out.extend(entry(
                &e.book_entitlement.id,
                "NewEntitlement",
                "Entitlement",
                e,
            ));
        }
        if let Some(e) = &r.changed_entitlement {
            out.extend(entry(
                &e.book_entitlement.id,
                "ChangedEntitlement",
                "Entitlement",
                e,
            ));
        }
        if let Some(c) = &r.changed_reading_state {
            let rs = &c.reading_state;
            out.extend(entry(
                &rs.entitlement_id,
                "ChangedReadingState",
                "ReadingState",
                rs,
            ));
        }
        if let Some(t) = &r.new_tag {
            out.extend(entry(&format!("tag_{}", t.id), "NewTag", "Tag", t));
        }
        if let Some(t) = &r.changed_tag {
            out.extend(entry(&format!("tag_{}", t.id), "ChangedTag", "Tag", t));
        }
        if let Some(d) = &r.deleted_tag {
            out.extend(entry(
                &format!("tag_{}", d.tag.id),
                "DeletedTag",
                "Tag",
                &d.tag,
            ));
        }
    }
    out
}

fn entry<T: Serialize>(
    key: &str,
    kind: &'static str,
    section: &'static str,
    value: &T,
) -> Option<DiffEntry> {
    serde_json::to_value(value).ok().map(|value| DiffEntry {
        key: key.to_string(),
        kind,
        section,
        value,
    })
}

/// Apply entries to the on-disk snapshots, appending a diff for each change.
/// `redact` strings (e.g. the auth key embedded in download URLs) are replaced
/// before anything is compared or written.
pub async fn record(
    root: &Path,
    entries: Vec<DiffEntry>,
    redact: &[String],
) -> Result<(), AppError> {
    if entries.is_empty() {
        return Ok(());
    }
    let _guard = WRITE_LOCK.lock().await;

    for entry in entries {
        // ids from proxied store responses are untrusted; never let one escape `root`
        if entry.key.contains(['/', '\\']) || entry.key.contains("..") {
            continue;
        }
        let dir: PathBuf = root.join(&entry.key);
        fs::create_dir_all(&dir).await?;
        let latest = dir.join("latest.json");
        let history = dir.join("history.diff");

        let old_map: Map<String, Value> = match fs::read_to_string(&latest).await {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
            Err(e) => return Err(e.into()),
        };

        let mut value = entry.value;
        redact_strings(&mut value, redact);

        let mut new_map = old_map.clone();
        new_map.insert(entry.section.to_string(), value);

        let old_text = pretty(&old_map)?;
        let new_text = pretty(&new_map)?;
        if old_text == new_text {
            continue; // resent identical content: nothing to log
        }

        let now = Utc::now().to_rfc3339();
        let old_label = if old_map.is_empty() {
            "/dev/null"
        } else {
            "previous"
        };
        let new_label = format!("{} @ {now}", entry.kind);
        let diff = TextDiff::from_lines(old_text.as_str(), new_text.as_str())
            .unified_diff()
            .context_radius(3)
            .header(old_label, &new_label)
            .to_string();

        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&history)
            .await?;
        f.write_all(
            format!("=== {now} {} book={} ===\n{diff}\n", entry.kind, entry.key).as_bytes(),
        )
        .await?;
        fs::write(&latest, new_text).await?;
    }
    Ok(())
}

fn pretty(map: &Map<String, Value>) -> Result<String, serde_json::Error> {
    Ok(serde_json::to_string_pretty(map)? + "\n")
}

fn redact_strings(v: &mut Value, secrets: &[String]) {
    match v {
        Value::String(s) => {
            for secret in secrets.iter().filter(|s| !s.is_empty()) {
                if s.contains(secret.as_str()) {
                    *s = s.replace(secret.as_str(), "<redacted>");
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| redact_strings(x, secrets)),
        Value::Object(m) => m.values_mut().for_each(|x| redact_strings(x, secrets)),
        _ => {}
    }
}
