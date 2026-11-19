//! Live TV. Platea non gestisce i fornitori IPTV: legge canali e guida da una sorgente (Dispatcharr consigliato,
//! oppure M3U + XMLTV, HDHomeRun, Xtream Codes), mostra la guida, fa vedere i canali nel browser trasformando il
//! flusso MPEG-TS in HLS (un solo FFmpeg per canale, condiviso) e affida le registrazioni a Dispatcharr.

pub mod dispatcharr;
pub mod m3u;
pub mod stream;
pub mod xmltv;

use crate::jobs::{self, priority, JobKind};
use crate::state::Shared;
use crate::util::now_ms;
use anyhow::{bail, Context};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;
/// Liste e guide più grandi di così non sono una lista o una guida.
const MAX_DOWNLOAD: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Source {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub url: String,
    pub epg_url: Option<String>,
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub channel_profile: Option<String>,
    pub output_profile: Option<i64>,
    pub recordings_path: Option<String>,
    pub max_streams: Option<i64>,
    pub refresh_hours: i64,
    pub epg_refresh_hours: i64,
    pub epg_days: i64,
    pub enabled: i64,
    pub last_refresh_at: Option<i64>,
    pub last_epg_at: Option<i64>,
}

pub const SOURCE_COLS: &str = "id, kind, name, url, epg_url, api_key, username, password, channel_profile, output_profile, recordings_path, max_streams, refresh_hours, epg_refresh_hours, epg_days, enabled, last_refresh_at, last_epg_at";

pub async fn load_source(state: &Shared, id: i64) -> anyhow::Result<Option<Source>> {
    Ok(sqlx::query_as(&format!("SELECT {SOURCE_COLS} FROM tv_sources WHERE id = ?")).bind(id).fetch_optional(&state.db.read).await?)
}

/// Un canale come lo descrive la sorgente.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fetched {
    pub remote_id: String,
    pub epg_id: Option<String>,
    pub number: Option<f64>,
    pub name: String,
    pub group: Option<String>,
    pub logo: Option<String>,
    pub stream_url: String,
    pub radio: bool,
    pub catchup_days: i64,
    pub remote_pk: Option<i64>,
}

