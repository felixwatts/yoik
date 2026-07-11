use axum::{
    Form,
    Router,
    response::{Html, Redirect},
    routing::{get, post},
};
use serde::Deserialize;
use tokio::process::Command;
use std::process::Stdio;

const AUDIO_OUTPUT_DIR: &str = "/home/felix/data/audio/music";
const VIDEO_OUTPUT_DIR: &str = "/home/felix/data/video/youtube";

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Yoik</title>
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
      max-width: 520px;
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
      gap: 1rem;
    }
    h2 {
      font-size: 1rem;
      font-weight: 600;
      color: #aaa;
      text-transform: uppercase;
      letter-spacing: 0.08em;
    }
    form {
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
      <h2>Rip Audio</h2>
      <form method="post" action="/rip/audio">
        <input type="text" name="url" placeholder="YouTube URL" required />
        <button type="submit">Rip</button>
      </form>
    </section>

    <section>
      <h2>Rip Video</h2>
      <form method="post" action="/rip/video">
        <input type="text" name="url" placeholder="YouTube URL" required />
        <button type="submit">Rip</button>
      </form>
    </section>
  </main>
</body>
</html>"#;

#[derive(Deserialize)]
struct RipForm {
    url: String,
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

fn spawn_yt_dlp(args: Vec<String>) {
    tokio::spawn(async move {
        let _ = Command::new("yt-dlp")
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    });
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn rip_audio(Form(form): Form<RipForm>) -> Redirect {
    if is_valid_youtube_url(&form.url) {
        let output = format!("{}/%(title)s.%(ext)s", AUDIO_OUTPUT_DIR);
        spawn_yt_dlp(vec![
            "-x".into(),
            "--audio-format".into(),
            "mp3".into(),
            "--no-playlist".into(),
            "-o".into(),
            output,
            form.url,
        ]);
    }
    Redirect::to("/")
}

async fn rip_video(Form(form): Form<RipForm>) -> Redirect {
    if is_valid_youtube_url(&form.url) {
        let output = format!("{}/%(title)s.%(ext)s", VIDEO_OUTPUT_DIR);
        spawn_yt_dlp(vec![
            "-f".into(),
            "bestvideo+bestaudio/best".into(),
            "--merge-output-format".into(),
            "mp4".into(),
            "--no-playlist".into(),
            "-o".into(),
            output,
            form.url,
        ]);
    }
    Redirect::to("/")
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("YOIK_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);

    let app = Router::new()
        .route("/", get(index))
        .route("/rip/audio", post(rip_audio))
        .route("/rip/video", post(rip_video));

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    eprintln!("Listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}
