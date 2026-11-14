//! Indice dei keyframe dai Cues di un file Matroska, senza leggere il file intero.
//! Bastano poche letture: intestazione, SeekHead, Info, Tracks e il blocco Cues (di solito alla fine, qualche centinaio di KB).
//! Su un mount cloud è la differenza tra scaricare 10 GB e scaricarne meno di uno.

use std::io::{Read, Seek, SeekFrom};

const ID_EBML: u32 = 0x1A45_DFA3;
const ID_SEGMENT: u32 = 0x1853_8067;
const ID_SEEKHEAD: u32 = 0x114D_9B74;
const ID_SEEK: u32 = 0x4DBB;
const ID_SEEKID: u32 = 0x53AB;
const ID_SEEKPOS: u32 = 0x53AC;
const ID_INFO: u32 = 0x1549_A966;
const ID_TIMESCALE: u32 = 0x2AD7B1;
const ID_TRACKS: u32 = 0x1654_AE6B;
const ID_TRACKENTRY: u32 = 0xAE;
const ID_TRACKNUMBER: u32 = 0xD7;
const ID_TRACKTYPE: u32 = 0x83;
const ID_CUES: u32 = 0x1C53_BB6B;
const ID_CUEPOINT: u32 = 0xBB;
const ID_CUETIME: u32 = 0xB3;
const ID_CUETRACKPOS: u32 = 0xB7;
const ID_CUETRACK: u32 = 0xF7;
const ID_CLUSTER: u32 = 0x1F43_B675;

/// Oltre questa dimensione un blocco di metadati non è plausibile.
const MAX_META: u64 = 64 * 1024 * 1024;
/// Elementi di primo livello da attraversare se il SeekHead non indica i Cues.
const MAX_TOP_LEVEL: usize = 64;

fn read_id(b: &[u8]) -> Option<(u32, usize)> {
    let first = *b.first()?;
    let len = first.leading_zeros() as usize + 1;
    if len > 4 || b.len() < len {
        return None;
    }
    Some((b[..len].iter().fold(0u32, |a, x| (a << 8) | *x as u32), len))
}

/// Dimensione di un elemento; `None` nel valore = dimensione sconosciuta.
fn read_size(b: &[u8]) -> Option<(Option<u64>, usize)> {
    let first = *b.first()?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 || b.len() < len {
        return None;
    }
    let mut v = (first as u64) & (0xFF >> len);
    for x in &b[1..len] {
        v = (v << 8) | *x as u64;
    }
    let unknown = v == (1u64 << (7 * len)) - 1;
    Some((if unknown { None } else { Some(v) }, len))
}

fn uint(b: &[u8]) -> u64 {
    b.iter().take(8).fold(0u64, |a, x| (a << 8) | *x as u64)
}

/// Figli di un elemento già in memoria: (id, contenuto).
fn children(mut b: &[u8]) -> Vec<(u32, &[u8])> {
    let mut out = Vec::new();
    while !b.is_empty() {
        let Some((id, il)) = read_id(b) else { break };
        let Some((size, sl)) = read_size(&b[il..]) else { break };
        let start = il + sl;
        let end = match size {
            Some(s) => start.saturating_add(s as usize).min(b.len()),
            None => b.len(),
        };
        out.push((id, &b[start..end]));
        b = &b[end..];
    }
    out
}

struct Reader<R> {
    r: R,
    len: u64,
}

impl<R: Read + Seek> Reader<R> {
    fn read_at(&mut self, off: u64, n: u64) -> std::io::Result<Vec<u8>> {
        let n = n.min(self.len.saturating_sub(off));
        self.r.seek(SeekFrom::Start(off))?;
        let mut buf = vec![0u8; n as usize];
        self.r.read_exact(&mut buf)?;
        Ok(buf)
    }
    /// Intestazione di un elemento in `off`: (id, dimensione, inizio del contenuto).
    fn header(&mut self, off: u64) -> Option<(u32, Option<u64>, u64)> {
        let b = self.read_at(off, 12).ok()?;
        let (id, il) = read_id(&b)?;
        let (size, sl) = read_size(&b[il..])?;
        Some((id, size, off + (il + sl) as u64))
    }
    fn element(&mut self, off: u64, want: u32) -> Option<Vec<u8>> {
        let (id, size, data) = self.header(off)?;
        let size = size?;
        if id != want || size > MAX_META {
            return None;
        }
        self.read_at(data, size).ok()
    }
}

