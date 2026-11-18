use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{header::SET_COOKIE, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use md5::{Digest, Md5};
use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    collections::{HashMap, HashSet},
    env,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
    http: reqwest::Client,
    is_admin_port: bool,
    static_dir: String,
    logins: Arc<Mutex<HashMap<String, LoginStrike>>>,
}

struct LoginStrike {
    fails: u32,
    open_at: Instant,
}

#[derive(Serialize, Deserialize, Clone)]
struct AppConfig {
    navidrome_url: String,
    navidrome_user: String,
    navidrome_pass: String,
    allow_navidrome_auth: bool,
    selected_folders: Vec<String>,
    fringe_percentage: u32,
    artist_cooldown: usize,
    min_station_tracks: usize,
}

#[derive(Serialize, Deserialize, Clone)]
struct Station {
    id: String,
    name: String,
    category: String,
    description: String,
    core_count: usize,
    fringe_count: usize,
    cover_id: String,
    is_favorite: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct TrackOut {
    id: String,
    title: String,
    artist: String,
    album: String,
    duration: u32,
    is_fringe: bool,
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct QueueQuery {
    count: Option<usize>,
    exclude_artists: Option<String>,
    exclude_tracks: Option<String>,
}

#[derive(Deserialize)]
struct LocalUserReq {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct FavReq {
    favorite: bool,
}

fn subsonic_auth_params(user: &str, pass: &str) -> Vec<(String, String)> {
    let salt: String = (0..8)
        .map(|_| rand::thread_rng().sample(rand::distributions::Alphanumeric) as char)
        .collect();
    let mut hasher = Md5::new();
    hasher.update(format!("{}{}", pass, salt).as_bytes());
    let token = hex::encode(hasher.finalize());
    vec![
        ("u".into(), user.into()),
        ("t".into(), token),
        ("s".into(), salt),
        ("v".into(), "1.16.1".into()),
        ("c".into(), "NaviRadio".into()),
        ("f".into(), "json".into()),
    ]
}

fn subsonic_password_params(user: &str, pass: &str) -> Vec<(String, String)> {
    vec![
        ("u".into(), user.into()),
        ("p".into(), pass.into()),
        ("v".into(), "1.16.1".into()),
        ("c".into(), "NaviRadio".into()),
        ("f".into(), "json".into()),
    ]
}

fn nd_auth_params(use_password: bool, user: &str, pass: &str) -> Vec<(String, String)> {
    if use_password {
        subsonic_password_params(user, pass)
    } else {
        subsonic_auth_params(user, pass)
    }
}

fn is_auth_error(message: &str) -> bool {
    let m = message.to_lowercase();
    m.contains("password") || m.contains("username") || m.contains("auth")
}

fn hash_pw(pw: &str) -> String {
    let mut h = Sha256::new();
    h.update(pw.as_bytes());
    hex::encode(h.finalize())
}

struct StyleInfo {
    key: &'static str,
    label: &'static str,
    family: &'static str,
    family_label: &'static str,
    phrases: &'static [&'static str],
}

const STYLES: &[StyleInfo] = &[
    StyleInfo { key: "pop-punk", label: "Pop-Punk", family: "punk", family_label: "Punk", phrases: &["pop punk"] },
    StyleInfo { key: "emo", label: "Emo", family: "punk", family_label: "Punk", phrases: &["emo"] },
    StyleInfo { key: "hardcore", label: "Hardcore", family: "punk", family_label: "Punk", phrases: &["post hardcore", "hardcore"] },
    StyleInfo { key: "punk", label: "Punk", family: "punk", family_label: "Punk", phrases: &["ska punk", "punk"] },
    StyleInfo { key: "indie-rock", label: "Indie Rock", family: "rock", family_label: "Rock", phrases: &["indie rock"] },
    StyleInfo { key: "alt-rock", label: "Alternative Rock", family: "rock", family_label: "Rock", phrases: &["alternative rock", "alt rock"] },
    StyleInfo { key: "classic-rock", label: "Classic Rock", family: "rock", family_label: "Rock", phrases: &["classic rock"] },
    StyleInfo { key: "hard-rock", label: "Hard Rock", family: "rock", family_label: "Rock", phrases: &["hard rock"] },
    StyleInfo { key: "grunge", label: "Grunge", family: "rock", family_label: "Rock", phrases: &["grunge"] },
    StyleInfo { key: "britpop", label: "Britpop", family: "rock", family_label: "Rock", phrases: &["britpop", "brit pop"] },
    StyleInfo { key: "pop-rock", label: "Pop Rock", family: "rock", family_label: "Rock", phrases: &["pop rock"] },
    StyleInfo { key: "garage-rock", label: "Garage Rock", family: "rock", family_label: "Rock", phrases: &["garage rock"] },
    StyleInfo { key: "prog-rock", label: "Progressive Rock", family: "rock", family_label: "Rock", phrases: &["progressive rock", "prog rock"] },
    StyleInfo { key: "psych-rock", label: "Psychedelic Rock", family: "rock", family_label: "Rock", phrases: &["psychedelic rock", "psych rock", "psychedelic"] },
    StyleInfo { key: "southern-rock", label: "Southern Rock", family: "rock", family_label: "Rock", phrases: &["southern rock", "swamp rock"] },
    StyleInfo { key: "blues-rock", label: "Blues Rock", family: "rock", family_label: "Rock", phrases: &["blues rock"] },
    StyleInfo { key: "folk-rock", label: "Folk Rock", family: "rock", family_label: "Rock", phrases: &["folk rock"] },
    StyleInfo { key: "rock", label: "Rock", family: "rock", family_label: "Rock", phrases: &["rock and roll", "rock n roll", "rock"] },
    StyleInfo { key: "heavy-metal", label: "Heavy Metal", family: "metal", family_label: "Metal", phrases: &["heavy metal"] },
    StyleInfo { key: "death-metal", label: "Death Metal", family: "metal", family_label: "Metal", phrases: &["death metal"] },
    StyleInfo { key: "black-metal", label: "Black Metal", family: "metal", family_label: "Metal", phrases: &["black metal"] },
    StyleInfo { key: "thrash", label: "Thrash Metal", family: "metal", family_label: "Metal", phrases: &["thrash metal", "thrash"] },
    StyleInfo { key: "doom", label: "Doom Metal", family: "metal", family_label: "Metal", phrases: &["doom metal", "doom"] },
    StyleInfo { key: "metalcore", label: "Metalcore", family: "metal", family_label: "Metal", phrases: &["metalcore"] },
    StyleInfo { key: "nu-metal", label: "Nu Metal", family: "metal", family_label: "Metal", phrases: &["nu metal"] },
    StyleInfo { key: "industrial", label: "Industrial", family: "metal", family_label: "Metal", phrases: &["industrial"] },
    StyleInfo { key: "metal", label: "Metal", family: "metal", family_label: "Metal", phrases: &["metal"] },
    StyleInfo { key: "hip-hop", label: "Hip-Hop", family: "hiphop", family_label: "Hip-Hop & Rap", phrases: &["hip hop", "hiphop"] },
    StyleInfo { key: "rap", label: "Rap", family: "hiphop", family_label: "Hip-Hop & Rap", phrases: &["g funk", "boom bap", "trap", "rap"] },
    StyleInfo { key: "shoegaze", label: "Shoegaze", family: "alternative", family_label: "Alternative & Indie", phrases: &["shoegaze", "dream pop"] },
    StyleInfo { key: "indie", label: "Indie", family: "alternative", family_label: "Alternative & Indie", phrases: &["indie"] },
    StyleInfo { key: "alternative", label: "Alternative", family: "alternative", family_label: "Alternative & Indie", phrases: &["alternative", "alt"] },
    StyleInfo { key: "electropop", label: "Electropop", family: "pop", family_label: "Pop", phrases: &["electro pop", "electropop", "synth pop", "synthpop", "dance pop"] },
    StyleInfo { key: "kpop", label: "K-Pop", family: "pop", family_label: "Pop", phrases: &["k pop", "kpop"] },
    StyleInfo { key: "pop", label: "Pop", family: "pop", family_label: "Pop", phrases: &["pop"] },
    StyleInfo { key: "drum-bass", label: "Drum and Bass", family: "electronic", family_label: "Electronic", phrases: &["drum and bass", "drum n bass"] },
    StyleInfo { key: "dubstep", label: "Dubstep", family: "electronic", family_label: "Electronic", phrases: &["dubstep"] },
    StyleInfo { key: "house", label: "House", family: "electronic", family_label: "Electronic", phrases: &["house"] },
    StyleInfo { key: "techno", label: "Techno", family: "electronic", family_label: "Electronic", phrases: &["techno"] },
    StyleInfo { key: "trance", label: "Trance", family: "electronic", family_label: "Electronic", phrases: &["trance"] },
    StyleInfo { key: "ambient", label: "Ambient", family: "electronic", family_label: "Electronic", phrases: &["ambient"] },
    StyleInfo { key: "electronic", label: "Electronic", family: "electronic", family_label: "Electronic", phrases: &["electronica", "electronic", "edm"] },
    StyleInfo { key: "rnb", label: "R&B", family: "soul", family_label: "Soul & R&B", phrases: &["rhythm and blues", "r&b", "rnb"] },
    StyleInfo { key: "funk", label: "Funk", family: "soul", family_label: "Soul & R&B", phrases: &["funk"] },
    StyleInfo { key: "soul", label: "Soul", family: "soul", family_label: "Soul & R&B", phrases: &["neo soul", "motown", "soul"] },
    StyleInfo { key: "country", label: "Country", family: "country", family_label: "Country", phrases: &["outlaw country", "bluegrass", "americana", "country"] },
    StyleInfo { key: "folk", label: "Folk", family: "folk", family_label: "Folk", phrases: &["singer songwriter", "folk"] },
    StyleInfo { key: "blues", label: "Blues", family: "blues", family_label: "Blues", phrases: &["blues"] },
    StyleInfo { key: "jazz", label: "Jazz", family: "jazz", family_label: "Jazz", phrases: &["jazz fusion", "bebop", "jazz"] },
    StyleInfo { key: "reggae", label: "Reggae", family: "reggae", family_label: "Reggae", phrases: &["dancehall", "reggae", "ska"] },
    StyleInfo { key: "classical", label: "Classical", family: "classical", family_label: "Classical", phrases: &["classical", "baroque", "opera"] },
    StyleInfo { key: "soundtrack", label: "Soundtrack", family: "soundtrack", family_label: "Soundtrack", phrases: &["soundtrack", "score"] },
    StyleInfo { key: "latin", label: "Latin", family: "latin", family_label: "Latin", phrases: &["reggaeton", "bachata", "cumbia", "salsa", "latin"] },
];

fn normalize_genre_text(raw: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for ch in raw.trim().to_lowercase().chars() {
        let mapped = match ch {
            '_' | '/' | ';' | '|' | '-' => ' ',
            other => other,
        };
        if mapped.is_whitespace() {
            if !out.is_empty() && !space {
                out.push(' ');
                space = true;
            }
        } else {
            out.push(mapped);
            space = false;
        }
    }
    out.trim().to_string()
}

fn find_phrase(text: &str, phrase: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(pos) = text[start..].find(phrase) {
        let abs = start + pos;
        let before_ok = abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric();
        let after = abs + phrase.len();
        let after_ok = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return Some(abs);
        }
        start = abs + phrase.len().max(1);
    }
    None
}

fn classify_style(raw: &str) -> Option<&'static StyleInfo> {
    let text = normalize_genre_text(raw);
    if text.is_empty() || matches!(text.as_str(), "unknown" | "other" | "various" | "misc" | "general") {
        return None;
    }
    let mut best: Option<(usize, usize, usize)> = None;
    for (index, style) in STYLES.iter().enumerate() {
        for phrase in style.phrases {
            if let Some(pos) = find_phrase(&text, phrase) {
                let rank = (pos, usize::MAX - phrase.len(), index);
                if best.map(|current| rank < current).unwrap_or(true) {
                    best = Some(rank);
                }
            }
        }
    }
    best.map(|(_, _, index)| &STYLES[index])
}

