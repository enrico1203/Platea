//! Indice dei keyframe di un MP4/MOV dal box `moov`, senza leggere il file intero.
//! Si salta `mdat` con un seek, si legge solo `moov` (di solito pochi MB, all'inizio o alla fine) e dalla prima traccia
//! video si ricavano i campioni di sincronizzazione (`stss`) con i loro tempi di presentazione (`stts`, `ctts`, `elst`).
//! È lo stesso risultato di `ffprobe -show_entries packet=pts_time,flags` limitato ai keyframe, con poche letture.

use std::io::{Read, Seek, SeekFrom};

/// Oltre questa dimensione un `moov` non è plausibile (o costerebbe troppo leggerlo da un mount remoto).
const MAX_MOOV: u64 = 64 * 1024 * 1024;
/// Box di primo livello da attraversare prima di rinunciare.
const MAX_TOP_LEVEL: usize = 64;
/// Campioni oltre i quali il file non è plausibile (24 h a 240 fps).
const MAX_SAMPLES: u64 = 24 * 3600 * 240;

/// Figli di un box già in memoria: (tipo, contenuto).
fn boxes(mut b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    while b.len() >= 8 {
        let size32 = u32::from_be_bytes(b[0..4].try_into().unwrap()) as u64;
        let kind: [u8; 4] = b[4..8].try_into().unwrap();
        let (header, size) = match size32 {
            0 => (8, b.len() as u64),
            1 if b.len() >= 16 => (16, u64::from_be_bytes(b[8..16].try_into().unwrap())),
            1 => break,
            s => (8, s),
        };
        if size < header as u64 || size > b.len() as u64 {
            break;
        }
        out.push((kind, &b[header..size as usize]));
        b = &b[size as usize..];
    }
    out
}

fn child<'a>(b: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(b).into_iter().find(|(k, _)| k == kind).map(|(_, v)| v)
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Timescale di `mvhd` o `mdhd` (stessa disposizione: versione, date, timescale).
fn timescale(full_box: &[u8]) -> Option<u32> {
    let v = *full_box.first()?;
    let ts = if v == 1 { be32(full_box, 20)? } else { be32(full_box, 12)? };
    (ts > 0).then_some(ts)
}

/// Tabella (conteggio, valore) di `stts` o `ctts`.
fn pairs(full_box: &[u8]) -> Option<Vec<(u32, u32)>> {
    let n = be32(full_box, 4)? as usize;
    if full_box.len() < 8 + n * 8 {
        return None;
    }
    Some((0..n).map(|i| (be32(full_box, 8 + i * 8).unwrap(), be32(full_box, 12 + i * 8).unwrap())).collect())
}

/// Spostamento dell'edit list in unità della traccia: (ritardo iniziale, inizio nel media).
fn edit_shift(edts: Option<&[u8]>, movie_ts: u32, media_ts: u32) -> (i64, i64) {
    let Some(elst) = edts.and_then(|e| child(e, b"elst")) else { return (0, 0) };
    let Some(&version) = elst.first() else { return (0, 0) };
    let Some(n) = be32(elst, 4) else { return (0, 0) };
    let entry = if version == 1 { 20 } else { 12 };
    let mut delay: i64 = 0;
    for i in 0..n as usize {
        let at = 8 + i * entry;
        let (dur, media_time) = if version == 1 {
            match (be64(elst, at), be64(elst, at + 8)) {
                (Some(d), Some(m)) => (d, m as i64),
                _ => break,
            }
        } else {
            match (be32(elst, at), be32(elst, at + 4)) {
                (Some(d), Some(m)) => (d as u64, m as i32 as i64),
                _ => break,
            }
        };
        if media_time == -1 {
            // Modifica vuota: la traccia parte più tardi.
            delay += (dur as i128 * media_ts as i128 / movie_ts.max(1) as i128) as i64;
            continue;
        }
        return (delay, media_time);
    }
    (delay, 0)
}

