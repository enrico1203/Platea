//! Costruzione dei comandi FFmpeg: funzione pura, verificata con test sugli argomenti prodotti.
//! Gli argomenti sono sempre una lista, mai una stringa per una shell; nessun testo dell'utente entra nel comando.

use platea_playback::{AudioAction, VideoAction};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Software,
    Vaapi,
    Qsv,
    Nvenc,
    Amf,
}

impl Backend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::Software => "software",
            Backend::Vaapi => "vaapi",
            Backend::Qsv => "qsv",
            Backend::Nvenc => "nvenc",
            Backend::Amf => "amf",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Backend::Software => "Software",
            Backend::Vaapi => "VA-API",
            Backend::Qsv => "QSV",
            Backend::Nvenc => "NVENC",
            Backend::Amf => "AMF",
        }
    }

    pub fn encoder(&self, codec: &str) -> &'static str {
        match (self, codec) {
            (Backend::Software, "hevc") => "libx265",
            (Backend::Software, "av1") => "libsvtav1",
            (Backend::Software, _) => "libx264",
            (Backend::Vaapi, "hevc") => "hevc_vaapi",
            (Backend::Vaapi, "av1") => "av1_vaapi",
            (Backend::Vaapi, _) => "h264_vaapi",
            (Backend::Qsv, "hevc") => "hevc_qsv",
            (Backend::Qsv, "av1") => "av1_qsv",
            (Backend::Qsv, _) => "h264_qsv",
            (Backend::Nvenc, "hevc") => "hevc_nvenc",
            (Backend::Nvenc, "av1") => "av1_nvenc",
            (Backend::Nvenc, _) => "h264_nvenc",
            (Backend::Amf, "hevc") => "hevc_amf",
            (Backend::Amf, "av1") => "av1_amf",
            (Backend::Amf, _) => "h264_amf",
        }
    }
}

/// Capacità di un backend sulla macchina, dalla prova reale.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HwProfile {
    pub backend: Backend,
    pub device: String,
    /// Formati decodificati in GPU: `h264`, `hevc`, `hevc10`, `vp9`, `vp910`, `av1`, `av110`.
    pub decode: Vec<String>,
    /// Codec codificati in GPU: `h264`, `hevc`, `av1`.
    pub encode: Vec<String>,
}

impl HwProfile {
    pub fn software() -> Self {
        HwProfile { backend: Backend::Software, device: String::new(), decode: vec![], encode: vec!["h264".into(), "hevc".into()] }
    }

    pub fn can_decode(&self, codec: &str, bit_depth: u8) -> bool {
        let key = if bit_depth > 8 { format!("{codec}10") } else { codec.to_string() };
        self.decode.contains(&key)
    }
}

#[derive(Debug, Clone)]
pub struct SourceVideo {
    pub index: u32,
    pub codec: String,
    pub bit_depth: u8,
    pub width: u32,
    pub height: u32,
    pub hdr: bool,
}

#[derive(Debug, Clone)]
pub struct JobSpec {
    pub input: PathBuf,
    pub video: Option<SourceVideo>,
    pub audio_index: Option<u32>,
    pub video_action: VideoAction,
    pub audio_action: AudioAction,
    /// Posizione di partenza (ms), sempre un confine di segmento.
    pub start_ms: i64,
    pub start_segment: u32,
    /// Confini dei segmenti successivi, in ms assoluti.
    pub cut_points_ms: Vec<i64>,
    pub out_dir: PathBuf,
    pub threads: u32,
    pub sw_preset: String,
    pub sw_crf: u32,
}

fn secs(ms: i64) -> String {
    format!("{:.3}", ms as f64 / 1000.0)
}

/// Dimensione d'uscita: rispetta il limite mantenendo le proporzioni, valori pari.
pub fn output_size(src_w: u32, src_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let (w, h) = (src_w.max(2), src_h.max(2));
    let scale = (max_w as f64 / w as f64).min(max_h as f64 / h as f64).min(1.0);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    (even(w as f64 * scale), even(h as f64 * scale))
}

