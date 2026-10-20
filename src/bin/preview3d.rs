use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use clap::Parser;
use serde_json::{Value, json};
use xbrstudio::jar::{self, load_preview_pairs};

const HTML: &str = include_str!("preview3d.html");
const INDEX_PLACEHOLDER: &str = "/*__INDEX__*/null";

#[derive(Parser, Debug)]
#[command(
    name = "preview3d",
    about = "Preview upscaled entity textures on their 3D models in a browser: \
             `preview3d` serves a local site, `--export` writes one self-contained HTML file."
)]
struct Cli {
    /// Directory of entity model JSONs. Defaults to models/entity when present.
    #[arg(long)]
    models: Option<PathBuf>,

    /// Upscaled resource pack root (contains assets/minecraft/textures).
    #[arg(long, default_value = "minecraft-26.3-client_x4")]
    upscaled: PathBuf,

    /// Original client jar (original textures for the A/B comparison).
    #[arg(long, default_value = "minecraft-26.3-client.jar")]
    original: PathBuf,

    /// Only include textures whose path contains this substring.
    #[arg(long)]
    only: Option<String>,

    /// HTTP port for serve mode.
    #[arg(long, default_value_t = 8765)]
    port: u16,

    /// Write a single self-contained HTML file instead of serving.
    #[arg(long)]
    export: Option<PathBuf>,
}

struct Site {
    index: String,
    orig_textures: Arc<HashMap<String, Vec<u8>>>,
    textures_root: PathBuf,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let model_dir = cli.models.clone().or_else(jar::default_model_dir).ok_or(
        "no model directory found — pass --models <dir> (expected models/entity)",
    )?;
    let textures_root = cli.upscaled.join("assets/minecraft/textures");
    if !textures_root.is_dir() {
        return Err(format!(
            "upscaled textures not found at {} (pass --upscaled <pack root>)",
            textures_root.display()
        ));
    }

    let data = load_preview_pairs(&model_dir, &textures_root, &cli.original)?;
    let mut model_jsons: HashMap<String, Value> = HashMap::new();
    let mut pairs_json = Vec::new();
    for pair in &data.pairs {
        if let Some(filter) = &cli.only
            && !pair.id.contains(filter.as_str())
        {
            continue;
        }
        let Some(model) = data.models.get(&pair.model_source) else {
            continue;
        };
        model_jsons
            .entry(pair.model_source.clone())
            .or_insert_with(|| model.clone());
        pairs_json.push(json!({
            "id": pair.id,
            "name": pair.name,
            "model": pair.model_source,
            "orig": [pair.orig_size.0, pair.orig_size.1],
            "frame": pair.frame.map(|frame| [frame.0, frame.1, frame.2, frame.3]),
            "u": format!("/api/tex/{}/u", pair.id),
            "o": format!("/api/tex/{}/o", pair.id),
        }));
    }
    if pairs_json.is_empty() {
        return Err("no texture/model pairs matched".into());
    }
    let orig_textures: HashMap<String, Vec<u8>> = data
        .orig_textures
        .into_iter()
        .filter(|(id, _)| pairs_json.iter().any(|pair| pair["id"] == id.as_str()))
        .collect();
    let index = json!({ "pairs": pairs_json, "models": model_jsons });

    match &cli.export {
        Some(path) => export(&index, &pairs_json, &orig_textures, &textures_root, path),
        None => serve(index, orig_textures, textures_root, cli.port),
    }
}

