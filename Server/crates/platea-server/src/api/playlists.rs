//! Playlist personali: film ed episodi in un ordine scelto dall'utente.
//! Una riga per voce (`playlist_items`), mai il contenuto serializzato in una colonna.
//! Le voci rispettano i permessi correnti: una libreria tolta o un limite d'età nascondono le voci, non le cancellano.

use super::items::{item_json, ItemRow, ITEM_SELECT};
use super::{Access, Api};
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use crate::util::now_ms;
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

/// Oltre questo numero di voci una playlist non è più una playlist.
const MAX_ENTRIES: i64 = 5000;

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/playlists", "Playlist", Access::User, "Le playlist dell'utente", list)
        .post("/api/v1/playlists", "Playlist", Access::User, "Crea una playlist, anche con le prime voci", create)
        .get("/api/v1/playlists/{id}", "Playlist", Access::User, "Una playlist con le sue voci in ordine", detail)
        .patch("/api/v1/playlists/{id}", "Playlist", Access::User, "Rinomina una playlist", rename)
        .delete("/api/v1/playlists/{id}", "Playlist", Access::User, "Cancella una playlist", remove)
        .post("/api/v1/playlists/{id}/items", "Playlist", Access::User, "Aggiunge film ed episodi (serie e stagioni diventano i loro episodi)", add_items)
        .delete("/api/v1/playlists/{id}/items/{entry}", "Playlist", Access::User, "Toglie una voce", remove_entry)
        .post("/api/v1/playlists/{id}/move", "Playlist", Access::User, "Sposta una voce in un'altra posizione", move_entry)
}

/// La playlist esiste ed è dell'utente.
async fn own(state: &Shared, user: &AuthUser, id: &str) -> ApiResult<i64> {
    let pid = state.ids.decode(id).ok_or_else(|| ApiError::not_found("Playlist"))?;
    let owner: Option<i64> = sqlx::query_scalar("SELECT user_id FROM playlists WHERE id = ?").bind(pid).fetch_optional(&state.db.read).await?;
    match owner {
        Some(u) if u == user.user_id => Ok(pid),
        _ => Err(ApiError::not_found("Playlist")),
    }
}

async fn list(State(state): State<Shared>, user: AuthUser) -> ApiResult<Json<Value>> {
    let rows: Vec<(i64, String, i64)> = sqlx::query_as("SELECT id, name, created_at FROM playlists WHERE user_id = ? ORDER BY name COLLATE NOCASE")
        .bind(user.user_id)
        .fetch_all(&state.db.read)
        .await?;
    let lf = user.library_filter("i.library_id");
    let mut out = Vec::with_capacity(rows.len());
    for (id, name, created) in rows {
        // Conteggio, durata e locandina contano solo le voci visibili all'utente.
        let (count, runtime): (i64, Option<i64>) =
            sqlx::query_as(&format!("SELECT COUNT(*), SUM(i.runtime_ms) FROM playlist_items p JOIN items i ON i.id = p.item_id WHERE p.playlist_id = ? {lf}"))
                .bind(id)
                .fetch_one(&state.db.read)
                .await?;
        let poster: Option<i64> = sqlx::query_scalar(&format!(
            "SELECT im.id FROM playlist_items p JOIN items i ON i.id = p.item_id
             JOIN images im ON im.item_id = COALESCE(i.series_id, i.id) AND im.kind = 'poster'
             WHERE p.playlist_id = ? {lf} ORDER BY p.sort_order LIMIT 1"
        ))
        .bind(id)
        .fetch_optional(&state.db.read)
        .await?;
        out.push(json!({
            "id": state.ids.encode(id),
            "name": name,
            "created_at": created,
            "count": count,
            "runtime_ms": runtime,
            "poster": super::items::image_url(&state, poster),
        }));
    }
    Ok(Json(json!({ "items": out })))
}