/// Argomenti di FFmpeg per una sessione HLS a segmenti MPEG-TS che parte da `start_segment`.
pub fn build_args(spec: &JobSpec, hw: &HwProfile) -> Vec<String> {
    let mut a: Vec<String> = Vec::with_capacity(64);
    let push = |a: &mut Vec<String>, items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    push(&mut a, &["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-nostats"]);

    let transcode = matches!(spec.video_action, VideoAction::Transcode { .. });
    let (out_codec, max_w, max_h, bitrate, tonemap, burn) = match &spec.video_action {
        VideoAction::Transcode { codec, max_width, max_height, bitrate, tonemap, burn_subtitle } => {
            (codec.as_str(), *max_width, *max_height, *bitrate, *tonemap, *burn_subtitle)
        }
        VideoAction::Copy => ("copy", 0, 0, 0, false, None),
    };
    let src = spec.video.as_ref();
    let backend = if transcode { hw.backend } else { Backend::Software };
    // La GPU decodifica solo se regge il formato e non serve imprimere sottotitoli (overlay in software).
    let hw_decode = transcode && backend != Backend::Software && burn.is_none() && src.is_some_and(|v| hw.can_decode(&v.codec, v.bit_depth));

    // Inizializzazione del dispositivo.
    if transcode {
        match backend {
            Backend::Vaapi => {
                push(&mut a, &["-init_hw_device", &format!("vaapi=va:{}", hw.device), "-filter_hw_device", "va"]);
                if hw_decode {
                    push(&mut a, &["-hwaccel", "vaapi", "-hwaccel_output_format", "vaapi", "-hwaccel_device", "va"]);
                }
            }
            Backend::Qsv => {
                push(&mut a, &["-init_hw_device", &format!("vaapi=va:{}", hw.device), "-init_hw_device", "qsv=qs@va", "-filter_hw_device", "qs"]);
                if hw_decode {
                    push(&mut a, &["-hwaccel", "qsv", "-hwaccel_output_format", "qsv"]);
                }
            }
            Backend::Nvenc => {
                push(&mut a, &["-init_hw_device", "cuda=cu:0", "-filter_hw_device", "cu"]);
                if hw_decode {
                    push(&mut a, &["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"]);
                }
            }
            Backend::Amf => {
                push(&mut a, &["-init_hw_device", "d3d11va=dx", "-filter_hw_device", "dx"]);
                if hw_decode {
                    push(&mut a, &["-hwaccel", "d3d11va", "-hwaccel_output_format", "d3d11"]);
                }
            }
            Backend::Software => {}
        }
    }

    // Tempi originali conservati: i segmenti di avvii successivi si allineano tra loro.
    push(&mut a, &["-copyts", "-avoid_negative_ts", "disabled", "-fflags", "+genpts"]);
    if spec.start_ms > 0 {
        // Per la copia si punta 200 ms dopo il keyframe: FFmpeg arretra la ricerca di ~130 ms sui flussi con
        // B-frame e poi prende il keyframe precedente o uguale, che così è proprio quello del taglio.
        let ss = spec.start_ms;
        a.push("-ss".into());
        a.push(if transcode { secs(ss) } else { secs(ss + COPY_SEEK_AFTER_MS) });
    }
    if spec.threads > 0 && !hw_decode {
        push(&mut a, &["-threads", &spec.threads.to_string()]);
    }
    a.push("-i".into());
    a.push(spec.input.to_string_lossy().to_string());

    // Tracce.
    if let Some(v) = src {
        if let Some(sub) = burn {
            a.push("-filter_complex".into());
            let (w, h) = output_size(v.width, v.height, max_w, max_h);
            let mut chain = format!("[0:{}][0:{}]overlay=eof_action=pass", v.index, sub);
            chain.push_str(&format!(",scale={w}:{h}"));
            if tonemap {
                chain.push_str(",zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv");
            }
            chain.push_str(",format=yuv420p");
            match backend {
                Backend::Vaapi | Backend::Qsv => chain.push_str(",format=nv12,hwupload=extra_hw_frames=64"),
                Backend::Nvenc => chain.push_str(",hwupload_cuda"),
                _ => {}
            }
            chain.push_str("[v]");
            a.push(chain);
            push(&mut a, &["-map", "[v]"]);
        } else {
            push(&mut a, &["-map", &format!("0:{}", v.index)]);
        }
    }
    if let Some(ai) = spec.audio_index {
        if !matches!(spec.audio_action, AudioAction::None) {
            push(&mut a, &["-map", &format!("0:{ai}")]);
        }
    }
    push(&mut a, &["-sn", "-dn", "-map_metadata", "-1", "-map_chapters", "-1"]);

    // Video.
    match (&spec.video_action, src) {
        (VideoAction::Copy, _) => push(&mut a, &["-c:v", "copy"]),
        (VideoAction::Transcode { .. }, Some(v)) => {
            let (w, h) = output_size(v.width, v.height, max_w, max_h);
            if burn.is_none() {
                let vf = video_filter(backend, hw_decode, w, h, tonemap && v.hdr, v.bit_depth);
                if !vf.is_empty() {
                    push(&mut a, &["-vf", &vf]);
                }
            }
            let enc = backend.encoder(out_codec);
            push(&mut a, &["-c:v", enc]);
            let br = bitrate.max(300_000);
            match backend {
                Backend::Software => {
                    if enc == "libx264" {
                        push(&mut a, &["-preset", &spec.sw_preset, "-crf", &spec.sw_crf.to_string(), "-profile:v", "high", "-level:v", "4.1"]);
                    } else if enc == "libx265" {
                        push(&mut a, &["-preset", &spec.sw_preset, "-crf", &(spec.sw_crf + 3).to_string(), "-tag:v", "hvc1"]);
                    } else {
                        push(&mut a, &["-preset", "10", "-crf", &(spec.sw_crf + 10).to_string()]);
                    }
                    push(&mut a, &["-maxrate", &br.to_string(), "-bufsize", &(br * 2).to_string()]);
                }
                Backend::Nvenc => {
                    push(&mut a, &["-preset", "p4", "-rc", "vbr", "-b:v", &br.to_string(), "-maxrate", &br.to_string(), "-bufsize", &(br * 2).to_string()])
                }
                Backend::Qsv => push(&mut a, &["-preset", "veryfast", "-b:v", &br.to_string(), "-maxrate", &br.to_string(), "-bufsize", &(br * 2).to_string()]),
                Backend::Vaapi | Backend::Amf => push(&mut a, &["-b:v", &br.to_string(), "-maxrate", &br.to_string(), "-bufsize", &(br * 2).to_string()]),
            }
            // Keyframe forzati sui confini dei segmenti: le playlist restano valide dopo ogni riavvio.
            if !spec.cut_points_ms.is_empty() {
                let list: Vec<String> = spec.cut_points_ms.iter().map(|t| secs(*t)).collect();
                push(&mut a, &["-force_key_frames", &list.join(",")]);
            }
            if backend == Backend::Software {
                push(&mut a, &["-sc_threshold", "0"]);
            }
        }
        (VideoAction::Transcode { .. }, None) => {}
    }

    // Audio.
    match &spec.audio_action {
        AudioAction::Copy => push(&mut a, &["-c:a", "copy"]),
        AudioAction::Transcode { codec, channels, bitrate } => {
            let c = if codec == "aac" { "aac" } else { codec.as_str() };
            push(&mut a, &["-c:a", c, "-ac", &channels.to_string(), "-b:a", &bitrate.to_string()]);
        }
        AudioAction::None => push(&mut a, &["-an"]),
    }

    // Uscita: muxer segment con confini espliciti, segmenti MPEG-TS.
    push(&mut a, &["-max_muxing_queue_size", "2048", "-muxdelay", "0", "-muxpreload", "0"]);
    push(&mut a, &["-f", "segment", "-segment_format", "mpegts"]);
    // Il muxer segment misura `segment_times` dal primo pacchetto ricevuto, non in tempo assoluto:
    // con -copyts dopo un salto i tempi assoluti non scatterebbero mai. Si passano relativi all'inizio,
    // con un margine (`segment_time_delta`) per i pochi ms tra il primo pacchetto audio e il keyframe.
    if !spec.cut_points_ms.is_empty() {
        let list: Vec<String> = spec.cut_points_ms.iter().map(|t| secs((t - spec.start_ms).max(0))).collect();
        push(&mut a, &["-segment_times", &list.join(",")]);
    } else {
        push(&mut a, &["-segment_time", "86400"]);
    }
    let delta_ms = if transcode { 100 } else { copy_cut_tolerance_ms(spec.start_ms, &spec.cut_points_ms) };
    push(&mut a, &["-segment_time_delta", &secs(delta_ms), "-segment_start_number", &spec.start_segment.to_string()]);
    let list = spec.out_dir.join("segments.csv");
    push(&mut a, &["-segment_list", &list.to_string_lossy(), "-segment_list_type", "csv", "-segment_list_flags", "+live"]);
    push(&mut a, &["-reset_timestamps", "0", "-break_non_keyframes", "0"]);
    a.push(spec.out_dir.join("seg_%05d.ts").to_string_lossy().to_string());
    a
}