/// Keyframe (ms, in ordine) della prima traccia video di un `moov` già letto.
fn keyframes_from_moov(moov: &[u8]) -> Option<Vec<i64>> {
    // I file frammentati tengono i campioni nei `moof`: qui non ci sono.
    if child(moov, b"mvex").is_some() {
        return None;
    }
    let movie_ts = child(moov, b"mvhd").and_then(timescale).unwrap_or(1000);
    for (kind, trak) in boxes(moov) {
        if &kind != b"trak" {
            continue;
        }
        let Some(mdia) = child(trak, b"mdia") else { continue };
        let Some(hdlr) = child(mdia, b"hdlr") else { continue };
        if hdlr.get(8..12) != Some(b"vide") {
            continue;
        }
        let media_ts = child(mdia, b"mdhd").and_then(timescale)?;
        let stbl = child(child(mdia, b"minf")?, b"stbl")?;
        let stts = pairs(child(stbl, b"stts")?)?;
        let ctts = child(stbl, b"ctts").and_then(pairs).unwrap_or_default();
        let total: u64 = stts.iter().map(|(c, _)| *c as u64).sum();
        if total == 0 || total > MAX_SAMPLES {
            return None;
        }
        // Senza `stss` ogni campione è un keyframe (intra-only): un indice così non serve a nulla, meglio ffprobe.
        let stss = child(stbl, b"stss")?;
        let n = be32(stss, 4)? as usize;
        if stss.len() < 8 + n * 4 || n == 0 {
            return None;
        }
        let sync: Vec<u32> = (0..n).map(|i| be32(stss, 8 + i * 4).unwrap()).collect();
        let (delay, start) = edit_shift(child(trak, b"edts"), movie_ts, media_ts);

        let mut out = Vec::with_capacity(n);
        let mut next_sync = sync.iter().copied().peekable();
        let (mut dts, mut sample): (i64, u32) = (0, 1);
        let mut ctts_it = ctts.iter().flat_map(|(c, off)| std::iter::repeat_n(*off as i32 as i64, *c as usize));
        'outer: for (count, delta) in stts {
            for _ in 0..count {
                let cts = ctts_it.next().unwrap_or(0);
                while next_sync.peek().is_some_and(|s| *s < sample) {
                    next_sync.next();
                }
                if next_sync.peek() == Some(&sample) {
                    let pts = dts + cts - start + delay;
                    out.push(((pts as i128 * 1000 + media_ts as i128 / 2).div_euclid(media_ts as i128)) as i64);
                    next_sync.next();
                }
                if next_sync.peek().is_none() {
                    break 'outer;
                }
                dts += delta as i64;
                sample += 1;
            }
        }
        out.retain(|t| *t >= 0);
        out.sort_unstable();
        out.dedup();
        return (!out.is_empty()).then_some(out);
    }
    None
}