fn base(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// Schema, host e porta di un indirizzo.
fn origin(url: &str) -> Option<String> {
    let u = reqwest::Url::parse(url).ok()?;
    let mut o = format!("{}://{}", u.scheme(), u.host_str()?);
    if let Some(p) = u.port() {
        o.push_str(&format!(":{p}"));
    }
    Some(o)
}

/// Dispatcharr scrive gli indirizzi con l'host con cui è stato chiamato (o quello configurato): li riporta
/// all'indirizzo della sorgente, l'unico che Platea sa di poter raggiungere.
fn rebase(url: &str, source_base: &str, paths: &[&str]) -> String {
    let (Some(o), Ok(u)) = (origin(source_base), reqwest::Url::parse(url)) else { return url.to_string() };
    if !paths.iter().any(|p| u.path().starts_with(p)) {
        return url.to_string();
    }
    let mut out = format!("{o}{}", u.path());
    if let Some(q) = u.query() {
        out.push('?');
        out.push_str(q);
    }
    out
}

/// Scarica in memoria un testo (lista, JSON) con un tetto.
async fn fetch_text(state: &Shared, url: &str, headers: &[(&str, String)], limit: u64) -> anyhow::Result<String> {
    if let Some(path) = url.strip_prefix("file://").or_else(|| url.starts_with('/').then_some(url)) {
        let meta = tokio::fs::metadata(path).await.with_context(|| path.to_string())?;
        if meta.len() > limit {
            bail!("file troppo grande");
        }
        return Ok(tokio::fs::read_to_string(path).await?);
    }
    let mut req = state.http.get(url).timeout(Duration::from_secs(120));
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let mut resp = req.send().await.with_context(|| format!("richiesta a {}", redact(url)))?;
    if !resp.status().is_success() {
        bail!("{} ha risposto {}", redact(url), resp.status());
    }
    let mut buf = Vec::new();
    while let Some(c) = resp.chunk().await? {
        if (buf.len() + c.len()) as u64 > limit {
            bail!("risposta troppo grande");
        }
        buf.extend_from_slice(&c);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Scarica in un file (le guide possono essere enormi): mai tutta in memoria.
async fn fetch_file(state: &Shared, url: &str, headers: &[(&str, String)], target: &std::path::Path) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    if let Some(d) = target.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    if let Some(path) = url.strip_prefix("file://").or_else(|| url.starts_with('/').then_some(url)) {
        tokio::fs::copy(path, target).await.with_context(|| format!("guida {path}"))?;
        return Ok(());
    }
    let mut req = state.http.get(url).timeout(Duration::from_secs(600));
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let mut resp = req.send().await.with_context(|| format!("richiesta a {}", redact(url)))?;
    if !resp.status().is_success() {
        bail!("{} ha risposto {}", redact(url), resp.status());
    }
    let mut f = tokio::fs::File::create(target).await?;
    let mut n = 0u64;
    while let Some(c) = resp.chunk().await? {
        n += c.len() as u64;
        if n > MAX_DOWNLOAD {
            bail!("guida troppo grande");
        }
        f.write_all(&c).await?;
    }
    f.flush().await?;
    Ok(())
}

/// Indirizzo senza credenziali (Xtream le mette nella query): per i log e i messaggi.
pub fn redact(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(mut u) => {
            let _ = u.set_password(None);
            let had_query = u.query().is_some();
            u.set_query(None);
            format!("{u}{}", if had_query { "?…" } else { "" })
        }
        Err(_) => url.chars().take(80).collect(),
    }
}

fn key_headers(s: &Source) -> Vec<(&'static str, String)> {
    match (&s.kind[..], &s.api_key) {
        ("dispatcharr", Some(k)) if !k.is_empty() => vec![("X-API-Key", k.clone())],
        _ => vec![],
    }
}

/// Indirizzo della guida della sorgente.
pub fn epg_url(s: &Source) -> Option<String> {
    if let Some(u) = s.epg_url.as_ref().filter(|u| !u.trim().is_empty()) {
        return Some(u.trim().to_string());
    }
    let b = base(&s.url);
    match s.kind.as_str() {
        "dispatcharr" => {
            let p = s.channel_profile.as_ref().filter(|p| !p.is_empty()).map(|p| format!("/{}", urlencoding::encode(p))).unwrap_or_default();
            Some(format!("{b}/output/epg{p}?days={}&prev_days=1", s.epg_days.clamp(1, 14)))
        }
        "xtream" => Some(format!(
            "{b}/xmltv.php?username={}&password={}",
            urlencoding::encode(s.username.as_deref().unwrap_or("")),
            urlencoding::encode(s.password.as_deref().unwrap_or(""))
        )),
        _ => None,
    }
}

/// Canali della sorgente.
pub async fn fetch_channels(state: &Shared, s: &Source) -> anyhow::Result<Vec<Fetched>> {
    let b = base(&s.url);
    let headers = key_headers(s);
    match s.kind.as_str() {
        "dispatcharr" => {
            let p = s.channel_profile.as_ref().filter(|p| !p.is_empty()).map(|p| format!("/{}", urlencoding::encode(p))).unwrap_or_default();
            let mut url = format!("{b}/output/m3u{p}?cachedlogos=true");
            if let Some(o) = s.output_profile {
                url.push_str(&format!("&output_profile={o}"));
            }
            let text = fetch_text(state, &url, &headers, 64 * 1024 * 1024).await?;
            let pks = if s.api_key.as_ref().is_some_and(|k| !k.is_empty()) {
                dispatcharr::channel_ids(state, s).await.unwrap_or_else(|e| {
                    tracing::info!(source = %s.name, error = %e, "Dispatcharr: identificativi dei canali non letti (registrazioni non disponibili)");
                    HashMap::new()
                })
            } else {
                HashMap::new()
            };
            Ok(m3u::parse(&text)
                .into_iter()
                .map(|e| {
                    let stream = rebase(&e.url, &b, &["/proxy/", "/live/"]);
                    // L'UUID del canale è nell'ultimo pezzo dell'indirizzo del flusso.
                    let uuid = reqwest::Url::parse(&stream)
                        .ok()
                        .and_then(|u| u.path_segments().and_then(|mut s| s.next_back().map(str::to_string)))
                        .filter(|x| x.len() >= 32);
                    let remote_id = uuid.clone().or(e.tvg_id.clone()).unwrap_or_else(|| e.name.clone());
                    Fetched {
                        remote_pk: uuid.as_ref().and_then(|u| pks.get(u).copied()),
                        remote_id,
                        epg_id: e.tvg_id.clone(),
                        number: e.number,
                        name: e.name,
                        group: e.group,
                        logo: e.logo.map(|l| rebase(&l, &b, &["/api/channels/logos/"])),
                        stream_url: stream,
                        radio: e.radio,
                        catchup_days: e.catchup_days,
                    }
                })
                .collect())
        }
        "m3u" => {
            let text = fetch_text(state, &s.url, &headers, 64 * 1024 * 1024).await?;
            Ok(m3u::parse(&text)
                .into_iter()
                .map(|e| Fetched {
                    remote_id: e.tvg_id.clone().filter(|t| !t.is_empty()).map(|t| format!("{t}|{}", e.name)).unwrap_or_else(|| e.url.clone()),
                    epg_id: e.tvg_id.clone().or(e.tvg_name.clone()),
                    number: e.number,
                    name: e.name,
                    group: e.group,
                    logo: e.logo,
                    stream_url: e.url,
                    radio: e.radio,
                    catchup_days: e.catchup_days,
                    remote_pk: None,
                })
                .collect())
        }
        "hdhomerun" => {
            let text = fetch_text(state, &format!("{b}/lineup.json"), &headers, 16 * 1024 * 1024).await?;
            let v: serde_json::Value = serde_json::from_str(&text).context("lineup.json non valido")?;
            Ok(v.as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| {
                    let num = c["GuideNumber"].as_str().map(str::to_string).or_else(|| c["GuideNumber"].as_f64().map(|n| n.to_string()))?;
                    let url = c["URL"].as_str()?.to_string();
                    Some(Fetched {
                        remote_id: num.clone(),
                        epg_id: c["GuideID"].as_str().map(str::to_string).or(Some(num.clone())),
                        number: num.parse().ok(),
                        name: c["GuideName"].as_str().unwrap_or(&num).to_string(),
                        group: if c["HD"].as_i64() == Some(1) { Some("HD".into()) } else { None },
                        logo: None,
                        stream_url: rebase(&url, &b, &["/auto/", "/proxy/", "/hdhr/"]),
                        radio: false,
                        catchup_days: 0,
                        remote_pk: None,
                    })
                })
                .collect())
        }
        "xtream" => {
            let auth = format!(
                "username={}&password={}",
                urlencoding::encode(s.username.as_deref().unwrap_or("")),
                urlencoding::encode(s.password.as_deref().unwrap_or(""))
            );
            let cats = fetch_text(state, &format!("{b}/player_api.php?{auth}&action=get_live_categories"), &[], 16 * 1024 * 1024).await?;
            let cats: serde_json::Value = serde_json::from_str(&cats).context("Xtream: categorie non valide (credenziali?)")?;
            let names: HashMap<String, String> =
                cats.as_array().into_iter().flatten().filter_map(|c| Some((json_str(&c["category_id"])?, c["category_name"].as_str()?.to_string()))).collect();
            let list = fetch_text(state, &format!("{b}/player_api.php?{auth}&action=get_live_streams"), &[], 128 * 1024 * 1024).await?;
            let list: serde_json::Value = serde_json::from_str(&list).context("Xtream: canali non validi")?;
            let (u, p) = (s.username.clone().unwrap_or_default(), s.password.clone().unwrap_or_default());
            Ok(list
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| {
                    let id = json_str(&c["stream_id"])?;
                    Some(Fetched {
                        remote_id: id.clone(),
                        epg_id: c["epg_channel_id"].as_str().filter(|x| !x.is_empty()).map(str::to_string),
                        number: c["num"].as_f64().or_else(|| c["num"].as_str().and_then(|x| x.parse().ok())),
                        name: c["name"].as_str()?.to_string(),
                        group: json_str(&c["category_id"]).and_then(|k| names.get(&k).cloned()),
                        logo: c["stream_icon"].as_str().filter(|x| !x.is_empty()).map(str::to_string),
                        stream_url: format!("{b}/live/{}/{}/{id}.ts", urlencoding::encode(&u), urlencoding::encode(&p)),
                        radio: c["stream_type"].as_str() == Some("radio_streams"),
                        catchup_days: if json_str(&c["tv_archive"]).as_deref() == Some("1") {
                            json_str(&c["tv_archive_duration"]).and_then(|d| d.parse().ok()).unwrap_or(0)
                        } else {
                            0
                        },
                        remote_pk: None,
                    })
                })
                .collect())
        }
        k => bail!("tipo di sorgente sconosciuto: {k}"),
    }
}