/// Quanto dopo il keyframe di partenza punta la ricerca nella copia.
pub const COPY_SEEK_AFTER_MS: i64 = 200;

/// Margine di taglio nella copia: fino a 300 ms, mai oltre metà della distanza minima tra keyframe,
/// così il taglio non scivola sul keyframe precedente.
fn copy_cut_tolerance_ms(start_ms: i64, cuts: &[i64]) -> i64 {
    let mut prev = start_ms;
    let mut min_gap = i64::MAX;
    for &c in cuts {
        min_gap = min_gap.min(c - prev);
        prev = c;
    }
    (min_gap / 2).clamp(1, 300)
}

fn video_filter(backend: Backend, hw_decode: bool, w: u32, h: u32, tonemap: bool, bit_depth: u8) -> String {
    match backend {
        Backend::Software => {
            let mut f = format!("scale={w}:{h}");
            if tonemap {
                f = format!("zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,{f}");
            }
            f.push_str(",format=yuv420p");
            f
        }
        Backend::Vaapi => {
            let mut f = String::new();
            if !hw_decode {
                f.push_str(if bit_depth > 8 { "format=p010le,hwupload=extra_hw_frames=64," } else { "format=nv12,hwupload=extra_hw_frames=64," });
            }
            if tonemap {
                f.push_str("tonemap_vaapi=format=nv12:t=bt709:m=bt709:p=bt709,");
            }
            f.push_str(&format!("scale_vaapi=w={w}:h={h}:format=nv12"));
            f
        }
        Backend::Qsv => {
            let mut f = String::new();
            if !hw_decode {
                f.push_str(if bit_depth > 8 { "format=p010le,hwupload=extra_hw_frames=64," } else { "format=nv12,hwupload=extra_hw_frames=64," });
            }
            if tonemap {
                f.push_str(&format!("vpp_qsv=w={w}:h={h}:format=nv12:tonemap=1"));
            } else {
                f.push_str(&format!("scale_qsv=w={w}:h={h}:format=nv12"));
            }
            f
        }
        Backend::Nvenc => {
            let mut f = String::new();
            if !hw_decode {
                f.push_str("format=yuv420p,hwupload_cuda,");
            }
            if tonemap {
                f.push_str("tonemap_cuda=format=yuv420p:t=bt709:m=bt709:p=bt709,");
            }
            f.push_str(&format!("scale_cuda=w={w}:h={h}:format=yuv420p"));
            f
        }
        Backend::Amf => {
            let mut f = String::new();
            if !hw_decode {
                f.push_str("format=nv12,hwupload,");
            }
            f.push_str(&format!("scale_d3d11=width={w}:height={h}:format=nv12"));
            f
        }
    }
}

