//! Liste M3U estese (IPTV): `#EXTINF:-1 tvg-id=".." tvg-name=".." tvg-logo=".." tvg-chno=".." group-title="..",Nome`
//! seguito dall'indirizzo del flusso. Attributi di Dispatcharr compresi (`catchup-days`, `radio`).

#[derive(Debug, Clone, Default, PartialEq)]
pub struct M3uEntry {
    pub name: String,
    pub url: String,
    pub tvg_id: Option<String>,
    pub tvg_name: Option<String>,
    pub logo: Option<String>,
    pub number: Option<f64>,
    pub group: Option<String>,
    pub radio: bool,
    pub catchup_days: i64,
}

/// Attributi `chiave="valore"` di una riga `#EXTINF`.
fn attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // chiave: lettere, cifre, '-', '_'
        let start = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b'_') {
            i += 1;
        }
        if i > start && i + 1 < b.len() && b[i] == b'=' && b[i + 1] == b'"' {
            let key = s[start..i].to_ascii_lowercase();
            let vstart = i + 2;
            let Some(end) = s[vstart..].find('"') else { break };
            out.push((key, s[vstart..vstart + end].to_string()));
            i = vstart + end + 1;
        } else {
            i = if i > start { i } else { i + 1 };
        }
    }
    out
}

/// Separa gli attributi dal nome (dopo la prima virgola fuori dalle virgolette: il nome può contenerne altre).
fn split_extinf(line: &str) -> (&str, &str) {
    let mut in_quotes = false;
    let mut first = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                first = Some(i);
                break;
            }
            _ => {}
        }
    }
    match first {
        Some(i) => (&line[..i], line[i + 1..].trim()),
        None => (line, ""),
    }
}

pub fn parse(text: &str) -> Vec<M3uEntry> {
    let mut out = Vec::new();
    let mut cur: Option<M3uEntry> = None;
    let mut group_line: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (a, name) = split_extinf(rest);
            let mut e = M3uEntry { name: name.to_string(), ..Default::default() };
            for (k, v) in attrs(a) {
                let v = v.trim().to_string();
                if v.is_empty() {
                    continue;
                }
                match k.as_str() {
                    "tvg-id" => e.tvg_id = Some(v),
                    "tvg-name" => e.tvg_name = Some(v),
                    "tvg-logo" => e.logo = Some(v),
                    "tvg-chno" | "channel-number" | "tvg-num" => e.number = v.parse().ok(),
                    "group-title" => e.group = Some(v),
                    "radio" => e.radio = v == "true" || v == "1",
                    "catchup-days" | "timeshift" | "tvg-rec" => e.catchup_days = v.parse().unwrap_or(0),
                    _ => {}
                }
            }
            if e.name.is_empty() {
                e.name = e.tvg_name.clone().unwrap_or_default();
            }
            cur = Some(e);
        } else if let Some(g) = line.strip_prefix("#EXTGRP:") {
            group_line = Some(g.trim().to_string());
        } else if line.starts_with('#') {
            continue;
        } else if let Some(mut e) = cur.take() {
            e.url = line.to_string();
            if e.group.is_none() {
                e.group = group_line.take();
            }
            group_line = None;
            if !e.name.is_empty() {
                out.push(e);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatcharr_playlist() {
        let t = "#EXTM3U x-tvg-url=\"http://d/output/epg\"\n#EXTINF:-1 tvg-id=\"1\" tvg-name=\"Platea Uno\" tvg-logo=\"http://d/api/channels/logos/1/cache/\" tvg-chno=\"1\" catchup=\"default\" catchup-days=\"3\" group-title=\"Intrattenimento, Cinema\",Platea Uno, HD\nhttp://d/proxy/ts/stream/fd43b5e7-a803-4853-854e-64397d3e8ce5\n#EXTINF:-1,Senza attributi\n#EXTGRP:Radio\nhttp://x/2\n";
        let v = parse(t);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "Platea Uno, HD");
        assert_eq!(v[0].tvg_id.as_deref(), Some("1"));
        assert_eq!(v[0].group.as_deref(), Some("Intrattenimento, Cinema"));
        assert_eq!(v[0].number, Some(1.0));
        assert_eq!(v[0].catchup_days, 3);
        assert_eq!(v[0].url, "http://d/proxy/ts/stream/fd43b5e7-a803-4853-854e-64397d3e8ce5");
        assert_eq!(v[1].name, "Senza attributi");
        assert_eq!(v[1].group.as_deref(), Some("Radio"));
    }
}