#[derive(Deserialize)]
struct CreateReq {
    name: String,
    #[serde(default)]
    item_ids: Vec<String>,
}

async fn create(State(state): State<Shared>, user: AuthUser, Json(req): Json<CreateReq>) -> ApiResult<Json<Value>> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(ApiError::BadRequest("Nome della playlist non valido".into()));
    }
    let id = sqlx::query("INSERT INTO playlists (user_id, name, created_at) VALUES (?, ?, ?)")
        .bind(user.user_id)
        .bind(name)
        .bind(now_ms())
        .execute(&state.db.write)
        .await?
        .last_insert_rowid();
    let added = append(&state, &user, id, &req.item_ids).await?;
    Ok(Json(json!({ "id": state.ids.encode(id), "name": name, "added": added })))
}

async fn detail(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let pid = own(&state, &user, &id).await?;
    let (name, created): (String, i64) = sqlx::query_as("SELECT name, created_at FROM playlists WHERE id = ?").bind(pid).fetch_one(&state.db.read).await?;
    let entries: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, item_id FROM playlist_items WHERE playlist_id = ? ORDER BY sort_order, id").bind(pid).fetch_all(&state.db.read).await?;
    let sql = format!("{ITEM_SELECT} WHERE i.id IN (SELECT item_id FROM playlist_items WHERE playlist_id = ?2) {}", user.library_filter("i.library_id"));
    let rows: Vec<ItemRow> = sqlx::query_as(&sql).bind(user.user_id).bind(pid).fetch_all(&state.db.read).await?;
    let by_id: std::collections::HashMap<i64, &ItemRow> = rows.iter().map(|r| (r.id, r)).collect();
    let items: Vec<Value> = entries
        .iter()
        .filter_map(|(entry, item)| {
            let r = by_id.get(item)?;
            let mut v = item_json(&state, r);
            v["entry_id"] = json!(state.ids.encode(*entry));
            Some(v)
        })
        .collect();
    let runtime: i64 = rows.iter().filter_map(|r| r.runtime_ms).sum();
    Ok(Json(json!({ "id": id, "name": name, "created_at": created, "runtime_ms": runtime, "items": items })))
}

#[derive(Deserialize)]
struct RenameReq {
    name: String,
}

async fn rename(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, Json(req): Json<RenameReq>) -> ApiResult<Json<Value>> {
    let pid = own(&state, &user, &id).await?;
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(ApiError::BadRequest("Nome della playlist non valido".into()));
    }
    sqlx::query("UPDATE playlists SET name = ? WHERE id = ?").bind(name).bind(pid).execute(&state.db.write).await?;
    Ok(Json(json!({ "id": id, "name": name })))
}

async fn remove(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let pid = own(&state, &user, &id).await?;
    sqlx::query("DELETE FROM playlists WHERE id = ?").bind(pid).execute(&state.db.write).await?;
    Ok(Json(json!({ "deleted": id })))
}

#[derive(Deserialize)]
struct AddReq {
    item_ids: Vec<String>,
}

async fn add_items(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, Json(req): Json<AddReq>) -> ApiResult<Json<Value>> {
    let pid = own(&state, &user, &id).await?;
    let added = append(&state, &user, pid, &req.item_ids).await?;
    Ok(Json(json!({ "added": added })))
}

