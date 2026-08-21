use axum::{
    Form, Json, Router,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
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
    http: reqwest::Client,
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
      max-width: 640px;
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
    .url-row button {
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
    .url-row button:hover { background: #c00; }
    .url-row button:disabled {
      opacity: 0.6;
      cursor: wait;
    }
    #results {
      display: flex;
      flex-direction: column;
      gap: 0.4rem;
      max-height: 60vh;
      overflow-y: auto;
    }
    #results:empty { display: none; }
    .result-status {
      color: #888;
      font-size: 0.9rem;
      text-align: center;
      padding: 0.5rem;
    }
    .result {
      display: flex;
      flex-direction: column;
      align-items: flex-start;
      gap: 0.2rem;
      width: 100%;
      padding: 0.7rem 0.85rem;
      background: #111;
      border: 1px solid #333;
      border-radius: 8px;
      color: #e8e8e8;
      cursor: pointer;
      text-align: left;
      font-weight: 400;
      white-space: normal;
    }
    .result:hover {
      border-color: #e00;
      background: #2a0000;
    }
    .result-name {
      color: #fff;
      font-size: 0.9rem;
      word-break: break-word;
    }
    .result-meta {
      color: #888;
      font-size: 0.8rem;
    }
  </style>
</head>
<body>
  <main>
    <h1>Yoik</h1>

    <section>
      <form method="post" action="/yoik">
        <div class="url-row">
          <input type="text" name="url" placeholder="YouTube URL, magnet / ftp / http link, or search term" required />
          <button type="submit">Yoik!</button>
        </div>
        <br />
        <div class="radio-group">
          <input type="radio" name="kind" id="kind-film" value="film" required />
          <label for="kind-film">🎬 Film</label>

          <input type="radio" name="kind" id="kind-series" value="series" />
          <label for="kind-series">📺 Series</label>

          <input type="radio" name="kind" id="kind-music" value="music" />
          <label for="kind-music">🎵 Music</label>

          <input type="radio" name="kind" id="kind-audiobook" value="audiobook" />
          <label for="kind-audiobook">📖 Audiobook</label>


        </div>
      </form>
      <div id="results"></div>
    </section>
  </main>
  <script>
    (function () {
      const YT_HOSTS = new Set([
        "youtube.com",
        "www.youtube.com",
        "m.youtube.com",
        "music.youtube.com",
        "youtu.be",
      ]);

      function isYoutubeUrl(raw) {
        const trimmed = raw.trim();
        if (!trimmed) return false;
        let parsed;
        try { parsed = new URL(trimmed); } catch { return false; }
        if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return false;
        return YT_HOSTS.has(parsed.hostname);
      }

      function isAria2cUrl(raw) {
        const trimmed = raw.trim();
        if (!trimmed) return false;
        if (trimmed.toLowerCase().startsWith("magnet:")) return true;
        let parsed;
        try { parsed = new URL(trimmed); } catch { return false; }
        return parsed.protocol === "http:" || parsed.protocol === "https:" || parsed.protocol === "ftp:";
      }

      const form = document.querySelector("form");
      const urlInput = form.querySelector('input[name="url"]');
      const results = document.getElementById("results");
      const submitBtn = form.querySelector("button[type=submit]");

      function setStatus(text) {
        results.replaceChildren();
        if (!text) return;
        const p = document.createElement("p");
        p.className = "result-status";
        p.textContent = text;
        results.appendChild(p);
      }

      function renderResults(items) {
        results.replaceChildren();
        if (!items.length) {
          setStatus("No seeded torrents found.");
          return;
        }
        for (const item of items) {
          const btn = document.createElement("button");
          btn.type = "button";
          btn.className = "result";
          const name = document.createElement("span");
          name.className = "result-name";
          name.textContent = item.name;
          const meta = document.createElement("span");
          meta.className = "result-meta";
          meta.textContent = item.size_label + " · " + item.seeders + " seeds · " + item.leechers + " leeches";
          btn.appendChild(name);
          btn.appendChild(meta);
          btn.addEventListener("click", function () {
            urlInput.value = item.magnet;
            form.submit();
          });
          results.appendChild(btn);
        }
      }

      form.addEventListener("submit", async function (e) {
        const q = urlInput.value;
        if (isYoutubeUrl(q) || isAria2cUrl(q)) return;
        e.preventDefault();
        setStatus("Searching…");
        submitBtn.disabled = true;
        try {
          const res = await fetch("/search?q=" + encodeURIComponent(q.trim()));
          const data = await res.json();
          if (!res.ok) {
            setStatus(data.error || "Search failed.");
            return;
          }
          renderResults(data.results || []);
        } catch (err) {
          setStatus("Search failed.");
        } finally {
          submitBtn.disabled = false;
        }
      });
    })();
  </script>
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

const DUMMY_INFO_HASH: &str = "0000000000000000000000000000000000000000";

const TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.openbittorrent.com:6969/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://tracker.torrent.eu.org:451/announce",
];

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

#[derive(Deserialize)]
struct ApiBayTorrent {
    id: String,
    name: String,
    info_hash: String,
    seeders: String,
    leechers: String,
    size: String,
}

#[derive(Serialize)]
struct SearchResult {
    name: String,
    magnet: String,
    seeders: u64,
    leechers: u64,
    size_label: String,
}

#[derive(Serialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Serialize)]
struct SearchError {
    error: String,
}

fn magnet_link(info_hash: &str, name: &str) -> String {
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    ser.append_pair("dn", name);
    for tracker in TRACKERS {
        ser.append_pair("tr", tracker);
    }
    format!("magnet:?xt=urn:btih:{info_hash}&{}", ser.finish())
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn search_error(status: StatusCode, message: String) -> Response {
    (status, Json(SearchError { error: message })).into_response()
}

async fn search(State(state): State<AppState>, Query(query): Query<SearchQuery>) -> Response {
    let q = query.q.trim();
    if q.is_empty() {
        return search_error(StatusCode::BAD_REQUEST, "missing search query".into());
    }

    info!("searching apibay for {q:?}");

    let response = match state
        .http
        .get("https://apibay.org/q.php")
        .query(&[("q", q)])
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            error!("apibay request failed: {e}");
            return search_error(StatusCode::BAD_GATEWAY, format!("search failed: {e}"));
        }
    };

    if !response.status().is_success() {
        error!("apibay returned {}", response.status());
        return search_error(
            StatusCode::BAD_GATEWAY,
            format!("search failed: HTTP {}", response.status()),
        );
    }

    let torrents: Vec<ApiBayTorrent> = match response.json().await {
        Ok(torrents) => torrents,
        Err(e) => {
            error!("apibay json parse failed: {e}");
            return search_error(StatusCode::BAD_GATEWAY, format!("search failed: {e}"));
        }
    };

    let mut results: Vec<SearchResult> = torrents
        .into_iter()
        .filter_map(|torrent| {
            if torrent.id == "0" || torrent.info_hash.eq_ignore_ascii_case(DUMMY_INFO_HASH) {
                return None;
            }
            let seeders = torrent.seeders.parse::<u64>().unwrap_or(0);
            if seeders == 0 {
                return None;
            }
            let leechers = torrent.leechers.parse::<u64>().unwrap_or(0);
            let size = torrent.size.parse::<u64>().unwrap_or(0);
            Some(SearchResult {
                name: torrent.name.clone(),
                magnet: magnet_link(&torrent.info_hash, &torrent.name),
                seeders,
                leechers,
                size_label: format_size(size),
            })
        })
        .collect();

    results.sort_by(|a, b| b.seeders.cmp(&a.seeders));
    Json(SearchResponse { results }).into_response()
}