/// Confini dei segmenti. Transcodifica: intervalli regolari. Remux: sui keyframe, almeno `target` ms ciascuno.
pub fn plan_segments(duration_ms: i64, target_ms: i64, keyframes: Option<&[i64]>) -> Vec<i64> {
    let duration_ms = duration_ms.max(1);
    let target_ms = target_ms.max(1000);
    let mut cuts = vec![0i64];
    match keyframes {
        None => {
            let mut t = target_ms;
            while t < duration_ms - target_ms / 4 {
                cuts.push(t);
                t += target_ms;
            }
        }
        Some(kf) => {
            let mut last = 0i64;
            for &k in kf {
                if k <= 0 {
                    continue;
                }
                if k - last >= target_ms && k < duration_ms - 500 {
                    cuts.push(k);
                    last = k;
                }
            }
        }
    }
    cuts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(video: VideoAction, audio: AudioAction) -> JobSpec {
        JobSpec {
            input: "/media/film.mkv".into(),
            video: Some(SourceVideo { index: 0, codec: "hevc".into(), bit_depth: 10, width: 3840, height: 2160, hdr: true }),
            audio_index: Some(1),
            video_action: video,
            audio_action: audio,
            start_ms: 0,
            start_segment: 0,
            cut_points_ms: vec![6000, 12000],
            out_dir: "/transcode/s1".into(),
            threads: 0,
            sw_preset: "veryfast".into(),
            sw_crf: 23,
        }
    }

    fn joined(a: &[String]) -> String {
        a.join(" ")
    }

    #[test]
    fn remux_copies_video() {
        let mut s = spec(VideoAction::Copy, AudioAction::Copy);
        s.start_ms = 12_012;
        s.start_segment = 2;
        s.cut_points_ms = vec![18_012, 24_012];
        let a = build_args(&s, &HwProfile::software());
        let j = joined(&a);
        assert!(j.contains("-c:v copy"));
        assert!(j.contains("-c:a copy"));
        assert!(j.contains("-ss 12.212"));
        assert!(j.contains("-segment_start_number 2"));
        assert!(j.contains("-segment_times 6.000,12.000"), "{j}");
        assert!(j.contains("-segment_time_delta 0.300"));
        assert!(!j.contains("hwaccel"));
        assert!(a.last().unwrap().ends_with("seg_%05d.ts"));
    }

    #[test]
    fn software_transcode_with_tonemap() {
        let v = VideoAction::Transcode { codec: "h264".into(), max_width: 1920, max_height: 1080, bitrate: 8_000_000, tonemap: true, burn_subtitle: None };
        let a = build_args(&spec(v, AudioAction::Transcode { codec: "aac".into(), channels: 2, bitrate: 192_000 }), &HwProfile::software());
        let j = joined(&a);
        assert!(j.contains("-c:v libx264"));
        assert!(j.contains("tonemap=tonemap=hable"));
        assert!(j.contains("scale=1920:1080"));
        assert!(j.contains("-force_key_frames 6.000,12.000"));
        assert!(j.contains("-c:a aac -ac 2 -b:a 192000"));
    }

    #[test]
    fn vaapi_full_gpu_chain() {
        let hw = HwProfile { backend: Backend::Vaapi, device: "/dev/dri/renderD128".into(), decode: vec!["hevc10".into()], encode: vec!["h264".into()] };
        let v = VideoAction::Transcode { codec: "h264".into(), max_width: 1920, max_height: 1080, bitrate: 8_000_000, tonemap: true, burn_subtitle: None };
        let j = joined(&build_args(&spec(v, AudioAction::Copy), &hw));
        assert!(j.contains("-hwaccel vaapi -hwaccel_output_format vaapi"));
        assert!(j.contains("tonemap_vaapi"));
        assert!(j.contains("scale_vaapi=w=1920:h=1080"));
        assert!(j.contains("-c:v h264_vaapi"));
        assert!(!j.contains("hwupload"));
    }

    #[test]
    fn partial_fallback_software_decode_hardware_encode() {
        // H.264 a 10 bit: nessuna GPU lo decodifica, si decodifica in software e si codifica in GPU.
        let hw =
            HwProfile { backend: Backend::Qsv, device: "/dev/dri/renderD128".into(), decode: vec!["h264".into(), "hevc".into()], encode: vec!["h264".into()] };
        let mut s = spec(
            VideoAction::Transcode { codec: "h264".into(), max_width: 1920, max_height: 1080, bitrate: 6_000_000, tonemap: false, burn_subtitle: None },
            AudioAction::Copy,
        );
        s.video = Some(SourceVideo { index: 0, codec: "h264".into(), bit_depth: 10, width: 1920, height: 1080, hdr: false });
        let j = joined(&build_args(&s, &hw));
        assert!(!j.contains("-hwaccel qsv"));
        assert!(j.contains("hwupload"));
        assert!(j.contains("-c:v h264_qsv"));
    }

    #[test]
    fn burn_in_uses_overlay() {
        let v = VideoAction::Transcode { codec: "h264".into(), max_width: 1920, max_height: 1080, bitrate: 6_000_000, tonemap: false, burn_subtitle: Some(4) };
        let j = joined(&build_args(&spec(v, AudioAction::Copy), &HwProfile::software()));
        assert!(j.contains("[0:0][0:4]overlay"));
        assert!(j.contains("-map [v]"));
    }

    #[test]
    fn sizes_and_segments() {
        assert_eq!(output_size(3840, 2160, 1920, 1080), (1920, 1080));
        assert_eq!(output_size(1920, 800, 1280, 720), (1280, 534));
        assert_eq!(output_size(1280, 720, 1920, 1080), (1280, 720));
        assert_eq!(plan_segments(20_000, 6000, None), vec![0, 6000, 12000, 18000]);
        assert_eq!(plan_segments(19_000, 6000, None), vec![0, 6000, 12000]);
        assert_eq!(plan_segments(30_000, 6000, Some(&[0, 2000, 4000, 6000, 8000, 13000, 20000, 29800])), vec![0, 6000, 13000, 20000]);
    }
}