/// Legge il `moov` saltando gli altri box di primo livello e ne ricava i keyframe.
pub fn sync_keyframes<R: Read + Seek>(mut r: R, file_size: u64) -> Option<Vec<i64>> {
    let mut pos = 0u64;
    for _ in 0..MAX_TOP_LEVEL {
        if pos + 8 > file_size {
            return None;
        }
        r.seek(SeekFrom::Start(pos)).ok()?;
        let mut h = [0u8; 16];
        r.read_exact(&mut h[..8]).ok()?;
        let size32 = u32::from_be_bytes(h[0..4].try_into().unwrap()) as u64;
        let kind: [u8; 4] = h[4..8].try_into().unwrap();
        let (header, size) = match size32 {
            0 => (8, file_size - pos),
            1 => {
                r.read_exact(&mut h[8..16]).ok()?;
                (16, u64::from_be_bytes(h[8..16].try_into().unwrap()))
            }
            s => (8, s),
        };
        if size < header || pos + size > file_size {
            return None;
        }
        match &kind {
            b"moov" => {
                let body = size - header;
                if body > MAX_MOOV {
                    return None;
                }
                let mut buf = vec![0u8; body as usize];
                r.read_exact(&mut buf).ok()?;
                return keyframes_from_moov(&buf);
            }
            // Un frammento prima del `moov` vuol dire file frammentato: nessun indice completo.
            b"moof" => return None,
            _ => {}
        }
        pos += size;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }
    fn full(kind: &[u8; 4], version: u8, rest: &[u8]) -> Vec<u8> {
        let mut b = vec![version, 0, 0, 0];
        b.extend_from_slice(rest);
        bx(kind, &b)
    }
    fn table(entries: &[(u32, u32)]) -> Vec<u8> {
        let mut v = (entries.len() as u32).to_be_bytes().to_vec();
        for (a, b) in entries {
            v.extend_from_slice(&a.to_be_bytes());
            v.extend_from_slice(&b.to_be_bytes());
        }
        v
    }
    fn hd(ts: u32) -> Vec<u8> {
        let mut v = vec![0u8; 8];
        v.extend_from_slice(&ts.to_be_bytes());
        v.extend_from_slice(&[0u8; 8]);
        v
    }

    /// 25 fps (timescale 12800, 512 per fotogramma), B-frame con ctts di 1024, keyframe ogni 50 fotogrammi,
    /// edit list che toglie lo spostamento iniziale (come scrive FFmpeg).
    fn sample_file(moov_at_end: bool) -> Vec<u8> {
        let mut hdlr = vec![0u8; 4];
        hdlr.extend_from_slice(b"vide");
        hdlr.extend_from_slice(&[0u8; 13]);
        let mut stss = 4u32.to_be_bytes().to_vec();
        for s in [1u32, 51, 101, 151] {
            stss.extend_from_slice(&s.to_be_bytes());
        }
        let stbl =
            [full(b"stts", 0, &table(&[(200, 512)])), full(b"ctts", 0, &table(&[(200, 1024)])), full(b"stss", 0, &stss), full(b"stsz", 0, &[0u8; 8])].concat();
        let minf = bx(b"minf", &bx(b"stbl", &stbl));
        let mdia = bx(b"mdia", &[full(b"mdhd", 0, &hd(12800)), full(b"hdlr", 0, &hdlr), minf].concat());
        let mut elst = 1u32.to_be_bytes().to_vec();
        elst.extend_from_slice(&8000u32.to_be_bytes());
        elst.extend_from_slice(&1024u32.to_be_bytes());
        elst.extend_from_slice(&[0, 1, 0, 0]);
        let edts = bx(b"edts", &full(b"elst", 0, &elst));
        // Prima una traccia audio, che va ignorata.
        let mut hdlr_a = vec![0u8; 4];
        hdlr_a.extend_from_slice(b"soun");
        hdlr_a.extend_from_slice(&[0u8; 13]);
        let audio = bx(b"trak", &bx(b"mdia", &[full(b"mdhd", 0, &hd(48000)), full(b"hdlr", 0, &hdlr_a)].concat()));
        let video = bx(b"trak", &[edts, mdia].concat());
        let moov = bx(b"moov", &[full(b"mvhd", 0, &hd(1000)), audio, video].concat());
        let ftyp = bx(b"ftyp", b"isom\0\0\0\0");
        let mdat = bx(b"mdat", &vec![0u8; 4096]);
        if moov_at_end {
            [ftyp, mdat, moov].concat()
        } else {
            [ftyp, moov, mdat].concat()
        }
    }

    #[test]
    fn keyframes_from_stss() {
        for at_end in [false, true] {
            let f = sample_file(at_end);
            let kf = sync_keyframes(Cursor::new(&f), f.len() as u64).unwrap();
            assert_eq!(kf, vec![0, 2000, 4000, 6000], "moov alla fine: {at_end}");
        }
    }

    #[test]
    fn rejects_garbage_and_fragments() {
        assert_eq!(sync_keyframes(Cursor::new(vec![0u8; 64]), 64), None);
        let f = [bx(b"ftyp", b"isom"), bx(b"moof", &[0u8; 16])].concat();
        assert_eq!(sync_keyframes(Cursor::new(&f), f.len() as u64), None);
    }

    /// Confronto con ffprobe su file veri: `PLATEA_MP4_DIR` con coppie `<file>` e `<file>.ref` (ms, uno per riga).
    #[test]
    #[ignore]
    fn matches_ffprobe() {
        let dir = std::env::var("PLATEA_MP4_DIR").expect("PLATEA_MP4_DIR");
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "ref") {
                continue;
            }
            let want: Vec<i64> = std::fs::read_to_string(format!("{}.ref", p.display())).unwrap().lines().map(|l| l.parse().unwrap()).collect();
            let size = std::fs::metadata(&p).unwrap().len();
            let got = sync_keyframes(std::io::BufReader::new(std::fs::File::open(&p).unwrap()), size).unwrap();
            let off = got.iter().zip(&want).map(|(a, b)| (a - b).abs()).max().unwrap_or(0);
            println!("{}: {} keyframe, ffprobe {}, scarto massimo {off} ms", p.display(), got.len(), want.len());
            assert_eq!(got.len(), want.len(), "{}", p.display());
            assert!(off <= 1, "{}", p.display());
        }
    }
}