/// Tempi dei keyframe in millisecondi dai Cues; `None` se il file non ne ha o non è Matroska.
pub fn cue_keyframes<R: Read + Seek>(r: R, len: u64) -> Option<Vec<i64>> {
    let mut rd = Reader { r, len };
    let (id, size, data) = rd.header(0)?;
    if id != ID_EBML {
        return None;
    }
    let seg_off = data + size?;
    let (id, seg_size, seg_data) = rd.header(seg_off)?;
    if id != ID_SEGMENT {
        return None;
    }
    let seg_end = seg_size.map(|s| seg_data + s).unwrap_or(len).min(len);

    // Posizioni dal SeekHead (relative all'inizio dei dati del segmento).
    let mut pos_cues = None;
    let mut pos_info = None;
    let mut pos_tracks = None;
    let mut off = seg_data;
    for _ in 0..MAX_TOP_LEVEL {
        if off >= seg_end {
            break;
        }
        let Some((id, size, data)) = rd.header(off) else { break };
        match id {
            ID_SEEKHEAD => {
                let body = rd.read_at(data, size?.min(MAX_META)).ok()?;
                for (sid, seek) in children(&body) {
                    if sid != ID_SEEK {
                        continue;
                    }
                    let mut target = None;
                    let mut pos = None;
                    for (cid, v) in children(seek) {
                        match cid {
                            ID_SEEKID => target = read_id(v).map(|x| x.0),
                            ID_SEEKPOS => pos = Some(seg_data + uint(v)),
                            _ => {}
                        }
                    }
                    match (target, pos) {
                        (Some(ID_CUES), Some(p)) => pos_cues = pos_cues.or(Some(p)),
                        (Some(ID_INFO), Some(p)) => pos_info = pos_info.or(Some(p)),
                        (Some(ID_TRACKS), Some(p)) => pos_tracks = pos_tracks.or(Some(p)),
                        _ => {}
                    }
                }
            }
            ID_INFO => pos_info = pos_info.or(Some(off)),
            ID_TRACKS => pos_tracks = pos_tracks.or(Some(off)),
            ID_CUES => pos_cues = pos_cues.or(Some(off)),
            _ => {}
        }
        // I Cluster sono il grosso del file: dopo il primo, se non si conosce ancora la posizione dei Cues, si rinuncia.
        if id == ID_CLUSTER && pos_cues.is_none() {
            return None;
        }
        if pos_cues.is_some() && pos_info.is_some() && pos_tracks.is_some() {
            break;
        }
        let Some(size) = size else { break };
        off = data + size;
    }
    let pos_cues = pos_cues?;

    let mut scale: u64 = 1_000_000;
    if let Some(info) = pos_info.and_then(|p| rd.element(p, ID_INFO)) {
        if let Some((_, v)) = children(&info).into_iter().find(|(id, _)| *id == ID_TIMESCALE) {
            scale = uint(v).max(1);
        }
    }
    let mut video_track: Option<u64> = None;
    if let Some(tracks) = pos_tracks.and_then(|p| rd.element(p, ID_TRACKS)) {
        for (id, entry) in children(&tracks) {
            if id != ID_TRACKENTRY {
                continue;
            }
            let mut num = None;
            let mut kind = None;
            for (cid, v) in children(entry) {
                match cid {
                    ID_TRACKNUMBER => num = Some(uint(v)),
                    ID_TRACKTYPE => kind = Some(uint(v)),
                    _ => {}
                }
            }
            if kind == Some(1) {
                video_track = num;
                break;
            }
        }
    }

    let cues = rd.element(pos_cues, ID_CUES)?;
    let mut times: Vec<i64> = Vec::new();
    for (id, point) in children(&cues) {
        if id != ID_CUEPOINT {
            continue;
        }
        let mut t = None;
        let mut on_video = video_track.is_none();
        for (cid, v) in children(point) {
            match cid {
                ID_CUETIME => t = Some(uint(v)),
                ID_CUETRACKPOS if !on_video => {
                    on_video = children(v).iter().any(|(tid, tv)| *tid == ID_CUETRACK && Some(uint(tv)) == video_track);
                }
                _ => {}
            }
        }
        if let (Some(t), true) = (t, on_video) {
            times.push(((t as u128 * scale as u128) / 1_000_000) as i64);
        }
    }
    times.sort_unstable();
    times.dedup();
    (times.len() >= 2).then_some(times)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn id_bytes(id: u32) -> Vec<u8> {
        let b = id.to_be_bytes();
        let skip = b.iter().position(|x| *x != 0).unwrap();
        b[skip..].to_vec()
    }
    fn el(id: u32, body: &[u8]) -> Vec<u8> {
        let mut v = id_bytes(id);
        // Dimensione su 8 byte, come fanno molti muxer.
        v.push(0x01);
        v.extend_from_slice(&(body.len() as u64).to_be_bytes()[1..]);
        v.extend_from_slice(body);
        v
    }
    fn u(id: u32, val: u64) -> Vec<u8> {
        el(id, &val.to_be_bytes())
    }

    fn build(cues_first: bool, with_seekhead: bool) -> Vec<u8> {
        let info = el(ID_INFO, &u(ID_TIMESCALE, 1_000_000));
        let mut tr = Vec::new();
        tr.extend(el(ID_TRACKENTRY, &[u(ID_TRACKNUMBER, 1), u(ID_TRACKTYPE, 2)].concat()));
        tr.extend(el(ID_TRACKENTRY, &[u(ID_TRACKNUMBER, 2), u(ID_TRACKTYPE, 1)].concat()));
        let tracks = el(ID_TRACKS, &tr);
        let mut cp = Vec::new();
        for (t, track) in [(0u64, 2u64), (2002, 2), (4004, 2), (3000, 1), (6006, 2)] {
            cp.extend(el(ID_CUEPOINT, &[u(ID_CUETIME, t), el(ID_CUETRACKPOS, &u(ID_CUETRACK, track))].concat()));
        }
        let cues = el(ID_CUES, &cp);
        let cluster = el(ID_CLUSTER, &vec![0u8; 5000]);
        // SeekHead a dimensione fissa: 3 voci.
        let seek = |id: u32, pos: u64| el(ID_SEEK, &[el(ID_SEEKID, &id_bytes(id)), u(ID_SEEKPOS, pos)].concat());
        let sh_len = el(ID_SEEKHEAD, &[seek(ID_INFO, 0), seek(ID_TRACKS, 0), seek(ID_CUES, 0)].concat()).len() as u64;
        let mut body_after = Vec::new();
        let base = if with_seekhead { sh_len } else { 0 };
        let info_pos = base;
        let tracks_pos = info_pos + info.len() as u64;
        let (cues_pos, layout) = if cues_first {
            (tracks_pos + tracks.len() as u64, [info.clone(), tracks.clone(), cues.clone(), cluster.clone()].concat())
        } else {
            (tracks_pos + tracks.len() as u64 + cluster.len() as u64, [info.clone(), tracks.clone(), cluster.clone(), cues.clone()].concat())
        };
        if with_seekhead {
            body_after.extend(el(ID_SEEKHEAD, &[seek(ID_INFO, info_pos), seek(ID_TRACKS, tracks_pos), seek(ID_CUES, cues_pos)].concat()));
        }
        body_after.extend(layout);
        let mut file = el(ID_EBML, &[0x42, 0x82, 0x88, b'm', b'a', b't', b'r', b'o', b's', b'k', b'a']);
        file.extend(el(ID_SEGMENT, &body_after));
        file
    }

    #[test]
    fn cues_at_end_via_seekhead() {
        let f = build(false, true);
        let n = f.len() as u64;
        assert_eq!(cue_keyframes(Cursor::new(f), n), Some(vec![0, 2002, 4004, 6006]));
    }

    #[test]
    fn cues_before_clusters_without_seekhead() {
        let f = build(true, false);
        let n = f.len() as u64;
        assert_eq!(cue_keyframes(Cursor::new(f), n), Some(vec![0, 2002, 4004, 6006]));
    }

    #[test]
    fn gives_up_without_index() {
        let f = build(false, false);
        let n = f.len() as u64;
        assert_eq!(cue_keyframes(Cursor::new(f), n), None);
        assert_eq!(cue_keyframes(Cursor::new(vec![0u8; 64]), 64), None);
    }

    #[test]
    fn vint() {
        assert_eq!(read_size(&[0x81]), Some((Some(1), 1)));
        assert_eq!(read_size(&[0x40, 0x02]), Some((Some(2), 2)));
        assert_eq!(read_size(&[0xFF]), Some((None, 1)));
        assert_eq!(read_id(&[0x1A, 0x45, 0xDF, 0xA3]), Some((ID_EBML, 4)));
    }
}