fn new_staging_dir(download_dir: &str) -> String {
    let n = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{download_dir}/{nanos}-{n}")
}

const STDERR_TAIL_LINES: usize = 20;
const STDERR_TAIL_MAX_BYTES: usize = 2048;

async fn run_command(cmd: &mut Command) -> std::io::Result<std::process::Output> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped()).output().await
}

fn stderr_tail(output: &std::process::Output) -> String {
    let text = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let start = lines.len().saturating_sub(STDERR_TAIL_LINES);
    let tail = lines[start..].join("\n");
    if tail.len() <= STDERR_TAIL_MAX_BYTES {
        return tail;
    }
    let mut start_byte = tail.len() - STDERR_TAIL_MAX_BYTES;
    while start_byte > 0 && !tail.is_char_boundary(start_byte) {
        start_byte -= 1;
    }
    format!("…{}", &tail[start_byte..])
}

fn with_stderr_tail(msg: String, output: &std::process::Output) -> String {
    let stderr = stderr_tail(output);
    if stderr.is_empty() {
        msg
    } else {
        format!("{msg}\n{stderr}")
    }
}

async fn prepare_staging(download_dir: &str) -> std::io::Result<String> {
    let staging = new_staging_dir(download_dir);
    tokio::fs::create_dir_all(&staging).await?;
    Ok(staging)
}

