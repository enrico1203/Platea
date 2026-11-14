//! Rilevamento della transcodifica hardware con prova reale.
//! Elencare gli encoder compilati in FFmpeg non basta: conta solo ciò che funziona davvero su questa macchina.
//! Per ogni backend si codifica un secondo di video sintetico e si decodificano campioni brevi.

use super::command::{Backend, HwProfile};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Default)]
pub struct TestResult {
    pub backend: String,
    pub kind: String,
    pub codec: String,
    pub ok: bool,
    pub ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Capabilities {
    pub tested_at: i64,
    pub ffmpeg_version: String,
    pub devices: Vec<String>,
    pub results: Vec<TestResult>,
    /// Backend scelto per la transcodifica, con le capacità verificate.
    pub selected: Option<HwProfile>,
    pub software_encoders: Vec<String>,
}

async fn run(ffmpeg: &Path, args: &[String], timeout: Duration) -> Result<u64, String> {
    let t0 = std::time::Instant::now();
    let mut cmd = Command::new(ffmpeg);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(out)) if out.status.success() => Ok(t0.elapsed().as_millis() as u64),
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr);
            let line = err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("errore").to_string();
            Err(line.chars().take(300).collect())
        }
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("tempo scaduto".into()),
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn init_args(backend: Backend, device: &str) -> Vec<String> {
    match backend {
        Backend::Vaapi => s(&["-init_hw_device", &format!("vaapi=va:{device}"), "-filter_hw_device", "va"]),
        Backend::Qsv => s(&["-init_hw_device", &format!("vaapi=va:{device}"), "-init_hw_device", "qsv=qs@va", "-filter_hw_device", "qs"]),
        Backend::Nvenc => s(&["-init_hw_device", "cuda=cu:0", "-filter_hw_device", "cu"]),
        Backend::Amf => s(&["-init_hw_device", "d3d11va=dx", "-filter_hw_device", "dx"]),
        Backend::Software => vec![],
    }
}

fn upload_filter(backend: Backend) -> &'static str {
    match backend {
        Backend::Vaapi | Backend::Qsv => "format=nv12,hwupload=extra_hw_frames=16",
        Backend::Nvenc => "format=yuv420p,hwupload_cuda",
        Backend::Amf => "format=nv12,hwupload",
        Backend::Software => "format=yuv420p",
    }
}

fn hwaccel_args(backend: Backend) -> Vec<String> {
    match backend {
        Backend::Vaapi => s(&["-hwaccel", "vaapi", "-hwaccel_output_format", "vaapi", "-hwaccel_device", "va"]),
        Backend::Qsv => s(&["-hwaccel", "qsv", "-hwaccel_output_format", "qsv"]),
        Backend::Nvenc => s(&["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"]),
        Backend::Amf => s(&["-hwaccel", "d3d11va"]),
        Backend::Software => vec![],
    }
}

/// Backend da provare su questa macchina, in ordine: Intel, NVIDIA, AMD.
fn candidates(device: &str) -> Vec<Backend> {
    let mut out = Vec::new();
    if Path::new(device).exists() {
        out.push(Backend::Qsv);
        out.push(Backend::Vaapi);
    }
    if Path::new("/dev/nvidia0").exists() || Path::new("/dev/nvidiactl").exists() {
        out.push(Backend::Nvenc);
    }
    if cfg!(windows) {
        out.push(Backend::Amf);
    }
    out
}