fn json_str(v: &serde_json::Value) -> Option<String> {
    v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string()))
}

/// Salva i canali: nuovi inseriti, noti aggiornati (le scelte dell'amministratore restano), spariti segnati assenti.
pub async fn store_channels(state: &Shared, source_id: i64, list: &[Fetched]) -> anyhow::Result<usize> {
    let now = now_ms();
    let mut tx = state.db.write.begin().await?;
    let mut seen = HashSet::new();
    for c in list {
        if !seen.insert(c.remote_id.clone()) {
            continue;
        }
        sqlx::query(
            "INSERT INTO tv_channels (source_id, remote_id, epg_id, number, name, group_name, logo_url, stream_url, radio, catchup_days, remote_pk, present, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)
             ON CONFLICT(source_id, remote_id) DO UPDATE SET epg_id = excluded.epg_id, number = excluded.number, name = excluded.name,
               group_name = excluded.group_name, logo_url = excluded.logo_url, stream_url = excluded.stream_url, radio = excluded.radio,
               catchup_days = excluded.catchup_days, remote_pk = COALESCE(excluded.remote_pk, tv_channels.remote_pk), present = 1,
               updated_at = excluded.updated_at",
        )
        .bind(source_id)
        .bind(&c.remote_id)
        .bind(&c.epg_id)
        .bind(c.number)
        .bind(&c.name)
        .bind(&c.group)
        .bind(&c.logo)
        .bind(&c.stream_url)
        .bind(c.radio as i64)
        .bind(c.catchup_days)
        .bind(c.remote_pk)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    let gone = sqlx::query("UPDATE tv_channels SET present = 0 WHERE source_id = ? AND updated_at < ? AND present = 1")
        .bind(source_id)
        .bind(now)
        .execute(&mut *tx)
        .await?
        .rows_affected() as usize;
    // Spariti da più di 30 giorni: via, con la loro guida.
    sqlx::query("DELETE FROM tv_channels WHERE source_id = ? AND present = 0 AND updated_at < ?")
        .bind(source_id)
        .bind(now - 30 * DAY)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(gone)
}

/// Guida: scarica, legge in streaming, sostituisce i programmi della finestra per i canali della sorgente.
pub async fn refresh_epg(state: &Shared, s: &Source) -> anyhow::Result<usize> {
    let Some(url) = epg_url(s) else { return Ok(0) };
    let channels: Vec<(i64, Option<String>, String)> =
        sqlx::query_as("SELECT id, epg_id, name FROM tv_channels WHERE source_id = ? AND present = 1").bind(s.id).fetch_all(&state.db.read).await?;
    if channels.is_empty() {
        return Ok(0);
    }
    let mut by_epg: HashMap<String, Vec<i64>> = HashMap::new();
    for (id, epg, _) in &channels {
        if let Some(e) = epg.as_ref().filter(|e| !e.is_empty()) {
            by_epg.entry(e.clone()).or_default().push(*id);
        }
    }
    let tmp = state.boot.cache_dir.join("livetv").join(format!("epg-{}.xml", s.id));
    fetch_file(state, &url, &key_headers(s), &tmp).await?;
    let now = now_ms();
    let from = now - DAY;
    let to = now + s.epg_days.clamp(1, 14) * DAY;
    let keep: HashSet<String> = by_epg.keys().cloned().collect();
    let path = tmp.clone();
    let (xml_channels, programs) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> { xmltv::parse(xmltv::open(&path)?, &keep, from, to) }).await??;
    let _ = tokio::fs::remove_file(&tmp).await;
    // Canali della guida senza corrispondenza per id: si prova per nome (liste M3U senza tvg-id).
    let mut matched = by_epg.clone();
    if matched.len() < channels.len() {
        let by_name: HashMap<String, Vec<i64>> = channels.iter().fold(HashMap::new(), |mut m, (id, epg, name)| {
            if epg.as_ref().is_none_or(|e| !by_epg.contains_key(e)) {
                m.entry(platea_naming::normalize_title(name)).or_default().push(*id);
            }
            m
        });
        let mut extra: HashSet<String> = HashSet::new();
        for xc in &xml_channels {
            if let Some(ids) = xc.name.as_ref().and_then(|n| by_name.get(&platea_naming::normalize_title(n))) {
                matched.entry(xc.id.clone()).or_default().extend(ids.iter().copied());
                extra.insert(xc.id.clone());
            }
        }
        // I programmi di questi canali non erano stati tenuti: seconda lettura solo se ce ne sono.
        if !extra.is_empty() {
            let tmp2 = state.boot.cache_dir.join("livetv").join(format!("epg-{}-b.xml", s.id));
            fetch_file(state, &url, &key_headers(s), &tmp2).await?;
            let p2 = tmp2.clone();
            let (_, more) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> { xmltv::parse(xmltv::open(&p2)?, &extra, from, to) }).await??;
            let _ = tokio::fs::remove_file(&tmp2).await;
            return save_programs(state, s.id, &matched, programs.into_iter().chain(more).collect(), from, to).await;
        }
    }
    save_programs(state, s.id, &matched, programs, from, to).await
}