fn style_by_key(key: &str) -> Option<&'static StyleInfo> {
    STYLES.iter().find(|style| style.key == key)
}

fn family_neighbor(family: &str) -> Option<&'static str> {
    match family {
        "punk" | "alternative" | "metal" | "blues" | "folk" => Some("rock"),
        "electronic" => Some("pop"),
        "country" => Some("folk"),
        "reggae" => Some("soul"),
        "soul" => Some("hiphop"),
        _ => None,
    }
}

struct StationBucket {
    label: String,
    family: String,
    track_ids: Vec<String>,
    cover: String,
    style_labels: HashSet<String>,
}

fn init_db(path: &str) -> Connection {
    let conn = Connection::open(path).expect("Failed to open SQLite DB");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS local_users (
            username TEXT PRIMARY KEY,
            password_hash TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            username TEXT NOT NULL,
            created_at INTEGER DEFAULT (strftime('%s','now'))
        );
        CREATE TABLE IF NOT EXISTS tracks (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            artist TEXT NOT NULL,
            artist_id TEXT NOT NULL,
            album TEXT NOT NULL,
            genre TEXT NOT NULL,
            cluster TEXT NOT NULL,
            duration INTEGER NOT NULL,
            cover_id TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS similar_artists (
            artist_name TEXT NOT NULL,
            similar_name TEXT NOT NULL,
            PRIMARY KEY (artist_name, similar_name)
        );
        CREATE TABLE IF NOT EXISTS stations (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            category TEXT NOT NULL,
            description TEXT NOT NULL,
            core_tracks_json TEXT NOT NULL,
            fringe_tracks_json TEXT NOT NULL,
            cover_id TEXT NOT NULL,
            is_favorite INTEGER DEFAULT 0
        );
        ",
    )
    .expect("Failed to initialize database schema");
    conn
}