fn export(
    index: &Value,
    pairs_json: &[Value],
    orig_textures: &HashMap<String, Vec<u8>>,
    textures_root: &Path,
    path: &Path,
) -> Result<(), String> {
    let mut index = index.clone();
    let pairs = index
        .get_mut("pairs")
        .and_then(Value::as_array_mut)
        .expect("index built with pairs");
    for (slot, pair) in pairs.iter_mut().zip(pairs_json) {
        let id = pair["id"].as_str().unwrap_or_default();
        let upscaled =
            fs::read(textures_root.join(format!("{id}.png"))).map_err(|error| {
                format!("cannot read upscaled texture {id}.png: {error}")
            })?;
        let orig = orig_textures.get(id).ok_or_else(|| format!("missing original {id}"))?;
        slot["u"] = Value::String(data_url(&upscaled));
        slot["o"] = Value::String(data_url(orig));
    }

    let payload = serde_json::to_string(&index).map_err(|error| error.to_string())?;
    let html = HTML.replace(INDEX_PLACEHOLDER, &payload);
    fs::write(path, html).map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    println!(
        "wrote {} ({} pairs, {:.1} MB)",
        path.display(),
        pairs_json.len(),
        fs::metadata(path).map(|meta| meta.len() as f64 / 1e6).unwrap_or(0.0)
    );
    Ok(())
}

fn serve(
    index: Value,
    orig_textures: HashMap<String, Vec<u8>>,
    textures_root: PathBuf,
    port: u16,
) -> Result<(), String> {
    let index = serde_json::to_string(&index).map_err(|error| error.to_string())?;
    let site = Arc::new(Site {
        index,
        orig_textures: Arc::new(orig_textures),
        textures_root,
    });

    let listener =
        TcpListener::bind(("0.0.0.0", port)).map_err(|error| format!("bind: {error}"))?;
    println!("preview3d serving on:");
    println!("  http://127.0.0.1:{port}/");
    if let Some(lan) = lan_ip() {
        println!("  http://{lan}:{port}/   (same network, e.g. phone)");
    }
    println!("press Ctrl-C to stop");

    for stream in listener.incoming() {
        let site = Arc::clone(&site);
        thread::spawn(move || {
            if let Ok(stream) = stream {
                let _ = handle_client(stream, &site);
            }
        });
    }
    Ok(())
}

fn lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:80").ok()?;
    let addr = socket.local_addr().ok()?;
    match addr.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() => Some(ip.to_string()),
        _ => None,
    }
}

fn handle_client(mut stream: TcpStream, site: &Site) -> Result<(), std::io::Error> {
    let mut buffer = [0u8; 8192];
    let mut read = 0usize;
    loop {
        let n = stream.read(&mut buffer[read..])?;
        if n == 0 {
            return Ok(());
        }
        read += n;
        if buffer[..read].windows(4).any(|w| w == b"\r\n\r\n") || read == buffer.len() {
            break;
        }
    }
    let request = String::from_utf8_lossy(&buffer[..read]);
    let Some(target) = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
    else {
        return respond(stream, 400, "text/plain", b"bad request");
    };
    let path = percent_decode(target.split('?').next().unwrap_or(target));

    if path == "/" || path == "/index.html" {
        respond(stream, 200, "text/html; charset=utf-8", HTML.as_bytes())
    } else if path == "/api/index" {
        respond(stream, 200, "application/json", site.index.as_bytes())
    } else if let Some(rest) = path.strip_prefix("/api/tex/") {
        serve_texture(stream, site, rest)
    } else {
        respond(stream, 404, "text/plain", b"not found")
    }
}

fn serve_texture(
    stream: TcpStream,
    site: &Site,
    rest: &str,
) -> Result<(), std::io::Error> {
    let Some((id, variant)) = rest.rsplit_once('/') else {
        return respond(stream, 404, "text/plain", b"not found");
    };
    let bytes = match variant {
        "o" => site.orig_textures.get(id).cloned(),
        "u" => fs::read(site.textures_root.join(format!("{id}.png"))).ok(),
        _ => None,
    };
    match bytes {
        Some(bytes) => respond(stream, 200, "image/png", &bytes),
        None => respond(stream, 404, "text/plain", b"not found"),
    }
}

fn respond(
    mut stream: TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), std::io::Error> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-cache\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn data_url(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64_encode(bytes))
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}