async fn save_programs(
    state: &Shared,
    source_id: i64,
    by_epg: &HashMap<String, Vec<i64>>,
    programs: Vec<xmltv::XmlProgram>,
    from: i64,
    to: i64,
) -> anyhow::Result<usize> {
    let mut tx = state.db.write.begin().await?;
    sqlx::query("DELETE FROM tv_programs WHERE channel_id IN (SELECT id FROM tv_channels WHERE source_id = ?) AND end_ms >= ? AND start_ms <= ?")
        .bind(source_id)
        .bind(from)
        .bind(to)
        .execute(&mut *tx)
        .await?;
    let mut n = 0;
    for p in &programs {
        let Some(ids) = by_epg.get(&p.channel) else { continue };
        for cid in ids {
            sqlx::query(
                "INSERT OR REPLACE INTO tv_programs (channel_id, start_ms, end_ms, title, subtitle, description, category, season, episode, image_url, is_new, rating)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(cid)
            .bind(p.start_ms)
            .bind(p.end_ms)
            .bind(&p.title)
            .bind(&p.subtitle)
            .bind(&p.description)
            .bind(&p.category)
            .bind(p.season)
            .bind(p.episode)
            .bind(&p.icon)
            .bind(p.is_new as i64)
            .bind(&p.rating)
            .execute(&mut *tx)
            .await?;
            n += 1;
        }
    }
    tx.commit().await?;
    Ok(n)
}

/// Lavoro in coda: canali e, se richiesto o scaduta, guida.
pub async fn refresh_source(state: &Shared, source_id: i64, epg: bool) -> anyhow::Result<()> {
    let Some(s) = load_source(state, source_id).await? else { return Ok(()) };
    let t0 = std::time::Instant::now();
    let result: anyhow::Result<String> = async {
        let list = fetch_channels(state, &s).await?;
        if list.is_empty() {
            bail!("la sorgente non ha restituito canali");
        }
        let gone = store_channels(state, s.id, &list).await?;
        sqlx::query("UPDATE tv_sources SET last_refresh_at = ? WHERE id = ?").bind(now_ms()).bind(s.id).execute(&state.db.write).await?;
        let mut msg = format!("{} canali", list.len());
        if gone > 0 {
            msg.push_str(&format!(", {gone} non più presenti"));
        }
        let epg_due = epg || s.last_epg_at.is_none_or(|t| now_ms() - t >= s.epg_refresh_hours.max(1) * HOUR);
        if epg_due {
            match refresh_epg(state, &s).await {
                Ok(n) => {
                    sqlx::query("UPDATE tv_sources SET last_epg_at = ? WHERE id = ?").bind(now_ms()).bind(s.id).execute(&state.db.write).await?;
                    msg.push_str(&format!(", {n} programmi in guida"));
                }
                Err(e) => msg.push_str(&format!(", guida non aggiornata: {e:#}")),
            }
        }
        Ok(msg)
    }
    .await;
    let (status, message) = match &result {
        Ok(m) => ("ok", format!("{m} · {} ms", t0.elapsed().as_millis())),
        Err(e) => ("error", format!("{e:#}")),
    };
    sqlx::query("UPDATE tv_sources SET last_status = ?, last_message = ? WHERE id = ?").bind(status).bind(&message).bind(s.id).execute(&state.db.write).await?;
    tracing::info!(source = %s.name, status, %message, "Live TV: sorgente aggiornata");
    state.events.publish("livetv.updated", serde_json::json!({ "source_id": state.ids.encode(s.id) }));
    result.map(|_| ())
}

/// Ogni minuto: sorgenti da aggiornare e pulizia della guida passata.
pub async fn schedule(state: &Shared) -> anyhow::Result<()> {
    let rows: Vec<(i64, i64, Option<i64>, i64, Option<i64>)> =
        sqlx::query_as("SELECT id, refresh_hours, last_refresh_at, epg_refresh_hours, last_epg_at FROM tv_sources WHERE enabled = 1")
            .fetch_all(&state.db.read)
            .await?;
    let now = now_ms();
    for (id, rh, last, eh, last_epg) in rows {
        let ch_due = last.is_none_or(|t| now - t >= rh.max(1) * HOUR);
        let epg_due = last_epg.is_none_or(|t| now - t >= eh.max(1) * HOUR);
        if ch_due || epg_due {
            jobs::enqueue(state, JobKind::RefreshTvSource { source_id: id, epg: epg_due }, priority::MAINTENANCE, None).await?;
        }
    }
    // Programmi finiti da più di 2 giorni (o oltre il catch-up): non servono più.
    sqlx::query(
        "DELETE FROM tv_programs WHERE end_ms < ?1 - 2 * 86400000
           AND end_ms < ?1 - 86400000 * (SELECT MAX(1, catchup_days) FROM tv_channels c WHERE c.id = tv_programs.channel_id)",
    )
    .bind(now)
    .execute(&state.db.write)
    .await?;
    Ok(())
}

/// Filtro SQL sui canali visibili all'utente (gruppi consentiti, nascosti, assenti, sorgenti spente).
pub fn channel_filter(user: &crate::auth::AuthUser, alias: &str) -> (String, Vec<String>) {
    let mut sql = format!(" AND {alias}.hidden = 0 AND {alias}.present = 1 AND {alias}.source_id IN (SELECT id FROM tv_sources WHERE enabled = 1)");
    let groups = user.perms.livetv_groups.clone();
    if !user.is_admin && !groups.is_empty() {
        sql.push_str(&format!(" AND COALESCE({alias}.group_name, '') IN ({})", vec!["?"; groups.len()].join(",")));
        return (sql, groups);
    }
    (sql, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebase_keeps_path() {
        assert_eq!(
            rebase("http://127.0.0.1:9191/proxy/ts/stream/abc?x=1", "http://dispatcharr:9191/", &["/proxy/"]),
            "http://dispatcharr:9191/proxy/ts/stream/abc?x=1"
        );
        assert_eq!(rebase("http://cdn.example/logo.png", "http://dispatcharr:9191", &["/proxy/"]), "http://cdn.example/logo.png");
    }

    #[test]
    fn redacts_credentials() {
        assert_eq!(redact("http://u:p@host/xmltv.php?username=a&password=b"), "http://u@host/xmltv.php?…");
    }
}
