use axum::{
    Form, Router,
    extract::State,
    http::header,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use log::{error, info, warn};
use serde::Deserialize;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::process::Command;

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct AppState {
    download_dir: String,
    music_dir: String,
    audiobook_dir: String,
    film_dir: String,
    series_dir: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum MediaKind {
    Music,
    Audiobook,
    Film,
    Series,
}

impl MediaKind {
    fn output_dir<'a>(&self, state: &'a AppState) -> &'a str {
        match self {
            MediaKind::Music => &state.music_dir,
            MediaKind::Audiobook => &state.audiobook_dir,
            MediaKind::Film => &state.film_dir,
            MediaKind::Series => &state.series_dir,
        }
    }

    fn is_audio(&self) -> bool {
        matches!(self, MediaKind::Music | MediaKind::Audiobook)
    }

    fn label(&self) -> &'static str {
        match self {
            MediaKind::Music => "music",
            MediaKind::Audiobook => "audiobook",
            MediaKind::Film => "film",
            MediaKind::Series => "series",
        }
    }
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Yoik</title>
  <link rel="icon" href="/favicon.ico" type="image/x-icon" />
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: system-ui, sans-serif;
      background: #0f0f0f;
      color: #e8e8e8;
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 2rem;
    }
    main {
      width: 100%;
      max-width: 540px;
      display: flex;
      flex-direction: column;
      gap: 2rem;
    }
    h1 {
      text-align: center;
      font-size: 2rem;
      letter-spacing: 0.05em;
      color: #fff;
    }
    section {
      background: #1a1a1a;
      border: 1px solid #2e2e2e;
      border-radius: 12px;
      padding: 1.5rem;
      display: flex;
      flex-direction: column;
      gap: 1.25rem;
    }
    .radio-group {
      display: flex;
      gap: 0.5rem;
      flex-wrap: wrap;
    }
    .radio-group input[type="radio"] {
      position: absolute;
      opacity: 0;
      width: 0;
      height: 0;
    }
    .radio-group label {
      display: inline-flex;
      align-items: center;
      gap: 0.35em;
      padding: 0.45rem 0.9rem;
      border: 1px solid #444;
      border-radius: 999px;
      cursor: pointer;
      font-size: 0.9rem;
      color: #aaa;
      background: #111;
      transition: border-color 0.15s, color 0.15s, background 0.15s;
      user-select: none;
    }
    .radio-group input[type="radio"]:checked + label {
      border-color: #e00;
      color: #fff;
      background: #2a0000;
    }
    .radio-group label:hover {
      border-color: #888;
      color: #e8e8e8;
    }
    .url-row {
      display: flex;
      gap: 0.5rem;
    }
    input[type="text"] {
      flex: 1;
      padding: 0.6rem 0.8rem;
      background: #111;
      border: 1px solid #333;
      border-radius: 8px;
      color: #e8e8e8;
      font-size: 0.95rem;
      outline: none;
    }
    input[type="text"]:focus {
      border-color: #666;
    }
    button {
      padding: 0.6rem 1.2rem;
      background: #e00;
      border: none;
      border-radius: 8px;
      color: #fff;
      font-size: 0.95rem;
      font-weight: 600;
      cursor: pointer;
      white-space: nowrap;
    }
    button:hover { background: #c00; }
  </style>
</head>
<body>
  <main>
    <h1>Yoik</h1>

    <section>
      <form method="post" action="/yoik">
        <div class="url-row">
          <input type="text" name="url" placeholder="YouTube URL or magnet / ftp / http link" required />
          <button type="submit">Yoik!</button>
        </div>
        <br />
        <div class="radio-group">
          <input type="radio" name="kind" id="kind-film" value="film" />
          <label for="kind-film">🎬 Film</label>

          <input type="radio" name="kind" id="kind-series" value="series" />
          <label for="kind-series">📺 Series</label>

          <input type="radio" name="kind" id="kind-music" value="music" />
          <label for="kind-music">🎵 Music</label>

          <input type="radio" name="kind" id="kind-audiobook" value="audiobook" />
          <label for="kind-audiobook">📖 Audiobook</label>


        </div>
      </form>
    </section>
  </main>
</body>
</html>"#;

#[derive(Deserialize)]
struct RipForm {
    url: String,
    kind: MediaKind,
}

fn is_valid_youtube_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    let Ok(parsed) = url::Url::parse(trimmed) else {
        return false;
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return false;
    }
    matches!(
        parsed.host_str(),
        Some("youtube.com")
            | Some("www.youtube.com")
            | Some("m.youtube.com")
            | Some("music.youtube.com")
            | Some("youtu.be")
    )
}