fn load_config(conn: &Connection) -> AppConfig {
    let get_val = |k: &str, def: &str| -> String {
        conn.query_row("SELECT value FROM settings WHERE key = ?1", params![k], |r| {
            r.get(0)
        })
        .unwrap_or_else(|_| def.to_string())
    };

    let def_url = env::var("DEFAULT_ND_URL").unwrap_or_default();
    let def_user = env::var("DEFAULT_ND_USER").unwrap_or_default();
    let def_pass = env::var("DEFAULT_ND_PASS").unwrap_or_default();
    let def_fringe = env::var("DEFAULT_FRINGE").unwrap_or_else(|_| "25".into());
    let def_cool = env::var("DEFAULT_COOLDOWN").unwrap_or_else(|_| "6".into());
    let def_min = env::var("DEFAULT_MIN_TRACKS").unwrap_or_else(|_| "100".into());

    let folders_raw = get_val("selected_folders", "[]");
    let selected_folders: Vec<String> = serde_json::from_str(&folders_raw).unwrap_or_default();

    AppConfig {
        navidrome_url: get_val("navidrome_url", &def_url).trim_end_matches('/').to_string(),
        navidrome_user: get_val("navidrome_user", &def_user),
        navidrome_pass: get_val("navidrome_pass", &def_pass),
        allow_navidrome_auth: get_val("allow_navidrome_auth", "true") == "true",
        selected_folders,
        fringe_percentage: get_val("fringe_percentage", &def_fringe).parse().unwrap_or(25),
        artist_cooldown: get_val("artist_cooldown", &def_cool).parse().unwrap_or(6),
        min_station_tracks: get_val("min_station_tracks", &def_min)
            .parse::<usize>()
            .unwrap_or(100)
            .max(100),
    }
}

fn save_setting(conn: &Connection, key: &str, value: &str) {
    let _ = conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    );
}

fn is_private_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> String {
    if is_private_peer(peer.ip()) {
        if let Some(real) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            let ip = real.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
        if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = forwarded.split(',').next() {
                let ip = first.trim();
                if !ip.is_empty() {
                    return ip.to_string();
                }
            }
        }
    }
    peer.ip().to_string()
}

fn pause_after(fails: u32) -> Duration {
    let secs = match fails {
        0..=4 => 0,
        5..=7 => 60,
        8..=11 => 300,
        _ => 900,
    };
    Duration::from_secs(secs)
}

fn login_pause_secs(guard: &HashMap<String, LoginStrike>, ip: &str) -> Option<u64> {
    let strike = guard.get(ip)?;
    let now = Instant::now();
    if strike.open_at > now {
        Some((strike.open_at - now).as_secs().max(1))
    } else {
        None
    }
}

fn note_login_failure(guard: &mut HashMap<String, LoginStrike>, ip: &str) {
    if guard.len() > 256 {
        let now = Instant::now();
        guard.retain(|_, strike| strike.open_at > now);
    }
    let strike = guard.entry(ip.to_string()).or_insert(LoginStrike {
        fails: 0,
        open_at: Instant::now(),
    });
    if strike.open_at + Duration::from_secs(1800) < Instant::now() {
        strike.fails = 0;
    }
    strike.fails = strike.fails.saturating_add(1);
    strike.open_at = Instant::now() + pause_after(strike.fails);
}

fn pause_message(seconds: u64) -> String {
    let wait = if seconds < 90 {
        "about a minute".to_string()
    } else {
        format!("about {} minutes", (seconds + 59) / 60)
    };
    format!(
        "Too many wrong passwords from this address. Try again in {wait}. The account is not locked."
    )
}

fn request_is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(',')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("https")
        })
        .unwrap_or(false)
}

fn session_cookie(token: &str, secure: bool) -> HeaderValue {
    let mut raw = format!(
        "naviradio_session={token}; HttpOnly; Path=/; SameSite=Lax; Max-Age=2592000"
    );
    if secure {
        raw.push_str("; Secure");
    }
    HeaderValue::from_str(&raw).expect("session cookie")
}

fn clear_session_cookie(secure: bool) -> HeaderValue {
    let mut raw = "naviradio_session=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0".to_string();
    if secure {
        raw.push_str("; Secure");
    }
    HeaderValue::from_str(&raw).expect("clear cookie")
}

