//! Collezioni automatiche: "James Bond Collection", "Il Signore degli Anelli - La Saga"…
//! Arrivano dal `<set>` degli NFO e da `belongs_to_collection` di TMDB. Ogni collezione è un elemento
//! della libreria (kind = 'collection'); i film ne sono membri. Una collezione rimasta vuota si cancella.

use crate::util::now_ms;
use sqlx::{Sqlite, Transaction};

/// Chiave per riconoscere la stessa collezione: identificativo TMDB se c'è, altrimenti il nome normalizzato.
fn name_key(name: &str) -> String {
    format!("set:{}", platea_naming::sort_title(name))
}

/// Mette il film nella collezione indicata (creandola) e lo toglie da altre. Restituisce l'id della collezione.
pub async fn link(tx: &mut Transaction<'_, Sqlite>, library_id: i64, item_id: i64, name: &str, tmdb_id: Option<i64>) -> anyhow::Result<Option<i64>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    let by_tmdb = tmdb_id.map(|t| format!("tmdbcol:{t}"));
    let by_name = name_key(name);
    let mut found: Option<i64> = None;
    if let Some(k) = &by_tmdb {
        found = sqlx::query_scalar("SELECT id FROM items WHERE library_id = ? AND kind = 'collection' AND match_key = ?")
            .bind(library_id)
            .bind(k)
            .fetch_optional(&mut **tx)
            .await?;
    }
    if found.is_none() {
        found = sqlx::query_scalar(
            "SELECT id FROM items WHERE library_id = ? AND kind = 'collection' AND (match_key = ? OR sort_title = ?) AND match_key NOT LIKE 'manual:%'",
        )
        .bind(library_id)
        .bind(&by_name)
        .bind(platea_naming::sort_title(name))
        .fetch_optional(&mut **tx)
        .await?;
    }
    let now = now_ms();
    let cid = match found {
        Some(id) => {
            // Con l'identificativo TMDB la chiave diventa quella stabile.
            if let Some(k) = &by_tmdb {
                sqlx::query("UPDATE items SET match_key = ? WHERE id = ?").bind(k).bind(id).execute(&mut **tx).await?;
            }
            id
        }
        None => sqlx::query(
            "INSERT INTO items (library_id, kind, title, sort_title, match_key, match_state, added_at, updated_at)
             VALUES (?, 'collection', ?, ?, ?, 'matched', ?, ?)",
        )
        .bind(library_id)
        .bind(name)
        .bind(platea_naming::sort_title(name))
        .bind(by_tmdb.as_deref().unwrap_or(&by_name))
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?
        .last_insert_rowid(),
    };
    // Il film lascia le altre collezioni automatiche; quelle create a mano dall'amministratore restano.
    sqlx::query(
        "DELETE FROM collection_members WHERE item_id = ? AND collection_id <> ?
         AND collection_id NOT IN (SELECT id FROM items WHERE kind = 'collection' AND match_key LIKE 'manual:%')",
    )
    .bind(item_id)
    .bind(cid)
    .execute(&mut **tx)
    .await?;
    sqlx::query("INSERT OR IGNORE INTO collection_members (collection_id, item_id) VALUES (?, ?)").bind(cid).bind(item_id).execute(&mut **tx).await?;
    refresh(tx, cid).await?;
    Ok(Some(cid))
}

/// Anno (il primo film) e data di aggiunta (l'ultimo arrivato) della collezione.
pub async fn refresh(tx: &mut Transaction<'_, Sqlite>, cid: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE items SET
           year = (SELECT MIN(m.year) FROM collection_members c JOIN items m ON m.id = c.item_id WHERE c.collection_id = items.id),
           added_at = COALESCE((SELECT MAX(m.added_at) FROM collection_members c JOIN items m ON m.id = c.item_id WHERE c.collection_id = items.id), added_at),
           updated_at = ?
         WHERE id = ?",
    )
    .bind(now_ms())
    .bind(cid)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Cancella le collezioni senza membri (dopo la pulizia dei file mancanti).
pub async fn prune(tx: &mut Transaction<'_, Sqlite>) -> anyhow::Result<u64> {
    Ok(sqlx::query("DELETE FROM items WHERE kind = 'collection' AND NOT EXISTS (SELECT 1 FROM collection_members c WHERE c.collection_id = items.id)")
        .execute(&mut **tx)
        .await?
        .rows_affected())
}