fn is_aria2c_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.to_ascii_lowercase().starts_with("magnet:") {
        return true;
    }
    let Ok(parsed) = url::Url::parse(trimmed) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https" | "ftp")
}

fn new_staging_dir(download_dir: &str) -> String {
    let n = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{download_dir}/{nanos}-{n}")
}

async fn prepare_staging(download_dir: &str) -> std::io::Result<String> {
    let staging = new_staging_dir(download_dir);
    tokio::fs::create_dir_all(&staging).await?;
    Ok(staging)
}

async fn move_staging_to_dest(staging_dir: &str, dest_dir: &str) -> Result<(), String> {
    tokio::fs::create_dir_all(dest_dir)
        .await
        .map_err(|e| format!("failed to create destination {dest_dir}: {e}"))?;

    let mut entries = tokio::fs::read_dir(staging_dir)
        .await
        .map_err(|e| format!("failed to read staging dir {staging_dir}: {e}"))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("failed to read staging dir {staging_dir}: {e}"))?
    {
        let src = entry.path();
        let dest = Path::new(dest_dir).join(entry.file_name());
        let status = Command::new("mv")
            .arg(&src)
            .arg(&dest)
            .status()
            .await
            .map_err(|e| format!("failed to spawn mv: {e}"))?;
        if !status.success() {
            return Err(format!(
                "mv {} {} failed with exit code {}",
                src.display(),
                dest.display(),
                status.code().map_or(-1, |c| c),
            ));
        }
    }

    tokio::fs::remove_dir_all(staging_dir)
        .await
        .map_err(|e| format!("failed to remove staging dir {staging_dir}: {e}"))?;
    Ok(())
}

async fn finish_download(label: &'static str, url: &str, staging: &str, dest_dir: &str) {
    match move_staging_to_dest(staging, dest_dir).await {
        Ok(()) => {
            info!("{label} completed successfully: {url} -> {dest_dir}");
            post_to_matrix(format!("Downloaded {label} to {dest_dir}"));
        }
        Err(e) => {
            error!("{label} downloaded but failed to move from {staging} to {dest_dir}: {e}");
            post_to_matrix(format!(
                "Downloaded {label} but failed to move to {dest_dir}: {e}"
            ));
        }
    }
}

fn spawn_yt_dlp(
    label: &'static str,
    url: String,
    mut args: Vec<String>,
    download_dir: String,
    dest_dir: String,
) {
    info!("spawning yt-dlp for {label}: {url}");
    tokio::spawn(async move {
        let staging = match prepare_staging(&download_dir).await {
            Ok(s) => s,
            Err(e) => {
                error!("failed to create staging dir in {download_dir}: {e}");
                post_to_matrix(format!("Failed to download {label}: {e}"));
                return;
            }
        };

        args.push("-o".into());
        args.push(format!("{staging}/%(title)s.%(ext)s"));
        args.push(url.clone());

        match Command::new("yt-dlp")
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
        {
            Ok(status) if status.success() => {
                finish_download(label, &url, &staging, &dest_dir).await;
            }
            Ok(status) => {
                error!(
                    "yt-dlp {label} failed (exit code {}): {url} (staging {staging})",
                    status.code().map_or(-1, |c| c),
                );

                post_to_matrix(format!("Failed to download {label}: exit code {}", status.code().map_or(-1, |c| c)));
            }
            Err(e) => {
                error!("failed to spawn yt-dlp for {label}: {e}");

                post_to_matrix(format!("Failed to download {label}: {e}"));
            }
        }
    });
}

