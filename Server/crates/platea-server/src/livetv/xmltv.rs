//! Guida XMLTV letta in streaming (le guide vere arrivano a centinaia di MB): si tengono solo i programmi dei canali
//! noti e dentro la finestra di giorni richiesta. Gzip riconosciuto dai primi byte.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct XmlChannel {
    pub id: String,
    pub name: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct XmlProgram {
    pub channel: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub icon: Option<String>,
    pub is_new: bool,
    pub rating: Option<String>,
}

/// `20261118220000 +0100` (anche senza fuso, senza secondi) → ms Unix.
pub fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let (dt, tz) = match s.split_once(' ') {
        Some((a, b)) => (a, b.trim()),
        None => (s, ""),
    };
    let digits: String = dt.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 12 {
        return None;
    }
    let n = |a: usize, b: usize| digits.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (y, mo, d, h, mi) = (n(0, 4)?, n(4, 6)?, n(6, 8)?, n(8, 10)?, n(10, 12)?);
    let sec = n(12, 14).unwrap_or(0);
    let days = days_from_civil(y, mo, d);
    let mut ms = ((days * 24 + h) * 60 + mi) * 60_000 + sec * 1000;
    if tz.len() >= 5 && (tz.starts_with('+') || tz.starts_with('-')) {
        let sign = if tz.starts_with('-') { -1 } else { 1 };
        let hh: i64 = tz[1..3].parse().ok()?;
        let mm: i64 = tz[3..5].parse().ok()?;
        ms -= sign * (hh * 60 + mm) * 60_000;
    }
    Some(ms)
}

