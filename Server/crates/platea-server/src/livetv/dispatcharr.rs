//! API REST di Dispatcharr (intestazione `X-API-Key`), usata solo per ciò che M3U e XMLTV non danno:
//! identificativi dei canali, registrazioni e regole di serie, versione. Ogni chiamata ha un tempo limite.

use super::Source;
use crate::state::Shared;
use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

fn base(s: &Source) -> String {
    s.url.trim().trim_end_matches('/').to_string()
}

async fn call(state: &Shared, s: &Source, method: reqwest::Method, path: &str, body: Option<Value>) -> anyhow::Result<Value> {
    let key = s.api_key.clone().filter(|k| !k.is_empty()).context("chiave API di Dispatcharr non impostata")?;
    let url = format!("{}{path}", base(s));
    let mut req = state.http.request(method, &url).header("X-API-Key", key).header("Accept", "application/json").timeout(Duration::from_secs(20));
    if let Some(b) = body {
        req = req.json(&b);
    }
    let r = req.send().await.with_context(|| format!("Dispatcharr non raggiungibile ({})", base(s)))?;
    let status = r.status();
    let text = r.text().await.unwrap_or_default();
    if !status.is_success() {
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"].as_str().or(v["detail"].as_str()).map(str::to_string))
            .unwrap_or_else(|| text.chars().take(200).collect());
        bail!("Dispatcharr: {status} {msg}");
    }
    Ok(if text.trim().is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap_or(Value::Null) })
}

/// Versione di Dispatcharr (prova di collegamento e della chiave).
pub async fn version(state: &Shared, s: &Source) -> anyhow::Result<String> {
    let v = call(state, s, reqwest::Method::GET, "/api/core/version/", None).await?;
    // La versione è pubblica: la chiave si verifica con una chiamata che la richiede.
    call(state, s, reqwest::Method::GET, "/api/channels/channels/?page_size=1", None).await.context("chiave API rifiutata")?;
    Ok(v["version"].as_str().unwrap_or("?").to_string())
}

/// UUID del canale → identificativo numerico (serve per le registrazioni).
pub async fn channel_ids(state: &Shared, s: &Source) -> anyhow::Result<HashMap<String, i64>> {
    let mut out = HashMap::new();
    let mut page = 1;
    loop {
        let v = call(state, s, reqwest::Method::GET, &format!("/api/channels/channels/?page_size=1000&page={page}"), None).await?;
        let list = if v.is_array() { v.as_array().cloned().unwrap_or_default() } else { v["results"].as_array().cloned().unwrap_or_default() };
        for c in &list {
            if let (Some(u), Some(id)) = (c["uuid"].as_str(), c["id"].as_i64()) {
                out.insert(u.to_string(), id);
            }
        }
        if v["next"].is_null() || list.is_empty() || page > 100 {
            break;
        }
        page += 1;
    }
    Ok(out)
}

pub fn iso(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub struct ProgramInfo<'a> {
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    pub description: Option<&'a str>,
}

/// Registrazione singola. Restituisce l'identificativo in Dispatcharr.
pub async fn create_recording(state: &Shared, s: &Source, channel_pk: i64, start_ms: i64, end_ms: i64, p: &ProgramInfo<'_>) -> anyhow::Result<String> {
    let body = json!({
        "channel": channel_pk,
        "start_time": iso(start_ms),
        "end_time": iso(end_ms),
        "custom_properties": { "program": { "title": p.title, "sub_title": p.subtitle, "description": p.description,
            "start_time": iso(start_ms), "end_time": iso(end_ms) }, "requested_by": "Platea" },
    });
    let v = call(state, s, reqwest::Method::POST, "/api/channels/recordings/", Some(body)).await?;
    v["id"].as_i64().map(|i| i.to_string()).context("Dispatcharr non ha restituito la registrazione")
}