/// Risolve gli elementi in voci riproducibili (film, episodi) e le accoda in un'unica transazione.
async fn append(state: &Shared, user: &AuthUser, pid: i64, ids: &[String]) -> ApiResult<usize> {
    let mut playable: Vec<i64> = Vec::new();
    for raw in ids {
        let iid = state.ids.decode(raw).ok_or_else(|| ApiError::not_found("Elemento"))?;
        let row = super::items::load_item(state, user, iid).await?;
        match row.kind.as_str() {
            "movie" | "episode" => playable.push(iid),
            // Una serie o una stagione diventano i loro episodi in ordine (speciali esclusi per le serie).
            "series" | "season" => {
                let col = if row.kind == "series" { "i.series_id" } else { "i.parent_id" };
                let specials = if row.kind == "series" { " AND COALESCE(i.parent_index_number, 0) > 0" } else { "" };
                let eps: Vec<i64> = sqlx::query_scalar(&format!(
                    "SELECT i.id FROM items i WHERE {col} = ? AND i.kind = 'episode'{specials} {} ORDER BY i.parent_index_number, i.index_number",
                    user.library_filter("i.library_id")
                ))
                .bind(iid)
                .fetch_all(&state.db.read)
                .await?;
                playable.extend(eps);
            }
            "collection" => {
                let films: Vec<i64> = sqlx::query_scalar(&format!(
                    "SELECT i.id FROM collection_members c JOIN items i ON i.id = c.item_id WHERE c.collection_id = ? {} ORDER BY COALESCE(i.premiere_date, i.year, '')",
                    user.library_filter("i.library_id")
                ))
                .bind(iid)
                .fetch_all(&state.db.read)
                .await?;
                playable.extend(films);
            }
            _ => return Err(ApiError::BadRequest("Questo elemento non si può aggiungere a una playlist".into())),
        }
    }
    if playable.is_empty() {
        return Ok(0);
    }
    let mut tx = state.db.write.begin().await?;
    let (count, max): (i64, Option<i64>) =
        sqlx::query_as("SELECT COUNT(*), MAX(sort_order) FROM playlist_items WHERE playlist_id = ?").bind(pid).fetch_one(&mut *tx).await?;
    if count + playable.len() as i64 > MAX_ENTRIES {
        return Err(ApiError::BadRequest(format!("Una playlist può avere al massimo {MAX_ENTRIES} voci")));
    }
    let mut order = max.unwrap_or(-1);
    for item in &playable {
        order += 1;
        sqlx::query("INSERT INTO playlist_items (playlist_id, item_id, sort_order) VALUES (?, ?, ?)")
            .bind(pid)
            .bind(item)
            .bind(order)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(playable.len())
}

async fn remove_entry(State(state): State<Shared>, user: AuthUser, Path((id, entry)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let pid = own(&state, &user, &id).await?;
    let eid = state.ids.decode(&entry).ok_or_else(|| ApiError::not_found("Voce"))?;
    let n = sqlx::query("DELETE FROM playlist_items WHERE id = ? AND playlist_id = ?").bind(eid).bind(pid).execute(&state.db.write).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::not_found("Voce"));
    }
    Ok(Json(json!({ "deleted": entry })))
}

#[derive(Deserialize)]
struct MoveReq {
    entry_id: String,
    /// Nuova posizione (da 0) nell'elenco completo della playlist.
    index: i64,
}

/// Riordino: le posizioni si riscrivono tutte, compatte da 0 (le playlist sono piccole).
async fn move_entry(State(state): State<Shared>, user: AuthUser, Path(id): Path<String>, Json(req): Json<MoveReq>) -> ApiResult<Json<Value>> {
    let pid = own(&state, &user, &id).await?;
    let eid = state.ids.decode(&req.entry_id).ok_or_else(|| ApiError::not_found("Voce"))?;
    let mut tx = state.db.write.begin().await?;
    let mut ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM playlist_items WHERE playlist_id = ? ORDER BY sort_order, id").bind(pid).fetch_all(&mut *tx).await?;
    let from = ids.iter().position(|x| *x == eid).ok_or_else(|| ApiError::not_found("Voce"))?;
    let e = ids.remove(from);
    let to = (req.index.max(0) as usize).min(ids.len());
    ids.insert(to, e);
    for (i, x) in ids.iter().enumerate() {
        sqlx::query("UPDATE playlist_items SET sort_order = ? WHERE id = ?").bind(i as i64).bind(x).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "moved": req.entry_id, "index": to })))
}
