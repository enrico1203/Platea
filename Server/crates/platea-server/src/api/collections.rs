//! Collezioni create a mano dall'amministratore ("Da vedere a Natale", "Marvel"…), accanto a quelle automatiche.
//! Sono elementi `kind = 'collection'` con chiave `manual:…`: le collezioni automatiche non le toccano.

use super::{Access, Api};
use crate::auth::AdminUser;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use crate::util::{now_ms, random_hex};
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn routes(api: Api) -> Api {
    api.get("/api/v1/collections/manual", "Metadati", Access::Admin, "Collezioni create a mano", list)
        .post("/api/v1/collections", "Metadati", Access::Admin, "Crea una collezione con i primi film o serie", create)
        .patch("/api/v1/collections/{id}", "Metadati", Access::Admin, "Rinomina una collezione creata a mano", rename)
        .delete("/api/v1/collections/{id}", "Metadati", Access::Admin, "Cancella una collezione creata a mano (i film restano)", remove)
        .post("/api/v1/collections/{id}/items", "Metadati", Access::Admin, "Aggiunge film o serie a una collezione creata a mano", add)
        .delete("/api/v1/collections/{id}/items/{item}", "Metadati", Access::Admin, "Toglie un titolo da una collezione creata a mano", remove_item)
}

fn decode(state: &Shared, id: &str, what: &'static str) -> ApiResult<i64> {
    state.ids.decode(id).ok_or_else(|| ApiError::not_found(what))
}

async fn manual(state: &Shared, id: &str) -> ApiResult<i64> {
    let cid = decode(state, id, "Collezione")?;
    let ok: Option<i64> = sqlx::query_scalar("SELECT 1 FROM items WHERE id = ? AND kind = 'collection' AND match_key LIKE 'manual:%'")
        .bind(cid)
        .fetch_optional(&state.db.read)
        .await?;
    ok.map(|_| cid).ok_or_else(|| ApiError::BadRequest("Si modificano solo le collezioni create a mano".into()))
}

fn clean_name(name: &str) -> ApiResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 200 {
        return Err(ApiError::BadRequest("Nome della collezione non valido".into()));
    }
    Ok(n.to_string())
}

async fn add_members(state: &Shared, cid: i64, ids: &[String]) -> ApiResult<usize> {
    let mut tx = state.db.write.begin().await?;
    let mut n = 0;
    for id in ids.iter().take(500) {
        let iid = decode(state, id, "Elemento")?;
        let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM items WHERE id = ?").bind(iid).fetch_optional(&mut *tx).await?;
        if !matches!(kind.as_deref(), Some("movie" | "series")) {
            continue;
        }
        n += sqlx::query("INSERT OR IGNORE INTO collection_members (collection_id, item_id) VALUES (?, ?)")
            .bind(cid)
            .bind(iid)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;
    }
    crate::metadata::collections::refresh(&mut tx, cid).await.map_err(ApiError::Internal)?;
    tx.commit().await?;
    Ok(n)
}

async fn list(State(state): State<Shared>, _a: AdminUser) -> ApiResult<Json<Value>> {
    let rows: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT i.id, i.title, (SELECT COUNT(*) FROM collection_members c WHERE c.collection_id = i.id)
         FROM items i WHERE i.kind = 'collection' AND i.match_key LIKE 'manual:%' ORDER BY i.sort_title",
    )
    .fetch_all(&state.db.read)
    .await?;
    Ok(Json(json!({ "items": rows.into_iter().map(|(id, t, n)| json!({ "id": state.ids.encode(id), "title": t, "count": n })).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
struct CreateReq {
    name: String,
    item_ids: Vec<String>,
}

async fn create(State(state): State<Shared>, _a: AdminUser, Json(req): Json<CreateReq>) -> ApiResult<Json<Value>> {
    let name = clean_name(&req.name)?;
    let first = req.item_ids.first().ok_or_else(|| ApiError::BadRequest("Serve almeno un titolo".into()))?;
    let lib: Option<i64> = sqlx::query_scalar("SELECT library_id FROM items WHERE id = ? AND kind IN ('movie','series')")
        .bind(decode(&state, first, "Elemento")?)
        .fetch_optional(&state.db.read)
        .await?;
    // La collezione vive nella libreria del primo titolo; i membri possono stare anche in altre librerie.
    let lib = lib.ok_or_else(|| ApiError::BadRequest("Il primo titolo deve essere un film o una serie".into()))?;
    let now = now_ms();
    let cid = sqlx::query(
        "INSERT INTO items (library_id, kind, title, sort_title, match_key, match_state, locked_fields, added_at, updated_at)
         VALUES (?, 'collection', ?, ?, ?, 'manual', 'title', ?, ?)",
    )
    .bind(lib)
    .bind(&name)
    .bind(platea_naming::sort_title(&name))
    .bind(format!("manual:{}", random_hex(6)))
    .bind(now)
    .bind(now)
    .execute(&state.db.write)
    .await?
    .last_insert_rowid();
    let added = add_members(&state, cid, &req.item_ids).await?;
    Ok(Json(json!({ "id": state.ids.encode(cid), "name": name, "added": added })))
}

#[derive(Deserialize)]
struct RenameReq {
    name: String,
}

async fn rename(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>, Json(req): Json<RenameReq>) -> ApiResult<Json<Value>> {
    let cid = manual(&state, &id).await?;
    let name = clean_name(&req.name)?;
    sqlx::query("UPDATE items SET title = ?, sort_title = ?, updated_at = ? WHERE id = ?")
        .bind(&name)
        .bind(platea_naming::sort_title(&name))
        .bind(now_ms())
        .bind(cid)
        .execute(&state.db.write)
        .await?;
    Ok(Json(json!({ "id": id, "name": name })))
}

async fn remove(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let cid = manual(&state, &id).await?;
    sqlx::query("DELETE FROM items WHERE id = ?").bind(cid).execute(&state.db.write).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct AddReq {
    item_ids: Vec<String>,
}

async fn add(State(state): State<Shared>, _a: AdminUser, Path(id): Path<String>, Json(req): Json<AddReq>) -> ApiResult<Json<Value>> {
    let cid = manual(&state, &id).await?;
    let added = add_members(&state, cid, &req.item_ids).await?;
    Ok(Json(json!({ "added": added })))
}

async fn remove_item(State(state): State<Shared>, _a: AdminUser, Path((id, item)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let cid = manual(&state, &id).await?;
    let iid = decode(&state, &item, "Elemento")?;
    sqlx::query("DELETE FROM collection_members WHERE collection_id = ? AND item_id = ?").bind(cid).bind(iid).execute(&state.db.write).await?;
    let mut tx = state.db.write.begin().await?;
    crate::metadata::collections::refresh(&mut tx, cid).await.map_err(ApiError::Internal)?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