pub async fn delete_recording(state: &Shared, s: &Source, id: &str) -> anyhow::Result<()> {
    match call(state, s, reqwest::Method::DELETE, &format!("/api/channels/recordings/{id}/"), None).await {
        Ok(_) => Ok(()),
        // Già cancellata in Dispatcharr: va bene.
        Err(e) if e.to_string().contains("404") => Ok(()),
        Err(e) => Err(e),
    }
}

/// Ferma una registrazione in corso tenendo la parte registrata.
pub async fn stop_recording(state: &Shared, s: &Source, id: &str) -> anyhow::Result<()> {
    call(state, s, reqwest::Method::POST, &format!("/api/channels/recordings/{id}/stop/"), None).await.map(|_| ())
}

/// Stato delle registrazioni in Dispatcharr: id → (stato, percorso del file).
pub async fn recordings(state: &Shared, s: &Source) -> anyhow::Result<HashMap<String, (String, Option<String>, i64, i64, Option<i64>, Option<String>)>> {
    let v = call(state, s, reqwest::Method::GET, "/api/channels/recordings/?page_size=1000", None).await?;
    let list = if v.is_array() { v.as_array().cloned().unwrap_or_default() } else { v["results"].as_array().cloned().unwrap_or_default() };
    let mut out = HashMap::new();
    for r in list {
        let Some(id) = r["id"].as_i64() else { continue };
        let cp = &r["custom_properties"];
        let status = cp["status"].as_str().unwrap_or("scheduled").to_string();
        let file = cp["file_path"].as_str().map(str::to_string);
        let start = parse_iso(r["start_time"].as_str().unwrap_or("")).unwrap_or(0);
        let end = parse_iso(r["end_time"].as_str().unwrap_or("")).unwrap_or(0);
        let title = cp["program"]["title"].as_str().map(str::to_string);
        out.insert(id.to_string(), (status, file, start, end, r["channel"].as_i64(), title));
    }
    Ok(out)
}

/// `2026-11-18T22:00:00Z`, `2026-11-18T22:00:00+01:00`, con o senza frazioni di secondo → ms.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let compact: String = s[..19].chars().filter(|c| c.is_ascii_digit()).collect();
    let mut ms = super::xmltv::parse_time(&compact)?;
    let rest = &s[19..];
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if let Some(off) = rest.strip_prefix('+').map(|o| (1, o)).or_else(|| rest.strip_prefix('-').map(|o| (-1, o))) {
        let (sign, o) = off;
        let o: String = o.chars().filter(|c| c.is_ascii_digit()).collect();
        if o.len() >= 4 {
            let h: i64 = o[0..2].parse().ok()?;
            let m: i64 = o[2..4].parse().ok()?;
            ms -= sign * (h * 60 + m) * 60_000;
        }
    }
    Some(ms)
}

/// Regola di serie: tutte le puntate (o solo le nuove) con questo titolo sul canale.
pub async fn create_series_rule(state: &Shared, s: &Source, channel_pk: i64, title: &str, only_new: bool) -> anyhow::Result<()> {
    let body = json!({ "mode": if only_new { "new" } else { "all" }, "title": title, "title_mode": "exact", "channel_id": channel_pk });
    call(state, s, reqwest::Method::POST, "/api/channels/series-rules/", Some(body)).await?;
    // Le puntate già in guida si programmano subito.
    let _ = call(state, s, reqwest::Method::POST, "/api/channels/series-rules/evaluate/", Some(json!({}))).await;
    Ok(())
}

pub async fn delete_series_rule(state: &Shared, s: &Source, title: &str) -> anyhow::Result<()> {
    call(state, s, reqwest::Method::DELETE, &format!("/api/channels/series-rules/?tvg_id=&title={}", urlencoding::encode(title)), None).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_roundtrip() {
        let t = super::super::xmltv::parse_time("20261118223015 +0000").unwrap();
        assert_eq!(iso(t), "2026-11-18T22:30:15Z");
        assert_eq!(parse_iso("2026-11-18T22:30:15Z"), Some(t));
        assert_eq!(parse_iso("2026-11-18T23:30:15.123+01:00"), Some(t));
        assert_eq!(parse_iso("2026-11-18T20:30:15-02:00"), Some(t));
    }
}