fn spawn_aria2c(label: &'static str, url: String, download_dir: String, dest_dir: String) {
    info!("spawning aria2c for {label}: {url}");
    tokio::spawn(async move {
        let staging = match prepare_staging(&download_dir).await {
            Ok(s) => s,
            Err(e) => {
                error!("failed to create staging dir in {download_dir}: {e}");
                post_to_matrix(format!("Failed to download {label}: {e}"));
                return;
            }
        };

        match Command::new("aria2c")
            .arg("--dir")
            .arg(&staging)
            .arg("--continue=true")
            .arg("--seed-time=60")
            .arg(&url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
        {
            Ok(status) if status.success() => {
                finish_download(label, &url, &staging, &dest_dir).await;
            }
            Ok(status) => {
                error!(
                    "aria2c {label} failed (exit code {}): {url} (staging {staging})",
                    status.code().map_or(-1, |c| c),
                );

                post_to_matrix(format!("Failed to download {label} to {dest_dir}: exit code {}", status.code().map_or(-1, |c| c)));
            }
            Err(e) => {
                error!("failed to spawn aria2c for {label}: {e}");

                post_to_matrix(format!("Failed to download {label} to {dest_dir}: {e}"));
            }
        }
    });
}

fn post_to_matrix(msg: String) {
    info!("posting to matrix: {msg}");
    tokio::spawn(async move {
        match Command::new("/home/felix/.cargo/bin/t2")
            .arg("pub")
            .arg("string/matrix")
            .arg(&msg)
            .arg("tcp:10.0.0.2:9999")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
        {
            Ok(status) if status.success() => {
                info!("matrix post completed successfully: {msg}");
            }
            Ok(status) => {
                error!(
                    "matrix post failed (exit code {}): {msg}",
                    status.code().map_or(-1, |c| c),
                );
            }
            Err(e) => {
                error!("failed to spawn matrix post: {e}");
            }
        }
    });
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn favicon() -> Response {
    static BYTES: &[u8] = include_bytes!("../favicon.ico");
    ([(header::CONTENT_TYPE, "image/x-icon")], BYTES).into_response()
}

async fn yoik(State(state): State<AppState>, Form(form): Form<RipForm>) -> Redirect {
    let label = form.kind.label();
    let dir = form.kind.output_dir(&state).to_owned();
    let download_dir = state.download_dir.clone();

    if is_valid_youtube_url(&form.url) {
        if form.kind.is_audio() {
            spawn_yt_dlp(
                label,
                form.url,
                vec![
                    "-x".into(),
                    "--audio-format".into(),
                    "mp3".into(),
                    "--no-playlist".into(),
                ],
                download_dir,
                dir,
            );
        } else {
            spawn_yt_dlp(
                label,
                form.url,
                vec![
                    "-f".into(),
                    "bestvideo+bestaudio/best".into(),
                    "--merge-output-format".into(),
                    "mp4".into(),
                    "--no-playlist".into(),
                ],
                download_dir,
                dir,
            );
        }
    } else if is_aria2c_url(&form.url) {
        spawn_aria2c(label, form.url, download_dir, dir);
    } else {
        warn!("rejected unsupported URL for {label} rip: {:?}", form.url);
    }

    Redirect::to("/")
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let port: u16 = std::env::var("YOIK_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);

    let download_dir = std::env::var("YOIK_DOWNLOAD_DIR")
        .unwrap_or_else(|_| "/home/felix/data/download".into());
    let music_dir = std::env::var("YOIK_MUSIC_DIR")
        .unwrap_or_else(|_| "/home/felix/data/audio/music".into());
    let audiobook_dir = std::env::var("YOIK_AUDIOBOOK_DIR")
        .unwrap_or_else(|_| "/home/felix/data/audio/audiobooks".into());
    let film_dir = std::env::var("YOIK_FILM_DIR")
        .unwrap_or_else(|_| "/home/felix/data/video/films".into());
    let series_dir = std::env::var("YOIK_SERIES_DIR")
        .unwrap_or_else(|_| "/home/felix/data/video/series".into());

    info!("download dir:  {download_dir}");
    info!("music dir:     {music_dir}");
    info!("audiobook dir: {audiobook_dir}");
    info!("film dir:      {film_dir}");
    info!("series dir:    {series_dir}");

    let state = AppState {
        download_dir,
        music_dir,
        audiobook_dir,
        film_dir,
        series_dir,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/favicon.ico", get(favicon))
        .route("/yoik", post(yoik))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    info!("listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}