/// Crea i campioni per le prove di decodifica (in software, una volta).
async fn make_samples(ffmpeg: &Path, dir: &Path) -> Vec<(String, PathBuf)> {
    let _ = tokio::fs::create_dir_all(dir).await;
    let specs: [(&str, &str, Vec<&str>); 3] = [
        ("h264", "h264.mp4", vec!["-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "ultrafast"]),
        ("hevc", "hevc.mp4", vec!["-c:v", "libx265", "-pix_fmt", "yuv420p", "-preset", "ultrafast", "-x265-params", "log-level=error"]),
        ("hevc10", "hevc10.mp4", vec!["-c:v", "libx265", "-pix_fmt", "yuv420p10le", "-preset", "ultrafast", "-x265-params", "log-level=error"]),
    ];
    let mut out = Vec::new();
    for (name, file, enc) in specs {
        let path = dir.join(file);
        if !path.exists() {
            let mut args = s(&["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25", "-t", "1"]);
            args.extend(enc.iter().map(|x| x.to_string()));
            args.push(path.to_string_lossy().to_string());
            if run(ffmpeg, &args, Duration::from_secs(30)).await.is_err() {
                continue;
            }
        }
        out.push((name.to_string(), path));
    }
    out
}

/// Prova reale di ogni combinazione di backend, encoder e decoder.
pub async fn detect(ffmpeg: &Path, device: &str, forced: Option<Backend>, work_dir: &Path) -> Capabilities {
    let mut caps = Capabilities { tested_at: crate::util::now_ms(), ..Default::default() };
    for d in ["/dev/dri/renderD128", "/dev/dri/renderD129", "/dev/nvidia0"] {
        if Path::new(d).exists() {
            caps.devices.push(d.to_string());
        }
    }
    // Encoder software disponibili.
    for (codec, enc) in [("h264", "libx264"), ("hevc", "libx265")] {
        let args =
            s(&["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-t", "0.2", "-c:v", enc, "-f", "null", "-"]);
        let r = run(ffmpeg, &args, Duration::from_secs(20)).await;
        if r.is_ok() {
            caps.software_encoders.push(codec.to_string());
        }
        caps.results.push(TestResult {
            backend: "software".into(),
            kind: "encode".into(),
            codec: codec.into(),
            ok: r.is_ok(),
            ms: *r.as_ref().unwrap_or(&0),
            error: r.err(),
        });
    }

    let list = match forced {
        Some(Backend::Software) => vec![],
        Some(b) => vec![b],
        None => candidates(device),
    };
    let samples = if list.is_empty() { vec![] } else { make_samples(ffmpeg, &work_dir.join("hwtest")).await };

    for backend in list {
        let mut profile = HwProfile { backend, device: device.to_string(), decode: vec![], encode: vec![] };
        for codec in ["h264", "hevc", "av1"] {
            let mut args = s(&["-hide_banner", "-loglevel", "error"]);
            args.extend(init_args(backend, device));
            args.extend(s(&["-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30", "-t", "1", "-vf", upload_filter(backend)]));
            args.extend(s(&["-c:v", backend.encoder(codec), "-f", "null", "-"]));
            let r = run(ffmpeg, &args, Duration::from_secs(20)).await;
            if r.is_ok() {
                profile.encode.push(codec.to_string());
            }
            caps.results.push(TestResult {
                backend: backend.as_str().into(),
                kind: "encode".into(),
                codec: codec.into(),
                ok: r.is_ok(),
                ms: *r.as_ref().unwrap_or(&0),
                error: r.err(),
            });
        }
        if profile.encode.is_empty() {
            continue;
        }
        for (name, path) in &samples {
            let mut args = s(&["-hide_banner", "-loglevel", "error"]);
            args.extend(init_args(backend, device));
            args.extend(hwaccel_args(backend));
            args.extend(s(&["-i", &path.to_string_lossy(), "-f", "null", "-"]));
            let r = run(ffmpeg, &args, Duration::from_secs(20)).await;
            if r.is_ok() {
                profile.decode.push(name.clone());
            }
            caps.results.push(TestResult {
                backend: backend.as_str().into(),
                kind: "decode".into(),
                codec: name.clone(),
                ok: r.is_ok(),
                ms: *r.as_ref().unwrap_or(&0),
                error: r.err(),
            });
        }
        if caps.selected.is_none() && profile.encode.contains(&"h264".to_string()) {
            caps.selected = Some(profile);
        }
    }
    match &caps.selected {
        Some(p) => tracing::info!(backend = p.backend.as_str(), encode = ?p.encode, decode = ?p.decode, "transcodifica hardware disponibile"),
        None => tracing::info!("nessuna GPU utilizzabile: transcodifica in software"),
    }
    caps
}