async fn move_staging_to_dest(staging_dir: &str, dest_dir: &str) -> Result<Vec<String>, String> {
    tokio::fs::create_dir_all(dest_dir)
        .await
        .map_err(|e| format!("failed to create destination {dest_dir}: {e}"))?;

    let mut entries = tokio::fs::read_dir(staging_dir)
        .await
        .map_err(|e| format!("failed to read staging dir {staging_dir}: {e}"))?;

    let mut names = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("failed to read staging dir {staging_dir}: {e}"))?
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let src = entry.path();
        let dest = Path::new(dest_dir).join(entry.file_name());
        let output = run_command(Command::new("mv").arg(&src).arg(&dest))
            .await
            .map_err(|e| format!("failed to spawn mv: {e}"))?;
        if !output.status.success() {
            return Err(with_stderr_tail(
                format!(
                    "mv {} {} failed with exit code {}",
                    src.display(),
                    dest.display(),
                    output.status.code().map_or(-1, |c| c),
                ),
                &output,
            ));
        }
        names.push(name);
    }

    tokio::fs::remove_dir_all(staging_dir)
        .await
        .map_err(|e| format!("failed to remove staging dir {staging_dir}: {e}"))?;
    Ok(names)
}

fn downloaded_name(names: &[String]) -> String {
    if names.is_empty() {
        "unknown".into()
    } else {
        names.join(", ")
    }
}

async fn finish_download(label: &'static str, url: &str, staging: &str, dest_dir: &str) {
    match move_staging_to_dest(staging, dest_dir).await {
        Ok(names) => {
            let name = downloaded_name(&names);
            info!("{label} completed successfully: {name} ({url} -> {dest_dir})");
            post_to_matrix(format!("Downloaded {label}: {name}"));
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
    post_to_matrix(format!("Downloading {label} {url}..."));
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

        match run_command(Command::new("yt-dlp").args(&args)).await {
            Ok(output) if output.status.success() => {
                finish_download(label, &url, &staging, &dest_dir).await;
            }
            Ok(output) => {
                let code = output.status.code().map_or(-1, |c| c);
                error!(
                    "{}",
                    with_stderr_tail(
                        format!(
                            "yt-dlp {label} failed (exit code {code}): {url} (staging {staging})"
                        ),
                        &output,
                    )
                );

                post_to_matrix(with_stderr_tail(
                    format!("Failed to download {label}: exit code {code}"),
                    &output,
                ));
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
    post_to_matrix(format!("Downloading {label} {url}..."));
    tokio::spawn(async move {
        let staging = match prepare_staging(&download_dir).await {
            Ok(s) => s,
            Err(e) => {
                error!("failed to create staging dir in {download_dir}: {e}");
                post_to_matrix(format!("Failed to download {label}: {e}"));
                return;
            }
        };

        match run_command(
            Command::new("aria2c")
                .arg("--dir")
                .arg(&staging)
                .arg("--continue=true")
                .arg("--seed-time=60")
                .arg(&url),
        )
        .await
        {
            Ok(output) if output.status.success() => {
                finish_download(label, &url, &staging, &dest_dir).await;
            }
            Ok(output) => {
                let code = output.status.code().map_or(-1, |c| c);
                error!(
                    "{}",
                    with_stderr_tail(
                        format!(
                            "aria2c {label} failed (exit code {code}): {url} (staging {staging})"
                        ),
                        &output,
                    )
                );

                post_to_matrix(with_stderr_tail(
                    format!("Failed to download {label} to {dest_dir}: exit code {code}"),
                    &output,
                ));
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
        match run_command(
            Command::new("/home/felix/.cargo/bin/t2")
                .arg("pub")
                .arg("string/matrix")
                .arg(&msg)
                .arg("tcp:10.0.0.2:9999"),
        )
        .await
        {
            Ok(output) if output.status.success() => {
                info!("matrix post completed successfully: {msg}");
            }
            Ok(output) => {
                error!(
                    "{}",
                    with_stderr_tail(
                        format!(
                            "matrix post failed (exit code {}): {msg}",
                            output.status.code().map_or(-1, |c| c),
                        ),
                        &output,
                    )
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

    let http = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0")
        .build()
        .expect("failed to build http client");

    let state = AppState {
        download_dir,
        music_dir,
        audiobook_dir,
        film_dir,
        series_dir,
        http,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/favicon.ico", get(favicon))
        .route("/search", get(search))
        .route("/yoik", post(yoik))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    info!("listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}