/// Giorni dal 1970-01-01 (algoritmo di Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `episode-num`: `xmltv_ns` ("1.4." = stagione 2, episodio 5, base zero) oppure "S02E05".
fn episode(system: &str, v: &str) -> (Option<i64>, Option<i64>) {
    if system == "xmltv_ns" {
        let mut p = v.split('.');
        let num =
            |x: Option<&str>| x.and_then(|x| x.split('/').next()).map(str::trim).filter(|x| !x.is_empty()).and_then(|x| x.parse::<i64>().ok()).map(|n| n + 1);
        return (num(p.next()), num(p.next()));
    }
    let up = v.to_ascii_uppercase();
    if let (Some(si), Some(ei)) = (up.find('S'), up.find('E')) {
        let s: String = up[si + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
        let e: String = up[ei + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
        return (s.parse().ok(), e.parse().ok());
    }
    (None, None)
}

/// Apre un file XMLTV, compresso o no.
pub fn open(path: &std::path::Path) -> std::io::Result<Box<dyn BufRead>> {
    let mut f = std::fs::File::open(path)?;
    let mut magic = [0u8; 2];
    let n = f.read(&mut magic)?;
    drop(f);
    let f = std::fs::File::open(path)?;
    if n == 2 && magic == [0x1f, 0x8b] {
        Ok(Box::new(BufReader::new(flate2::read::MultiGzDecoder::new(f))))
    } else {
        Ok(Box::new(BufReader::with_capacity(256 * 1024, f)))
    }
}

fn attr(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == name).and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// Legge canali e programmi. `keep`: canali di cui tenere i programmi (vuoto = tutti); finestra in ms.
pub fn parse<R: BufRead>(input: R, keep: &HashSet<String>, from_ms: i64, to_ms: i64) -> anyhow::Result<(Vec<XmlChannel>, Vec<XmlProgram>)> {
    let mut r = Reader::from_reader(input);
    r.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut channels = Vec::new();
    let mut programs = Vec::new();
    let mut cur_ch: Option<XmlChannel> = None;
    let mut cur_pr: Option<XmlProgram> = None;
    let mut skip_pr = false;
    let mut ep_system = String::new();
    let mut text = String::new();
    loop {
        match r.read_event_into(&mut buf)? {
            ev @ (Event::Start(_) | Event::Empty(_))
                if matches!(&ev, Event::Start(e) | Event::Empty(e) if matches!(e.name().as_ref(), b"channel" | b"programme"))
                    && cur_ch.is_none()
                    && cur_pr.is_none() =>
            {
                let (e, empty) = match ev {
                    Event::Start(e) => (e, false),
                    Event::Empty(e) => (e, true),
                    _ => unreachable!(),
                };
                if e.name().as_ref() == b"channel" {
                    let c = XmlChannel { id: attr(&e, b"id").unwrap_or_default(), ..Default::default() };
                    if empty {
                        channels.push(c);
                    } else {
                        cur_ch = Some(c);
                    }
                } else if !empty {
                    let ch = attr(&e, b"channel").unwrap_or_default();
                    let start = attr(&e, b"start").and_then(|s| parse_time(&s));
                    let stop = attr(&e, b"stop").and_then(|s| parse_time(&s));
                    skip_pr = (!keep.is_empty() && !keep.contains(&ch)) || start.is_none();
                    let (start, stop) = (start.unwrap_or(0), stop.unwrap_or(0));
                    if stop != 0 && (stop < from_ms || start > to_ms) {
                        skip_pr = true;
                    }
                    cur_pr = Some(XmlProgram { channel: ch, start_ms: start, end_ms: stop, ..Default::default() });
                }
            }
            Event::Start(e) => {
                let field = e.name().as_ref().to_vec();
                text.clear();
                if field == b"episode-num" {
                    ep_system = attr(&e, b"system").unwrap_or_default();
                }
                if field == b"new" {
                    if let Some(p) = cur_pr.as_mut() {
                        p.is_new = true;
                    }
                }
            }
            Event::Empty(e) => match e.name().as_ref() {
                b"icon" => {
                    let src = attr(&e, b"src");
                    if let Some(c) = cur_ch.as_mut() {
                        c.icon = c.icon.take().or(src);
                    } else if let Some(p) = cur_pr.as_mut() {
                        p.icon = p.icon.take().or(src);
                    }
                }
                b"new" => {
                    if let Some(p) = cur_pr.as_mut() {
                        p.is_new = true;
                    }
                }
                _ => {}
            },
            Event::Text(t) => {
                if !skip_pr || cur_ch.is_some() {
                    if let Ok(s) = t.unescape() {
                        text.push_str(&s);
                    }
                }
            }
            Event::CData(t) => text.push_str(&String::from_utf8_lossy(&t)),
            Event::End(e) => {
                let name = e.name().as_ref().to_vec();
                match name.as_slice() {
                    b"channel" => {
                        if let Some(c) = cur_ch.take() {
                            channels.push(c);
                        }
                    }
                    b"programme" => {
                        if let Some(mut p) = cur_pr.take() {
                            if !skip_pr && !p.title.is_empty() {
                                if p.end_ms == 0 {
                                    p.end_ms = p.start_ms + 30 * 60_000;
                                }
                                programs.push(p);
                            }
                        }
                        skip_pr = false;
                    }
                    _ => {
                        let v = text.trim().to_string();
                        if let Some(c) = cur_ch.as_mut() {
                            if name == b"display-name" && c.name.is_none() && !v.is_empty() {
                                c.name = Some(v);
                            }
                        } else if let (Some(p), false) = (cur_pr.as_mut(), skip_pr) {
                            if !v.is_empty() {
                                match name.as_slice() {
                                    b"title" if p.title.is_empty() => p.title = v,
                                    b"sub-title" if p.subtitle.is_none() => p.subtitle = Some(v),
                                    b"desc" if p.description.is_none() => p.description = Some(v),
                                    b"category" if p.category.is_none() => p.category = Some(v),
                                    b"episode-num" if p.episode.is_none() => {
                                        let (s, e) = episode(&ep_system, &v);
                                        p.season = p.season.or(s);
                                        p.episode = e;
                                    }
                                    b"value" if p.rating.is_none() => p.rating = Some(v),
                                    _ => {}
                                }
                            }
                        }
                        text.clear();
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    // Programmi sovrapposti dello stesso canale con lo stesso inizio: vale l'ultimo.
    let mut seen: HashMap<(String, i64), usize> = HashMap::new();
    let mut dedup: Vec<XmlProgram> = Vec::with_capacity(programs.len());
    for p in programs {
        match seen.get(&(p.channel.clone(), p.start_ms)) {
            Some(i) => dedup[*i] = p,
            None => {
                seen.insert((p.channel.clone(), p.start_ms), dedup.len());
                dedup.push(p);
            }
        }
    }
    Ok((channels, dedup))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse_time("19700101000000 +0000"), Some(0));
        assert_eq!(parse_time("19700101010000 +0100"), Some(0));
        assert_eq!(parse_time("20261118220000 -0200"), parse_time("20261119000000 +0000"));
        assert_eq!(parse_time("202611182200"), parse_time("20261118220000 +0000"));
        assert_eq!(parse_time("x"), None);
    }

    #[test]
    fn guide() {
        let x = r#"<?xml version="1.0"?><tv>
          <channel id="a"><display-name>Uno</display-name><icon src="http://l/1.png"/></channel>
          <channel id="b"><display-name>Due</display-name></channel>
          <programme start="20261118200000 +0000" stop="20261118210000 +0000" channel="a">
            <title lang="it">Il &amp; telegiornale</title><sub-title>Edizione</sub-title><desc>Notizie</desc><category>News</category>
            <episode-num system="xmltv_ns">1.4.</episode-num><icon src="http://i/p.jpg"/><new/><rating><value>T</value></rating>
          </programme>
          <programme start="20261118200000 +0000" stop="20261118210000 +0000" channel="b"><title>Altro</title></programme>
          <programme start="20201118200000 +0000" stop="20201118210000 +0000" channel="a"><title>Vecchio</title></programme>
        </tv>"#;
        let keep: HashSet<String> = ["a".to_string()].into();
        let (ch, pr) = parse(x.as_bytes(), &keep, parse_time("20261118000000").unwrap(), parse_time("20261120000000").unwrap()).unwrap();
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[0].icon.as_deref(), Some("http://l/1.png"));
        assert_eq!(pr.len(), 1);
        let p = &pr[0];
        assert_eq!(p.title, "Il & telegiornale");
        assert_eq!((p.season, p.episode), (Some(2), Some(5)));
        assert_eq!(p.subtitle.as_deref(), Some("Edizione"));
        assert_eq!(p.icon.as_deref(), Some("http://i/p.jpg"));
        assert!(p.is_new);
        assert_eq!(p.rating.as_deref(), Some("T"));
        assert_eq!(p.end_ms - p.start_ms, 3_600_000);
    }

    #[test]
    fn onscreen_episodes() {
        assert_eq!(episode("onscreen", "S03E12"), (Some(3), Some(12)));
        assert_eq!(episode("xmltv_ns", ".7/10."), (None, Some(8)));
    }
}