fn presented_token(headers: &HeaderMap) -> Option<String> {
    if let Some(auth) = headers.get("authorization").and_then(|h| h.to_str().ok()) {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    let cookie = headers.get("cookie").and_then(|h| h.to_str().ok())?;
    for part in cookie.split(';') {
        if let Some(val) = part.trim().strip_prefix("naviradio_session=") {
            let val = val.trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn session_is_valid(state: &AppState, token: &str) -> bool {
    let conn = state.db.lock().unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE token = ?1",
        params![token],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

fn known_media_id(state: &AppState, id: &str) -> bool {
    let conn = state.db.lock().unwrap();
    let in_tracks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tracks WHERE id = ?1 OR cover_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if in_tracks > 0 {
        return true;
    }
    let in_stations: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM stations WHERE cover_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    in_stations > 0
}

fn is_authenticated(state: &AppState, headers: &HeaderMap) -> bool {
    if state.is_admin_port {
        return true;
    }
    match presented_token(headers) {
        Some(token) => session_is_valid(state, &token),
        None => false,
    }
}

async fn index_html(State(state): State<AppState>) -> impl IntoResponse {
    let path = format!("{}/index.html", state.static_dir);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => Html(content).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Missing index.html").into_response(),
    }
}

async fn api_context(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let token = presented_token(&headers);
    let authed = if state.is_admin_port {
        true
    } else {
        token
            .as_deref()
            .map(|t| session_is_valid(&state, t))
            .unwrap_or(false)
    };
    let conn = state.db.lock().unwrap();
    let track_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0))
        .unwrap_or(0);
    let station_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM stations", [], |r| r.get(0))
        .unwrap_or(0);
    drop(conn);

    let mut resp = Json(serde_json::json!({
        "is_admin": state.is_admin_port,
        "authenticated": authed,
        "track_count": track_count,
        "station_count": station_count
    }))
    .into_response();
    if authed && !state.is_admin_port {
        if let Some(token) = token {
            resp.headers_mut().insert(
                SET_COOKIE,
                session_cookie(&token, request_is_https(&headers)),
            );
        }
    }
    resp
}

fn finish_login(token: String, username: &str, headers: &HeaderMap) -> Response {
    let mut resp = Json(serde_json::json!({ "token": token, "username": username })).into_response();
    resp.headers_mut().insert(
        SET_COOKIE,
        session_cookie(&token, request_is_https(headers)),
    );
    resp
}

async fn api_login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> Result<Response, (StatusCode, String)> {
    let ip = client_ip(&headers, peer);
    {
        let guard = state.logins.lock().unwrap();
        if let Some(seconds) = login_pause_secs(&guard, &ip) {
            log_ha(&format!("login paused ip={ip} seconds={seconds}"));
            return Err((StatusCode::TOO_MANY_REQUESTS, pause_message(seconds)));
        }
    }

    let cfg = {
        let conn = state.db.lock().unwrap();
        let pw_hash = hash_pw(&req.password);
        let local_ok: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM local_users WHERE username = ?1 AND password_hash = ?2",
                params![req.username.trim(), pw_hash],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if local_ok > 0 {
            let token = uuid::Uuid::new_v4().to_string();
            let _ = conn.execute(
                "INSERT INTO sessions (token, username) VALUES (?1, ?2)",
                params![token, req.username.trim()],
            );
            drop(conn);
            state.logins.lock().unwrap().remove(&ip);
            return Ok(finish_login(token, req.username.trim(), &headers));
        }
        load_config(&conn)
    };

    if !cfg.allow_navidrome_auth {
        note_login_failure(&mut state.logins.lock().unwrap(), &ip);
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".into()));
    }

    let params = subsonic_auth_params(req.username.trim(), &req.password);
    let url = format!("{}/rest/ping.view", cfg.navidrome_url);
    let resp = state
        .http
        .get(&url)
        .query(&params)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Navidrome unreachable: {}", e)))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| (StatusCode::BAD_GATEWAY, "Invalid Subsonic response".into()))?;

    if body["subsonic-response"]["status"].as_str() == Some("ok") {
        state.logins.lock().unwrap().remove(&ip);
        let token = uuid::Uuid::new_v4().to_string();
        let conn = state.db.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO sessions (token, username) VALUES (?1, ?2)",
            params![token, req.username.trim()],
        );
        Ok(finish_login(token, req.username.trim(), &headers))
    } else {
        note_login_failure(&mut state.logins.lock().unwrap(), &ip);
        log_ha(&format!("login failed ip={ip}"));
        Err((StatusCode::UNAUTHORIZED, "Invalid credentials".into()))
    }
}

async fn api_stations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Station>>, StatusCode> {
    if !is_authenticated(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let conn = state.db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, name, category, description, core_tracks_json, fringe_tracks_json, cover_id, is_favorite FROM stations ORDER BY is_favorite DESC, category ASC, name ASC")
        .unwrap();

    let list = stmt
        .query_map([], |row| {
            let core_raw: String = row.get(4)?;
            let fringe_raw: String = row.get(5)?;
            let core: Vec<String> = serde_json::from_str(&core_raw).unwrap_or_default();
            let fringe: Vec<String> = serde_json::from_str(&fringe_raw).unwrap_or_default();
            Ok(Station {
                id: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                description: row.get(3)?,
                core_count: core.len(),
                fringe_count: fringe.len(),
                cover_id: row.get(6)?,
                is_favorite: row.get::<_, i64>(7)? == 1,
            })
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    Ok(Json(list))
}

async fn api_toggle_favorite(
    State(state): State<AppState>,
    Path(station_id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<FavReq>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !is_authenticated(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let val = if req.favorite { 1 } else { 0 };
    let conn = state.db.lock().unwrap();
    let _ = conn.execute(
        "UPDATE stations SET is_favorite = ?1 WHERE id = ?2",
        params![val, station_id],
    );
    Ok(Json(serde_json::json!({ "status": "ok", "is_favorite": val })))
}

async fn api_station_tracks(
    State(state): State<AppState>,
    Path(station_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<TrackOut>>, StatusCode> {
    if !is_authenticated(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let conn = state.db.lock().unwrap();
    let (core_raw, fringe_raw): (String, String) = conn
        .query_row(
            "SELECT core_tracks_json, fringe_tracks_json FROM stations WHERE id = ?1",
            params![station_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let core_ids: Vec<String> = serde_json::from_str(&core_raw).unwrap_or_default();
    let fringe_ids: Vec<String> = serde_json::from_str(&fringe_raw).unwrap_or_default();

    let mut load = |ids: &[String], is_fringe: bool, out: &mut Vec<TrackOut>| {
        for tid in ids {
            if let Ok(t) = conn.query_row(
                "SELECT id, title, artist, album, duration FROM tracks WHERE id = ?1",
                params![tid],
                |r| {
                    Ok(TrackOut {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        artist: r.get(2)?,
                        album: r.get(3)?,
                        duration: r.get(4)?,
                        is_fringe,
                    })
                },
            ) {
                out.push(t);
            }
        }
    };

    let mut tracks = Vec::new();
    load(&core_ids, false, &mut tracks);
    load(&fringe_ids, true, &mut tracks);
    tracks.sort_by(|a, b| {
        a.artist
            .to_lowercase()
            .cmp(&b.artist.to_lowercase())
            .then(a.album.to_lowercase().cmp(&b.album.to_lowercase()))
            .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    Ok(Json(tracks))
}

async fn api_station_queue(
    State(state): State<AppState>,
    Path(station_id): Path<String>,
    Query(q): Query<QueueQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<TrackOut>>, StatusCode> {
    if !is_authenticated(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let batch_size = q.count.unwrap_or(20).min(100);
    let conn = state.db.lock().unwrap();
    let cfg = load_config(&conn);

    let (core_raw, fringe_raw): (String, String) = conn
        .query_row(
            "SELECT core_tracks_json, fringe_tracks_json FROM stations WHERE id = ?1",
            params![station_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let core_ids: Vec<String> = serde_json::from_str(&core_raw).unwrap_or_default();
    let fringe_ids: Vec<String> = serde_json::from_str(&fringe_raw).unwrap_or_default();

    let mut load_tracks = |ids: &[String], is_fringe: bool| -> Vec<TrackOut> {
        let mut out = Vec::new();
        for tid in ids {
            if let Ok(t) = conn.query_row(
                "SELECT id, title, artist, album, duration FROM tracks WHERE id = ?1",
                params![tid],
                |r| {
                    Ok(TrackOut {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        artist: r.get(2)?,
                        album: r.get(3)?,
                        duration: r.get(4)?,
                        is_fringe,
                    })
                },
            ) {
                out.push(t);
            }
        }
        out
    };

    let mut core_pool = load_tracks(&core_ids, false);
    let mut fringe_pool = load_tracks(&fringe_ids, true);

    let mut rng = rand::thread_rng();
    core_pool.shuffle(&mut rng);
    fringe_pool.shuffle(&mut rng);

    let mut recent_artists: Vec<String> = q
        .exclude_artists
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();

    let mut used_tracks: HashSet<String> = q
        .exclude_tracks
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut queue = Vec::new();
    let cooldown = cfg.artist_cooldown;

    for _ in 0..batch_size {
        let want_fringe =
            !fringe_pool.is_empty() && rng.gen_range(0..100) < cfg.fringe_percentage;
        let (primary, secondary) = if want_fringe {
            (&mut fringe_pool, &mut core_pool)
        } else {
            (&mut core_pool, &mut fringe_pool)
        };

        let pick_idx = primary
            .iter()
            .position(|t| {
                !used_tracks.contains(&t.id)
                    && !recent_artists.contains(&t.artist.to_lowercase())
            })
            .or_else(|| {
                primary
                    .iter()
                    .position(|t| !recent_artists.contains(&t.artist.to_lowercase()))
            });

        if let Some(idx) = pick_idx {
            let chosen = primary.remove(idx);
            used_tracks.insert(chosen.id.clone());
            recent_artists.push(chosen.artist.to_lowercase());
            if recent_artists.len() > cooldown {
                recent_artists.remove(0);
            }
            primary.push(chosen.clone());
            queue.push(chosen);
        } else if let Some(idx) = secondary
            .iter()
            .position(|t| !recent_artists.contains(&t.artist.to_lowercase()))
            .or(if !primary.is_empty() { Some(0) } else { None })
        {
            let pool = if idx < secondary.len()
                && !recent_artists.contains(&secondary[idx].artist.to_lowercase())
            {
                secondary
            } else {
                primary
            };
            if !pool.is_empty() {
                let safe_idx = idx.min(pool.len() - 1);
                let chosen = pool.remove(safe_idx);
                used_tracks.insert(chosen.id.clone());
                recent_artists.push(chosen.artist.to_lowercase());
                if recent_artists.len() > cooldown {
                    recent_artists.remove(0);
                }
                pool.push(chosen.clone());
                queue.push(chosen);
            }
        }
    }

    Ok(Json(queue))
}

async fn api_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = presented_token(&headers) {
        let conn = state.db.lock().unwrap();
        let _ = conn.execute("DELETE FROM sessions WHERE token = ?1", params![token]);
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().insert(
        SET_COOKIE,
        clear_session_cookie(request_is_https(&headers)),
    );
    resp
}

async fn proxy_stream(
    State(state): State<AppState>,
    Path(track_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    if !is_authenticated(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if !known_media_id(&state, &track_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let cfg = {
        let conn = state.db.lock().unwrap();
        load_config(&conn)
    };
    let params = subsonic_auth_params(&cfg.navidrome_user, &cfg.navidrome_pass);
    let mut url = reqwest::Url::parse(&format!("{}/rest/stream.view", cfg.navidrome_url))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    {
        let mut qp = url.query_pairs_mut();
        for (k, v) in params {
            qp.append_pair(&k, &v);
        }
        qp.append_pair("id", &track_id);
    }

    let mut req = state.http.get(url);
    if let Some(range) = headers.get("Range") {
        req = req.header("Range", range);
    }

    let upstream = req.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let status = StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::OK);
    let mut resp_builder = Response::builder().status(status);

    for h in ["content-type", "content-length", "accept-ranges", "content-range"] {
        if let Some(val) = upstream.headers().get(h) {
            resp_builder = resp_builder.header(h, val);
        }
    }

    let body = Body::from_stream(upstream.bytes_stream());
    resp_builder.body(body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn proxy_cover(
    State(state): State<AppState>,
    Path(cover_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    if !is_authenticated(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if !known_media_id(&state, &cover_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let cfg = {
        let conn = state.db.lock().unwrap();
        load_config(&conn)
    };
    let mut params = subsonic_auth_params(&cfg.navidrome_user, &cfg.navidrome_pass);
    params.push(("id".into(), cover_id));
    params.push(("size".into(), "300".into()));

    let url = format!("{}/rest/getCoverArt.view", cfg.navidrome_url);
    let upstream = state
        .http
        .get(&url)
        .query(&params)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let mut resp_builder = Response::builder().status(StatusCode::OK);
    if let Some(ct) = upstream.headers().get("content-type") {
        resp_builder = resp_builder.header("content-type", ct);
    }
    let body = Body::from_stream(upstream.bytes_stream());
    resp_builder.body(body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn admin_get_config(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = {
        let conn = state.db.lock().unwrap();
        load_config(&conn)
    };
    let params = subsonic_auth_params(&cfg.navidrome_user, &cfg.navidrome_pass);
    let folder_url = format!("{}/rest/getMusicFolders.view", cfg.navidrome_url);
    let mut folders = Vec::new();
    if let Ok(resp) = state.http.get(&folder_url).query(&params).send().await {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(arr) = json["subsonic-response"]["musicFolders"]["musicFolder"].as_array() {
                for f in arr {
                    folders.push(serde_json::json!({
                        "id": f["id"].to_string().trim_matches('"'),
                        "name": f["name"].as_str().unwrap_or("Library")
                    }));
                }
            }
        }
    }

    let users: Vec<String> = {
        let conn = state.db.lock().unwrap();
        let mut stmt = conn.prepare("SELECT username FROM local_users ORDER BY username").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().filter_map(Result::ok).collect()
    };

    Json(serde_json::json!({
        "config": {
            "navidrome_url": cfg.navidrome_url,
            "navidrome_user": cfg.navidrome_user,
            "allow_navidrome_auth": cfg.allow_navidrome_auth,
            "selected_folders": cfg.selected_folders,
            "fringe_percentage": cfg.fringe_percentage,
            "artist_cooldown": cfg.artist_cooldown,
            "min_station_tracks": cfg.min_station_tracks,
            "password_saved": !cfg.navidrome_pass.is_empty()
        },
        "available_folders": folders,
        "local_users": users
    }))
}

async fn admin_save_config(
    State(state): State<AppState>,
    Json(cfg): Json<AppConfig>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    save_setting(&conn, "navidrome_url", cfg.navidrome_url.trim_end_matches('/'));
    save_setting(&conn, "navidrome_user", &cfg.navidrome_user);
    if !cfg.navidrome_pass.is_empty() {
        save_setting(&conn, "navidrome_pass", &cfg.navidrome_pass);
    }
    save_setting(
        &conn,
        "allow_navidrome_auth",
        if cfg.allow_navidrome_auth { "true" } else { "false" },
    );
    save_setting(
        &conn,
        "selected_folders",
        &serde_json::to_string(&cfg.selected_folders).unwrap_or_else(|_| "[]".into()),
    );
    save_setting(&conn, "fringe_percentage", &cfg.fringe_percentage.to_string());
    save_setting(&conn, "artist_cooldown", &cfg.artist_cooldown.to_string());
    save_setting(&conn, "min_station_tracks", &cfg.min_station_tracks.to_string());

    Json(serde_json::json!({ "status": "saved" }))
}

async fn admin_add_user(
    State(state): State<AppState>,
    Json(u): Json<LocalUserReq>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let _ = conn.execute(
        "INSERT INTO local_users (username, password_hash) VALUES (?1, ?2)
         ON CONFLICT(username) DO UPDATE SET password_hash = excluded.password_hash",
        params![u.username.trim(), hash_pw(&u.password)],
    );
    Json(serde_json::json!({ "status": "ok" }))
}

async fn admin_del_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let _ = conn.execute("DELETE FROM local_users WHERE username = ?1", params![username]);
    Json(serde_json::json!({ "status": "deleted" }))
}

fn json_items(value: &serde_json::Value) -> Vec<serde_json::Value> {
    match value {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::Object(_) => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn json_str(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn subsonic_failure(json: &serde_json::Value) -> Option<String> {
    let resp = &json["subsonic-response"];
    if resp.is_null() {
        return Some("Navidrome did not return a Subsonic response.".into());
    }
    if resp["status"].as_str() == Some("failed") {
        let message = resp["error"]["message"]
            .as_str()
            .unwrap_or("Navidrome rejected the request");
        return Some(message.to_string());
    }
    None
}

struct NdAuth {
    user: String,
    token: String,
    salt: String,
}

fn nd_params(auth: &NdAuth) -> Vec<(String, String)> {
    vec![
        ("u".into(), auth.user.clone()),
        ("t".into(), auth.token.clone()),
        ("s".into(), auth.salt.clone()),
        ("v".into(), "1.16.1".into()),
        ("c".into(), "NaviRadio".into()),
        ("f".into(), "json".into()),
    ]
}

fn log_ha(message: &str) {
    eprintln!("naviradio: {message}");
}

fn header_str(headers: &reqwest::header::HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

async fn navidrome_login(
    state: &AppState,
    base_url: &str,
    user: &str,
    pass: &str,
    typed_password: bool,
) -> Result<NdAuth, String> {
    let url = format!("{}/auth/login", base_url.trim_end_matches('/'));
    let source = if typed_password { "typed" } else { "saved" };
    log_ha(&format!("login start user={user:?} url={url} source={source}"));
    let resp = state
        .http
        .post(&url)
        .json(&serde_json::json!({ "username": user, "password": pass }))
        .send()
        .await
        .map_err(|e| {
            log_ha(&format!("login connection failed url={url} error={e}"));
            format!("Failed connecting to Navidrome: {}", e)
        })?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let limit = header_str(&headers, "x-ratelimit-limit");
    let remaining = header_str(&headers, "x-ratelimit-remaining");
    let reset = header_str(&headers, "x-ratelimit-reset");
    let body_text = resp.text().await.unwrap_or_default();
    log_ha(&format!(
        "login response http={} ratelimit={limit} remaining={remaining} reset={reset}",
        status.as_u16()
    ));
    if status.is_success() {
        let body: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|_| "Navidrome login did not return JSON.".to_string())?;
        let token = json_str(&body["subsonicToken"]);
        let salt = json_str(&body["subsonicSalt"]);
        if token.is_empty() || salt.is_empty() {
            log_ha("login succeeded but subsonicToken or subsonicSalt was missing");
            return Err("Navidrome accepted the login but did not return a Subsonic token.".into());
        }
        let name = json_str(&body["username"]);
        log_ha(&format!("login ok user={name:?}"));
        return Ok(NdAuth {
            user: if name.is_empty() {
                user.to_string()
            } else {
                name
            },
            token,
            salt,
        });
    }
    if status.as_u16() == 429 || remaining == "0" {
        return Err(
            "Navidrome temporarily blocked logins from this server after too many tries. Wait a minute, then rescan once. Details are in the NaviRadio add-on log."
                .into(),
        );
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        let source = if typed_password {
            "the password just typed into Navidrome Service Password"
        } else {
            "the saved password. The box starts empty, so type the current website password into Navidrome Service Password"
        };
        return Err(format!(
            "Navidrome refused {source} for username \"{user}\"."
        ));
    }
    Err(format!(
        "Navidrome login failed with HTTP {}. See the NaviRadio add-on log.",
        status
    ))
}

async fn subsonic_get(
    state: &AppState,
    base_url: &str,
    endpoint: &str,
    params: Vec<(String, String)>,
) -> Result<serde_json::Value, String> {
    let url = format!("{}/rest/{}", base_url.trim_end_matches('/'), endpoint);
    let body = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let resp = state
        .http
        .post(&url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Failed connecting to Navidrome: {}", e))?;
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| "Invalid JSON from Navidrome".to_string())?;
    if let Some(message) = subsonic_failure(&json) {
        return Err(message);
    }
    Ok(json)
}

#[derive(Deserialize, Default)]
struct ScanRequest {
    #[serde(default)]
    navidrome_url: String,
    #[serde(default)]
    navidrome_user: String,
    #[serde(default)]
    navidrome_pass: String,
    #[serde(default)]
    selected_folders: Option<Vec<String>>,
}

async fn perform_scan_and_build(state: &AppState, req: &ScanRequest) -> Result<(usize, usize), String> {
    let mut cfg = {
        let conn = state.db.lock().unwrap();
        load_config(&conn)
    };
    let url = req.navidrome_url.trim();
    if !url.is_empty() {
        cfg.navidrome_url = url.trim_end_matches('/').to_string();
    }
    let user = req.navidrome_user.trim();
    if !user.is_empty() {
        cfg.navidrome_user = user.to_string();
    }
    let typed_password = !req.navidrome_pass.is_empty();
    if !req.navidrome_pass.is_empty() {
        cfg.navidrome_pass = req.navidrome_pass.clone();
    }
    if let Some(folders) = &req.selected_folders {
        cfg.selected_folders = folders.clone();
    }
    cfg.navidrome_user = cfg.navidrome_user.trim().to_string();
    cfg.navidrome_url = cfg.navidrome_url.trim().trim_end_matches('/').to_string();

    if cfg.navidrome_pass.is_empty() {
        return Err(
            "Enter the Navidrome password in Navidrome Service Password, then rescan. The listener sign-in does not scan the library."
                .into(),
        );
    }
    if cfg.navidrome_user.is_empty() || cfg.navidrome_url.is_empty() {
        return Err("Navidrome URL and service username are required.".into());
    }

    let auth = navidrome_login(
        state,
        &cfg.navidrome_url,
        &cfg.navidrome_user,
        &cfg.navidrome_pass,
        typed_password,
    )
    .await?;
    cfg.navidrome_user = auth.user.clone();

    {
        let conn = state.db.lock().unwrap();
        save_setting(&conn, "navidrome_url", &cfg.navidrome_url);
        save_setting(&conn, "navidrome_user", &cfg.navidrome_user);
        save_setting(&conn, "navidrome_pass", &cfg.navidrome_pass);
        save_setting(
            &conn,
            "selected_folders",
            &serde_json::to_string(&cfg.selected_folders).unwrap_or_else(|_| "[]".into()),
        );
    }

    let mut all_tracks = Vec::new();
    let folders_to_scan = if cfg.selected_folders.is_empty() {
        vec![None]
    } else {
        cfg.selected_folders.iter().cloned().map(Some).collect()
    };

    // search3 with a blank query returns no songs on Navidrome. Walk albums instead.
    for folder_opt in folders_to_scan {
        let mut offset = 0usize;
        loop {
            let mut params = nd_params(&auth);
            params.push(("type".into(), "alphabeticalByName".into()));
            params.push(("size".into(), "500".into()));
            params.push(("offset".into(), offset.to_string()));
            if let Some(ref fid) = folder_opt {
                params.push(("musicFolderId".into(), fid.clone()));
            }

            let json = subsonic_get(state, &cfg.navidrome_url, "getAlbumList2.view", params).await?;
            let albums = json_items(&json["subsonic-response"]["albumList2"]["album"]);
            if albums.is_empty() {
                break;
            }

            for album in &albums {
                let album_id = json_str(&album["id"]);
                if album_id.is_empty() {
                    continue;
                }
                let mut params = nd_params(&auth);
                params.push(("id".into(), album_id));
                let album_json = subsonic_get(state, &cfg.navidrome_url, "getAlbum.view", params).await?;
                let songs = json_items(&album_json["subsonic-response"]["album"]["song"]);
                for s in &songs {
                    let id = json_str(&s["id"]);
                    let title = s["title"].as_str().unwrap_or("Unknown").to_string();
                    let artist = s["artist"].as_str().unwrap_or("Unknown").to_string();
                    let artist_id = json_str(&s["artistId"]);
                    let album_name = s["album"].as_str().unwrap_or("Unknown").to_string();
                    let genre = s["genre"].as_str().unwrap_or("").trim().to_string();
                    let duration = s["duration"]
                        .as_u64()
                        .or_else(|| s["duration"].as_f64().map(|n| n as u64))
                        .unwrap_or(180) as u32;
                    let cover_id = {
                        let cover = json_str(&s["coverArt"]);
                        if cover.is_empty() { id.clone() } else { cover }
                    };
                    let style_key = classify_style(&genre)
                        .map(|style| style.key.to_string())
                        .unwrap_or_default();
                    if !id.is_empty() {
                        all_tracks.push((
                            id,
                            title,
                            artist,
                            artist_id,
                            album_name,
                            genre,
                            style_key,
                            duration,
                            cover_id,
                        ));
                    }
                }
            }

            if albums.len() < 500 {
                break;
            }
            offset += 500;
        }
    }

    if all_tracks.is_empty() {
        return Err(
            "Navidrome returned no tracks. Check the URL, username, password, and selected libraries."
                .into(),
        );
    }

    let mut unique_artist_ids: HashMap<String, String> = HashMap::new();
    for t in &all_tracks {
        if !t.3.is_empty() {
            unique_artist_ids.entry(t.2.clone()).or_insert_with(|| t.3.clone());
        }
    }

    let mut similarity_pairs: Vec<(String, String)> = Vec::new();
    for (artist_name, artist_id) in unique_artist_ids.iter().take(60) {
        let mut params = nd_params(&auth);
        params.push(("id".into(), artist_id.clone()));
        params.push(("count".into(), "15".into()));
        if let Ok(json) = subsonic_get(state, &cfg.navidrome_url, "getArtistInfo2.view", params).await {
                if let Some(sim_arr) =
                    json["subsonic-response"]["artistInfo2"]["similarArtist"].as_array()
                {
                    for sim in sim_arr {
                        if let Some(sname) = sim["name"].as_str() {
                            similarity_pairs.push((
                                artist_name.to_lowercase(),
                                sname.to_lowercase(),
                            ));
                        }
                    }
                }
            }
        }

    let mut by_artist: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, track) in all_tracks.iter().enumerate() {
        by_artist.entry(track.2.to_lowercase()).or_default().push(index);
    }
    for indexes in by_artist.values() {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for index in indexes {
            let style = &all_tracks[*index].6;
            if !style.is_empty() {
                *counts.entry(style.clone()).or_default() += 1;
            }
        }
        let tagged: usize = counts.values().sum();
        if tagged < 3 {
            continue;
        }
        let Some((best, count)) = counts.iter().max_by_key(|(_, count)| *count) else {
            continue;
        };
        if *count * 5 < tagged * 3 {
            continue;
        }
        let best = best.clone();
        for index in indexes {
            all_tracks[*index].6 = best.clone();
        }
    }

    let conn = state.db.lock().unwrap();
    conn.execute("DELETE FROM tracks", []).unwrap();
    conn.execute("DELETE FROM similar_artists", []).unwrap();
    // Preserve is_favorite state across rescans by mapping existing favorites
    let mut existing_favorites: HashSet<String> = HashSet::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id FROM stations WHERE is_favorite = 1") {
        if let Ok(rows) = stmt.query_map([], |row| row.get(0)) {
            for r in rows.filter_map(Result::ok) {
                existing_favorites.insert(r);
            }
        }
    }
    conn.execute("DELETE FROM stations", []).unwrap();

    for t in &all_tracks {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO tracks (id, title, artist, artist_id, album, genre, cluster, duration, cover_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![t.0, t.1, t.2, t.3, t.4, t.5, t.6, t.7, t.8],
        );
    }

    for (a, b) in &similarity_pairs {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO similar_artists (artist_name, similar_name) VALUES (?1, ?2)",
            params![a, b],
        );
    }

    let floor = cfg.min_station_tracks.max(100);
    let mut style_counts: HashMap<String, usize> = HashMap::new();
    for track in &all_tracks {
        if !track.6.is_empty() {
            *style_counts.entry(track.6.clone()).or_default() += 1;
        }
    }

    let mut buckets: HashMap<String, StationBucket> = HashMap::new();
    for track in &all_tracks {
        let Some(style) = style_by_key(&track.6) else {
            continue;
        };
        let own_style = style_counts.get(style.key).copied().unwrap_or(0) >= floor;
        let key = if own_style {
            format!("style:{}", style.key)
        } else {
            format!("family:{}", style.family)
        };
        let bucket = buckets.entry(key).or_insert_with(|| StationBucket {
            label: if own_style {
                style.label.to_string()
            } else {
                style.family_label.to_string()
            },
            family: style.family.to_string(),
            track_ids: Vec::new(),
            cover: track.8.clone(),
            style_labels: HashSet::new(),
        });
        bucket.track_ids.push(track.0.clone());
        bucket.style_labels.insert(style.label.to_string());
    }

    let mut frozen: HashSet<String> = HashSet::new();
    loop {
        let small_key = buckets
            .iter()
            .filter(|(key, bucket)| bucket.track_ids.len() < floor && !frozen.contains(*key))
            .min_by_key(|(_, bucket)| bucket.track_ids.len())
            .map(|(key, _)| key.clone());
        let Some(small_key) = small_key else {
            break;
        };
        let family = buckets.get(&small_key).map(|bucket| bucket.family.clone());
        let Some(family) = family else {
            break;
        };
        let mut dest_key = None;
        let mut cursor = family_neighbor(&family);
        let mut hops = 0;
        while let Some(next_family) = cursor {
            if hops > 5 {
                break;
            }
            hops += 1;
            let mut best: Option<(usize, String)> = None;
            for (key, bucket) in &buckets {
                if key == &small_key || bucket.family != next_family {
                    continue;
                }
                let len = bucket.track_ids.len();
                if best.as_ref().map(|(count, _)| len > *count).unwrap_or(true) {
                    best = Some((len, key.clone()));
                }
            }
            if let Some((_, key)) = best {
                dest_key = Some(key);
                break;
            }
            cursor = family_neighbor(next_family);
        }
        if let Some(dest_key) = dest_key {
            let Some(small) = buckets.remove(&small_key) else {
                break;
            };
            if let Some(dest) = buckets.get_mut(&dest_key) {
                dest.track_ids.extend(small.track_ids);
                dest.style_labels.extend(small.style_labels);
                if dest.style_labels.len() > 1 {
                    if let Some(style) = STYLES.iter().find(|style| style.family == dest.family) {
                        dest.label = style.family_label.to_string();
                    }
                }
            }
        } else {
            frozen.insert(small_key);
        }
    }

    let mut stations_built = 0;
    let mut placed = 0usize;
    for (key, bucket) in &buckets {
        if bucket.track_ids.len() < floor {
            log_ha(&format!(
                "station skipped {} ({} songs, under {})",
                bucket.label,
                bucket.track_ids.len(),
                floor
            ));
            continue;
        }
        let core_set: HashSet<&String> = bucket.track_ids.iter().collect();
        let mut station_artists: HashSet<String> = HashSet::new();
        for track in &all_tracks {
            if core_set.contains(&track.0) {
                station_artists.insert(track.2.to_lowercase());
            }
        }
        let mut similar_artists: HashSet<String> = HashSet::new();
        for (left, right) in &similarity_pairs {
            if station_artists.contains(left) {
                similar_artists.insert(right.clone());
            }
            if station_artists.contains(right) {
                similar_artists.insert(left.clone());
            }
        }
        let mut fringe_ids = Vec::new();
        for track in &all_tracks {
            if core_set.contains(&track.0) || !similar_artists.contains(&track.2.to_lowercase()) {
                continue;
            }
            let same_family = style_by_key(&track.6)
                .map(|style| style.family == bucket.family)
                .unwrap_or(false);
            if same_family {
                fringe_ids.push(track.0.clone());
            }
        }
        let fringe_cap = bucket.track_ids.len() / 5;
        if fringe_ids.len() > fringe_cap {
            fringe_ids.shuffle(&mut rand::thread_rng());
            fringe_ids.truncate(fringe_cap);
        }
        let mut included: Vec<String> = bucket.style_labels.iter().cloned().collect();
        included.sort();
        let description = format!("Includes {}", included.join(", "));
        let st_id = format!("style-{}", key.replace(':', "-"));
        let is_fav = if existing_favorites.contains(&st_id) { 1 } else { 0 };
        let _ = conn.execute(
            "INSERT INTO stations (id, name, category, description, core_tracks_json, fringe_tracks_json, cover_id, is_favorite)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                st_id,
                bucket.label,
                "Dynamic Genre Stations",
                description,
                serde_json::to_string(&bucket.track_ids).unwrap(),
                serde_json::to_string(&fringe_ids).unwrap(),
                bucket.cover,
                is_fav
            ],
        );
        placed += bucket.track_ids.len();
        stations_built += 1;
        log_ha(&format!(
            "genre station {} core={} fringe={} {}",
            bucket.label,
            bucket.track_ids.len(),
            fringe_ids.len(),
            description
        ));
    }
    log_ha(&format!(
        "genre stations={} placed={} indexed={}",
        stations_built,
        placed,
        all_tracks.len()
    ));

    let mut artist_tracks: HashMap<String, Vec<(String, String, String)>> = HashMap::new();
    for track in &all_tracks {
        artist_tracks.entry(track.2.clone()).or_default().push((
            track.0.clone(),
            track.6.clone(),
            track.8.clone(),
        ));
    }

    for (artist_name, tracks) in &artist_tracks {
        if tracks.len() < 20 {
            continue;
        }
        let cover = &tracks[0].2;
        let core_ids: Vec<String> = tracks.iter().map(|track| track.0.clone()).collect();
        let core_set: HashSet<&String> = core_ids.iter().collect();
        let mut style_counts: HashMap<String, usize> = HashMap::new();
        for track in tracks {
            if !track.1.is_empty() {
                *style_counts.entry(track.1.clone()).or_default() += 1;
            }
        }
        let artist_family = style_counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .and_then(|(style, _)| style_by_key(style))
            .map(|style| style.family.to_string());
        let mut similar_artists: HashSet<String> = HashSet::new();
        let artist_low = artist_name.to_lowercase();
        for (left, right) in &similarity_pairs {
            if left == &artist_low {
                similar_artists.insert(right.clone());
            }
            if right == &artist_low {
                similar_artists.insert(left.clone());
            }
        }
        let mut fringe_ids = Vec::new();
        if let Some(family) = artist_family {
            for track in &all_tracks {
                if core_set.contains(&track.0) || !similar_artists.contains(&track.2.to_lowercase()) {
                    continue;
                }
                let same_family = style_by_key(&track.6)
                    .map(|style| style.family == family)
                    .unwrap_or(false);
                if same_family {
                    fringe_ids.push(track.0.clone());
                }
            }
        }
        let fringe_cap = core_ids.len() / 5;
        if fringe_ids.len() > fringe_cap {
            fringe_ids.shuffle(&mut rand::thread_rng());
            fringe_ids.truncate(fringe_cap);
        }
        let st_id = format!(
            "artist-{}",
            artist_name.to_lowercase().replace([' ', '.', '&', '/'], "-")
        );
        let is_fav = if existing_favorites.contains(&st_id) { 1 } else { 0 };
        let _ = conn.execute(
            "INSERT INTO stations (id, name, category, description, core_tracks_json, fringe_tracks_json, cover_id, is_favorite)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                st_id,
                format!("{} Radio", artist_name),
                "Artist Stations",
                format!("Songs by {artist_name}, with a few similar artists from the same style"),
                serde_json::to_string(&core_ids).unwrap(),
                serde_json::to_string(&fringe_ids).unwrap(),
                cover,
                is_fav
            ],
        );
        stations_built += 1;
    }

    Ok((all_tracks.len(), stations_built))
}

async fn admin_rescan(
    State(state): State<AppState>,
    Json(req): Json<ScanRequest>,
) -> impl IntoResponse {
    match perform_scan_and_build(&state, &req).await {
        Ok((tracks, stations)) => Json(serde_json::json!({
            "status": "ok",
            "tracks_indexed": tracks,
            "stations_created": stations
        }))
        .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "status": "error", "message": e })),
        )
            .into_response(),
    }
}

fn build_router(state: AppState) -> Router {
    let mut r = Router::new()
        .route("/", get(index_html))
        .route("/api/context", get(api_context))
        .route("/api/login", post(api_login))
        .route("/api/logout", post(api_logout))
        .route("/api/stations", get(api_stations))
        .route("/api/stations/:id/favorite", post(api_toggle_favorite))
        .route("/api/stations/:id/tracks", get(api_station_tracks))
        .route("/api/stations/:id/queue", get(api_station_queue))
        .route("/api/stream/:id", get(proxy_stream))
        .route("/api/cover/:id", get(proxy_cover));

    if state.is_admin_port {
        r = r
            .route("/api/admin/config", get(admin_get_config).post(admin_save_config))
            .route("/api/admin/rescan", post(admin_rescan))
            .route("/api/admin/users", post(admin_add_user))
            .route("/api/admin/users/:username", delete(admin_del_user));
    }

    r.with_state(state)
}

#[tokio::main]
async fn main() {
    let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| ".".into());
    let static_dir = env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into());
    let ingress_port: u16 = env::var("INGRESS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8099);
    let public_port: u16 = env::var("PUBLIC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8093);

    let db_path = format!("{}/naviradio.db", data_dir);
    let db = Arc::new(Mutex::new(init_db(&db_path)));
    let logins = Arc::new(Mutex::new(HashMap::new()));
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap();

    let admin_state = AppState {
        db: db.clone(),
        http: http.clone(),
        is_admin_port: true,
        static_dir: static_dir.clone(),
        logins: logins.clone(),
    };

    let public_state = AppState {
        db: db.clone(),
        http: http.clone(),
        is_admin_port: false,
        static_dir,
        logins,
    };

    let startup_state = admin_state.clone();
    tokio::spawn(async move {
        let _ = perform_scan_and_build(&startup_state, &ScanRequest::default()).await;
    });

    let admin_app = build_router(admin_state);
    let public_app = build_router(public_state);

    let admin_addr = SocketAddr::from(([0, 0, 0, 0], ingress_port));
    let public_addr = SocketAddr::from(([0, 0, 0, 0], public_port));

    let admin_listener = tokio::net::TcpListener::bind(admin_addr).await.unwrap();
    let public_listener = tokio::net::TcpListener::bind(public_addr).await.unwrap();
    log_ha(&format!(
        "version 0.2.20 listening admin={ingress_port} public={public_port}"
    ));

    tokio::select! {
        _ = axum::serve(
            admin_listener,
            admin_app.into_make_service_with_connect_info::<SocketAddr>(),
        ) => {},
        _ = axum::serve(
            public_listener,
            public_app.into_make_service_with_connect_info::<SocketAddr>(),
        ) => {},
    }
}
