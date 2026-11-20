//! Forma dei dati nel documento OpenAPI. Gli schemi vengono dai tipi TypeScript della web UI
//! (`scripts/openapi-schemas.sh` → `openapi_schemas.json`): una sola fonte, già verificata dal compilatore della web UI.
//! Qui si dice quale schema ha il corpo della richiesta e quale la risposta di ogni route.

use serde_json::{json, Value};

/// (metodo, percorso, corpo della richiesta, risposta 200).
pub const BODIES: &[(&str, &str, Option<&str>, Option<&str>)] = &[
    ("get", "/api/v1/system/info", None, Some("SystemInfo")),
    ("post", "/api/v1/auth/login", Some("LoginRequest"), Some("LoginResponse")),
    ("post", "/api/v1/auth/logout", None, Some("Ok")),
    ("get", "/api/v1/me", None, Some("Me")),
    ("post", "/api/v1/me/password", Some("PasswordChangeRequest"), Some("Ok")),
    ("get", "/api/v1/me/preferences", None, Some("UserPrefs")),
    ("put", "/api/v1/me/preferences", Some("UserPrefs"), Some("UserPrefs")),
    ("get", "/api/v1/me/devices", None, Some("DeviceList")),
    ("get", "/api/v1/home", None, Some("Home")),
    ("get", "/api/v1/libraries", None, Some("LibraryPage")),
    ("get", "/api/v1/libraries/{id}", None, Some("Library")),
    ("get", "/api/v1/libraries/stats", None, Some("LibraryStats")),
    ("get", "/api/v1/items", None, Some("ItemPage")),
    ("get", "/api/v1/search", None, Some("ItemPage")),
    ("get", "/api/v1/items/{id}", None, Some("ItemDetail")),
    ("get", "/api/v1/items/{id}/children", None, Some("ItemPage")),
    ("get", "/api/v1/items/{id}/similar", None, Some("ItemPage")),
    ("get", "/api/v1/items/{id}/missing", None, Some("MissingEpisodeList")),
    ("get", "/api/v1/people/{id}", None, Some("PersonDetail")),
    ("post", "/api/v1/playback/info", Some("PlaybackInfoRequest"), Some("PlaybackInfo")),
    ("post", "/api/v1/playback/{session}/progress", Some("ProgressRequest"), Some("Ok")),
    ("post", "/api/v1/playback/{session}/tracks", Some("TracksRequest"), None),
    ("post", "/api/v1/playback/{session}/stop", Some("ProgressRequest"), Some("Ok")),
    ("get", "/api/v1/playlists", None, Some("PlaylistList")),
    ("post", "/api/v1/playlists", Some("PlaylistCreateRequest"), Some("PlaylistCreateResponse")),
    ("get", "/api/v1/playlists/{id}", None, Some("PlaylistDetail")),
    ("patch", "/api/v1/playlists/{id}", Some("PlaylistUpdateRequest"), None),
    ("post", "/api/v1/playlists/{id}/items", Some("PlaylistAddRequest"), None),
    ("post", "/api/v1/playlists/{id}/move", Some("PlaylistMoveRequest"), Some("Ok")),
    ("post", "/api/v1/quickconnect/initiate", Some("QuickConnectInitiateRequest"), Some("QuickConnectInitiateResponse")),
    ("get", "/api/v1/quickconnect/status/{secret}", None, Some("QuickConnectStatus")),
    ("post", "/api/v1/quickconnect/redeem", Some("QuickConnectRedeemRequest"), Some("LoginResponse")),
    ("post", "/api/v1/quickconnect/authorize", Some("QuickConnectAuthorizeRequest"), None),
    ("get", "/api/v1/admin/overview", None, Some("Overview")),
    ("get", "/api/v1/admin/review", None, Some("ReviewList")),
    ("get", "/api/v1/users", None, Some("UserAdminPage")),
];

/// Schemi generati dai tipi della web UI.
pub fn schemas() -> serde_json::Map<String, Value> {
    serde_json::from_str(include_str!("openapi_schemas.json")).expect("openapi_schemas.json valido")
}

/// Aggiunge corpo e risposta tipizzati a un'operazione, se la route è nella tabella.
pub fn annotate(method: &str, path: &str, op: &mut Value) {
    let Some((_, _, req, resp)) = BODIES.iter().find(|(m, p, ..)| *m == method && *p == path) else { return };
    if let Some(r) = req {
        op["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{r}") } } } });
    }
    if let Some(r) = resp {
        op["responses"]["200"] =
            json!({ "description": "OK", "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{r}") } } } });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ogni voce indica uno schema esistente e ogni riferimento dentro gli schemi punta a uno schema esistente.
    #[test]
    fn schemas_and_refs_exist() {
        let s = schemas();
        for (m, p, req, resp) in BODIES {
            for name in [req, resp].into_iter().flatten() {
                assert!(s.contains_key(*name), "{m} {p}: schema {name} mancante");
            }
        }
        fn refs(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Object(o) => {
                    if let Some(Value::String(r)) = o.get("$ref") {
                        out.push(r.trim_start_matches("#/components/schemas/").to_string());
                    }
                    o.values().for_each(|x| refs(x, out));
                }
                Value::Array(a) => a.iter().for_each(|x| refs(x, out)),
                _ => {}
            }
        }
        let mut all = Vec::new();
        refs(&Value::Object(s.clone()), &mut all);
        for r in all {
            assert!(s.contains_key(&r), "riferimento a schema mancante: {r}");
        }
    }
}
