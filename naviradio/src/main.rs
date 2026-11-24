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
    scan: Arc<Mutex<ScanStatus>>,
}

struct ScanStatus {
    running: bool,
    message: String,
    tracks: usize,
    stations: usize,
    error: String,
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

fn clean_tag(raw: &str) -> Option<String> {
    let text = normalize_genre_text(raw);
    if text.is_empty()
        || text.chars().all(|ch| ch.is_ascii_digit())
        || matches!(
            text.as_str(),
            "unknown"
                | "other"
                | "various"
                | "misc"
                | "general"
                | "soundtrack"
                | "score"
                | "billboard"
                | "album"
                | "seen live"
                | "favorites"
                | "favourite"
                | "favorite"
                | "all"
                | "under 2000 listeners"
                | "spotify"
        )
    {
        return None;
    }
    Some(text)
}

fn title_tag(tag: &str) -> String {
    let mut words = Vec::new();
    for word in tag.split_whitespace() {
        let mut chars = word.chars();
        let Some(first) = chars.next() else { continue };
        let mut titled = first.to_uppercase().collect::<String>();
        titled.push_str(chars.as_str());
        words.push(titled);
    }
    if words.is_empty() {
        "Mix".to_string()
    } else {
        words.join(" ")
    }
}

fn usable_label_tag(raw: &str) -> Option<String> {
    let text = clean_tag(raw)?;
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || words.len() > 3 {
        return None;
    }
    // "Alternative en Indie" is two styles glued together. It is not a station name.
    if words.iter().any(|word| {
        matches!(
            *word,
            "en" | "and" | "und" | "et" | "och" | "of" | "feat" | "featuring"
        )
    }) {
        return None;
    }
    Some(title_tag(&text))
}

fn sonic_matches(json: &serde_json::Value) -> Vec<(String, f64)> {
    let Some(items) = json["subsonic-response"]["sonicMatch"].as_array() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for item in items {
        let id = json_str(&item["entry"]["id"]);
        if id.is_empty() {
            continue;
        }
        let mut score = item["similarity"].as_f64().unwrap_or(0.0);
        if score > 1.0 {
            score /= 100.0;
        }
        if score > 0.0 {
            found.push((id, score));
        }
    }
    found
}

fn keep_strong(found: Vec<(String, f64)>, self_id: &str, library: &HashSet<String>) -> Vec<(String, f64)> {
    let mut found: Vec<(String, f64)> = found
        .into_iter()
        .filter(|(other, score)| other != self_id && library.contains(other) && *score >= 0.55)
        .collect();
    if found.is_empty() {
        return found;
    }
    let best = found.iter().map(|(_, score)| *score).fold(0.0_f64, f64::max);
    found.retain(|(_, score)| *score >= best * 0.85);
    found.truncate(5);
    found
}

fn sound_station_names(
    ids: &[String],
    tag_of: &HashMap<String, String>,
    artist_of: &HashMap<String, String>,
    library_tag_counts: &HashMap<String, usize>,
    library_size: usize,
) -> Vec<String> {
    let mut songs: HashMap<String, usize> = HashMap::new();
    let mut artists: HashMap<String, HashSet<String>> = HashMap::new();
    for id in ids {
        let Some(tag) = tag_of.get(id) else {
            continue;
        };
        *songs.entry(tag.clone()).or_default() += 1;
        if let Some(artist) = artist_of.get(id) {
            artists.entry(tag.clone()).or_default().insert(artist.clone());
        }
    }
    let library_top = library_tag_counts
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then(right.0.cmp(left.0)))
        .map(|(tag, _)| tag.clone());
    let cluster_len = ids.len().max(1) as f64;
    let library_len = library_size.max(1) as f64;
    let mut ranked: Vec<(i64, usize, String)> = songs
        .into_iter()
        .filter_map(|(tag, count)| {
            let n_artists = artists.get(&tag).map(|set| set.len()).unwrap_or(0);
            if n_artists < 2 || count < 3 {
                return None;
            }
            let lib = library_tag_counts.get(&tag).copied().unwrap_or(1).max(1) as f64;
            let lift = (count as f64 / cluster_len) / (lib / library_len);
            Some(((lift * 1000.0) as i64, count, tag))
        })
        .collect();
    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(right.1.cmp(&left.1))
            .then(left.2.len().cmp(&right.2.len()))
            .then(left.2.cmp(&right.2))
    });
    let mut names = Vec::new();
    if let Some(top) = library_top {
        for (_, _, tag) in &ranked {
            if tag != &top && !names.contains(tag) {
                names.push(tag.clone());
            }
        }
        for (_, _, tag) in &ranked {
            if tag == &top && !names.contains(tag) {
                names.push(tag.clone());
            }
        }
    } else {
        for (_, _, tag) in ranked {
            if !names.contains(&tag) {
                names.push(tag);
            }
        }
    }
    names
}

fn decade_label(year: u16) -> Option<&'static str> {
    Some(match year {
        1950..=1959 => "50s",
        1960..=1969 => "60s",
        1970..=1979 => "70s",
        1980..=1989 => "80s",
        1990..=1999 => "90s",
        2000..=2009 => "2000s",
        2010..=2019 => "2010s",
        2020..=2029 => "2020s",
        _ => return None,
    })
}

fn decade_index(label: &str) -> u16 {
    match label {
        "50s" => 1950,
        "60s" => 1960,
        "70s" => 1970,
        "80s" => 1980,
        "90s" => 1990,
        "2000s" => 2000,
        "2010s" => 2010,
        "2020s" => 2020,
        _ => 0,
    }
}

fn family_slug(name: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

async fn audiomuse_neighbors(
    state: &AppState,
    base: &str,
    auth: &NdAuth,
    ids: &[String],
    library: &HashSet<String>,
) -> HashMap<String, Vec<(String, f64)>> {
    let mut neighbors: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    let total = ids.len();
    let mut done = 0usize;
    let mut used_scores = false;
    for chunk in ids.chunks(6) {
        let mut jobs = Vec::new();
        for id in chunk {
            let state = state.clone();
            let base = base.to_string();
            let auth = auth.clone();
            let id = id.clone();
            let library = library.clone();
            jobs.push(tokio::spawn(async move {
                let mut params = nd_params(&auth);
                params.push(("id".into(), id.clone()));
                params.push(("count".into(), "8".into()));
                let scored = match subsonic_get(&state, &base, "getSonicSimilarTracks.view", params.clone()).await {
                    Ok(json) => sonic_matches(&json),
                    Err(_) => Vec::new(),
                };
                if !scored.is_empty() {
                    let kept = keep_strong(scored, &id, &library);
                    return (id, kept, true);
                }
                let ranked = match subsonic_get(&state, &base, "getSimilarSongs2.view", params).await {
                    Ok(json) => json_items(&json["subsonic-response"]["similarSongs2"]["song"])
                        .iter()
                        .enumerate()
                        .map(|(rank, song)| {
                            let song_id = json_str(&song["id"]);
                            let score = (1.0 - rank as f64 * 0.06).max(0.4);
                            (song_id, score)
                        })
                        .filter(|(song_id, _)| !song_id.is_empty())
                        .collect::<Vec<_>>(),
                    Err(err) => {
                        log_ha(&format!("audiomuse similar failed for {id}: {err}"));
                        Vec::new()
                    }
                };
                let kept = keep_strong(ranked, &id, &library);
                (id, kept, false)
            }));
        }
        for job in jobs {
            let Ok((id, found, scored)) = job.await else {
                continue;
            };
            done += 1;
            if scored {
                used_scores = true;
            }
            if !found.is_empty() {
                neighbors.insert(id, found);
            }
        }
        if done == total || done % 60 == 0 {
            log_ha(&format!("audiomuse similar {done}/{total}"));
        }
    }
    if used_scores {
        log_ha("audiomuse similarity scores came from getSonicSimilarTracks");
    } else {
        log_ha("getSonicSimilarTracks was not available, used the closest ranked similar songs");
    }
    neighbors
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
    // 100 was the old clamp. The box could not be set lower, and the scan did not use it.
    let migrated: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'min_tracks_floor_fixed'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "0".into());
    if migrated != "1" {
        let current: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'min_station_tracks'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "100".into());
        if current == "100" || current.is_empty() {
            let _ = conn.execute(
                "INSERT INTO settings (key, value) VALUES ('min_station_tracks', '10')
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [],
            );
        }
        let _ = conn.execute(
            "INSERT INTO settings (key, value) VALUES ('min_tracks_floor_fixed', '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        );
    }
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
    let def_min = env::var("DEFAULT_MIN_TRACKS").unwrap_or_else(|_| "10".into());

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
            .unwrap_or(10)
            .clamp(1, 1000),
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

#[derive(Clone)]
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
    let mut track_years: HashMap<String, u16> = HashMap::new();
    let folders_to_scan = if cfg.selected_folders.is_empty() {
        vec![None]
    } else {
        cfg.selected_folders.iter().cloned().map(Some).collect()
    };

    let mut newest_params = nd_params(&auth);
    newest_params.push(("type".into(), "newest".into()));
    newest_params.push(("size".into(), "40".into()));
    let library_stamp = match subsonic_get(
        state,
        &cfg.navidrome_url,
        "getAlbumList2.view",
        newest_params,
    )
    .await
    {
        Ok(json) => json_items(&json["subsonic-response"]["albumList2"]["album"])
            .iter()
            .map(|album| format!("{}:{}", json_str(&album["id"]), json_str(&album["songCount"])))
            .collect::<Vec<_>>()
            .join(","),
        Err(err) => {
            log_ha(&format!("could not read newest albums: {err}"));
            String::new()
        }
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
                let album_body = &album_json["subsonic-response"]["album"];
                let album_year = album_body["year"].as_u64().unwrap_or(0) as u16;
                let songs = json_items(&album_body["song"]);
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
                    let style_key = clean_tag(&genre).unwrap_or_default();
                    if !id.is_empty() {
                        let song_year = s["year"].as_u64().unwrap_or(0) as u16;
                        let year = if (1950..2030).contains(&song_year) {
                            song_year
                        } else {
                            album_year
                        };
                        if (1950..2030).contains(&year) {
                            track_years.insert(id.clone(), year);
                        }
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

    let mut track_ids: Vec<String> = all_tracks.iter().map(|track| track.0.clone()).collect();
    track_ids.sort();
    track_ids.dedup();
    let library: HashSet<String> = track_ids.iter().cloned().collect();
    let mut artist_of: HashMap<String, String> = HashMap::new();
    let mut tag_of: HashMap<String, String> = HashMap::new();
    let mut library_tag_counts: HashMap<String, usize> = HashMap::new();
    for track in &all_tracks {
        artist_of.entry(track.0.clone()).or_insert_with(|| track.2.to_lowercase());
        if let Some(tag) = usable_label_tag(&track.5) {
            *library_tag_counts.entry(tag.clone()).or_default() += 1;
            tag_of.entry(track.0.clone()).or_insert(tag);
        }
    }

    let neighbors =
        audiomuse_neighbors(state, &cfg.navidrome_url, &auth, &track_ids, &library).await;
    log_ha(&format!(
        "audiomuse similar songs: {} of {} had a match",
        neighbors.len(),
        track_ids.len()
    ));
    let mut similarity_pairs: Vec<(String, String)> = Vec::new();
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();
    for (id, links) in &neighbors {
        let Some(left) = artist_of.get(id) else {
            continue;
        };
        for (other, _) in links {
            let Some(right) = artist_of.get(other) else {
                continue;
            };
            if left == right {
                continue;
            }
            let key = if left < right {
                (left.clone(), right.clone())
            } else {
                (right.clone(), left.clone())
            };
            if seen_pairs.insert(key.clone()) {
                similarity_pairs.push(key);
            }
        }
    }
    log_ha(&format!(
        "audiomuse artist links: {}",
        similarity_pairs.len()
    ));

    let mut score_of: HashMap<String, HashMap<String, f64>> = HashMap::new();
    for (id, links) in &neighbors {
        for (other, score) in links {
            score_of.entry(id.clone()).or_default().insert(other.clone(), *score);
        }
    }
    let pair_score = |left: &str, right: &str| -> f64 {
        let forward = score_of
            .get(left)
            .and_then(|links| links.get(right))
            .copied()
            .unwrap_or(0.0);
        let back = score_of
            .get(right)
            .and_then(|links| links.get(left))
            .copied()
            .unwrap_or(0.0);
        forward.max(back)
    };
    // A song joins a group only when it is close to the songs already in it.
    // One shared neighbor is not enough, so a chain cannot pull unlike artists together.
    let cohesion = |group: &[String], candidate: &str| -> Option<f64> {
        let mut direct = 0usize;
        let mut sum = 0.0;
        for member in group {
            let score = pair_score(member, candidate);
            if score >= 0.55 {
                direct += 1;
                sum += score;
            }
        }
        if direct == 0 {
            return None;
        }
        if group.len() == 1 {
            return Some(sum);
        }
        let need = (group.len() / 2).max(2);
        if direct < need {
            return None;
        }
        let average = sum / group.len() as f64;
        if average < 0.35 {
            return None;
        }
        Some(average)
    };

    let floor = cfg.min_station_tracks.max(1);
    let count_artists = |ids: &[String]| -> usize {
        let mut seen = HashSet::new();
        for id in ids {
            if let Some(artist) = artist_of.get(id) {
                seen.insert(artist.as_str());
            }
        }
        seen.len()
    };
    let sound_cap = 80usize;
    let mut placed: HashSet<String> = HashSet::new();
    let mut skipped_seeds: HashSet<String> = HashSet::new();
    let mut groups: Vec<Vec<String>> = Vec::new();
    loop {
        let mut seed: Option<&String> = None;
        for id in &track_ids {
            if placed.contains(id) || skipped_seeds.contains(id) || !neighbors.contains_key(id) {
                continue;
            }
            let links = neighbors.get(id).map(|list| list.len()).unwrap_or(0);
            if links == 0 {
                continue;
            }
            match seed {
                None => seed = Some(id),
                Some(current) => {
                    let current_links = neighbors.get(current).map(|list| list.len()).unwrap_or(0);
                    if links > current_links || (links == current_links && id < current) {
                        seed = Some(id);
                    }
                }
            }
        }
        let Some(seed) = seed.cloned() else {
            break;
        };
        let mut group = vec![seed.clone()];
        while group.len() < sound_cap {
            let mut best: Option<(i64, String)> = None;
            for candidate in &track_ids {
                if placed.contains(candidate) || group.iter().any(|id| id == candidate) {
                    continue;
                }
                let Some(average) = cohesion(&group, candidate) else {
                    continue;
                };
                let rank = (average * 1000.0) as i64;
                if best.as_ref().map(|(score, _)| rank > *score).unwrap_or(true) {
                    best = Some((rank, candidate.clone()));
                }
            }
            let Some((_, next)) = best else {
                break;
            };
            group.push(next);
        }
        if group.len() >= floor && count_artists(&group) >= 2 {
            for id in &group {
                placed.insert(id.clone());
            }
            groups.push(group);
        } else {
            skipped_seeds.insert(seed);
        }
    }

    let mut membership: HashMap<String, usize> = HashMap::new();
    for group in &groups {
        for id in group {
            *membership.entry(id.clone()).or_default() += 1;
        }
    }
    for group in &mut groups {
        let snapshot = group.clone();
        let mut extras: Vec<(i64, String)> = Vec::new();
        for candidate in &track_ids {
            if snapshot.iter().any(|id| id == candidate) {
                continue;
            }
            if membership.get(candidate).copied().unwrap_or(0) >= 2 {
                continue;
            }
            let Some(average) = cohesion(&snapshot, candidate) else {
                continue;
            };
            extras.push(((average * 1000.0) as i64, candidate.clone()));
        }
        extras.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
        for (_, candidate) in extras {
            if group.len() >= sound_cap {
                break;
            }
            if membership.get(&candidate).copied().unwrap_or(0) >= 2 {
                continue;
            }
            if group.iter().any(|id| id == &candidate) {
                continue;
            }
            group.push(candidate.clone());
            *membership.entry(candidate).or_default() += 1;
        }
    }

    let mut pending = groups;
    pending.sort_by(|left, right| right.len().cmp(&left.len()).then(left[0].cmp(&right[0])));

    let mut family_of: HashMap<String, Vec<String>> = HashMap::new();
    let mut sound_stations: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut used_station_ids: HashSet<String> = HashSet::new();
    let mut placed_ids: HashSet<String> = HashSet::new();
    let mut used_names: HashSet<String> = HashSet::new();
    for mut ids in pending {
        ids.sort();
        ids.dedup();
        let candidates = sound_station_names(
            &ids,
            &tag_of,
            &artist_of,
            &library_tag_counts,
            library.len(),
        );
        let Some(name) = candidates.into_iter().find(|name| !used_names.contains(name)) else {
            log_ha(&format!(
                "sound group left off the list ({} songs). Its only name is already a station.",
                ids.len()
            ));
            continue;
        };
        used_names.insert(name.clone());
        let slug = {
            let raw = family_slug(&name);
            if raw.is_empty() { "mix".to_string() } else { raw }
        };
        let mut station_id = format!("genre-{slug}");
        let mut n = 2usize;
        while used_station_ids.contains(&station_id) {
            station_id = format!("genre-{slug}-{n}");
            n += 1;
        }
        used_station_ids.insert(station_id.clone());
        for id in &ids {
            family_of.entry(id.clone()).or_default().push(name.clone());
            placed_ids.insert(id.clone());
        }
        sound_stations.push((station_id, name, ids));
    }
    log_ha(&format!(
        "sound stations={} songs_placed={}",
        sound_stations.len(),
        placed_ids.len()
    ));

    let mut genre_of_track: HashMap<String, String> = HashMap::new();
    for (station_id, _, ids) in &sound_stations {
        for id in ids {
            genre_of_track.insert(id.clone(), station_id.clone());
        }
    }
    for track in &mut all_tracks {
        track.6 = genre_of_track.get(&track.0).cloned().unwrap_or_default();
    }

    let conn = state.db.lock().unwrap();
    conn.execute("DELETE FROM tracks", []).unwrap();
    conn.execute("DELETE FROM similar_artists", []).unwrap();
    let mut existing_favorites: HashSet<String> = HashSet::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id FROM stations WHERE is_favorite = 1") {
        if let Ok(rows) = stmt.query_map([], |row| row.get(0)) {
            for r in rows.filter_map(Result::ok) {
                existing_favorites.insert(r);
            }
        }
    }
    conn.execute("DELETE FROM stations", []).unwrap();

    for (a, b) in &similarity_pairs {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO similar_artists (artist_name, similar_name) VALUES (?1, ?2)",
            params![a, b],
        );
    }

    let mut stations_built = 0;
    let mut placed = 0usize;
    for track in &all_tracks {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO tracks (id, title, artist, artist_id, album, genre, cluster, duration, cover_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![track.0, track.1, track.2, track.3, track.4, track.5, track.6, track.7, track.8],
        );
    }

    for (station_id, name, ids) in &sound_stations {
        let cover = all_tracks
            .iter()
            .find(|track| ids.iter().any(|id| id == &track.0) && !track.8.is_empty())
            .map(|track| track.8.clone())
            .unwrap_or_default();
        let is_fav = if existing_favorites.contains(station_id) { 1 } else { 0 };
        let _ = conn.execute(
            "INSERT INTO stations (id, name, category, description, core_tracks_json, fringe_tracks_json, cover_id, is_favorite)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                station_id,
                name,
                "Genre Stations",
                format!("{} songs", ids.len()),
                serde_json::to_string(&ids).unwrap(),
                "[]",
                cover,
                is_fav
            ],
        );
        placed += ids.len();
        stations_built += 1;
        log_ha(&format!("genre station {name} songs={}", ids.len()));
    }

    let mut other: Vec<String> = all_tracks
        .iter()
        .map(|track| track.0.clone())
        .filter(|id| !placed_ids.contains(id))
        .collect();
    other.sort();
    other.dedup();
    if !other.is_empty() {
        let station_id = "genre-other";
        let cover = all_tracks
            .iter()
            .find(|track| other.iter().any(|id| id == &track.0) && !track.8.is_empty())
            .map(|track| track.8.clone())
            .unwrap_or_default();
        let is_fav = if existing_favorites.contains(station_id) { 1 } else { 0 };
        let _ = conn.execute(
            "INSERT INTO stations (id, name, category, description, core_tracks_json, fringe_tracks_json, cover_id, is_favorite)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                station_id,
                "Other",
                "Genre Stations",
                format!("{} songs", other.len()),
                serde_json::to_string(&other).unwrap(),
                "[]",
                cover,
                is_fav
            ],
        );
        placed += other.len();
        stations_built += 1;
        log_ha(&format!("genre station Other songs={}", other.len()));
    }

    let mut era_buckets: HashMap<(String, String), (String, Vec<String>, String)> = HashMap::new();
    for track in &all_tracks {
        let Some(year) = track_years.get(&track.0).copied() else {
            continue;
        };
        let Some(decade) = decade_label(year) else {
            continue;
        };
        let Some(families) = family_of.get(&track.0) else {
            continue;
        };
        for family in families {
            let entry = era_buckets
                .entry((decade.to_string(), family.clone()))
                .or_insert_with(|| (decade.to_string(), Vec::new(), track.8.clone()));
            if !entry.1.iter().any(|id| id == &track.0) {
                entry.1.push(track.0.clone());
            }
        }
    }
    let mut by_family: HashMap<String, Vec<(u16, String, Vec<String>, String)>> = HashMap::new();
    for ((decade, family), (_, ids, cover)) in era_buckets {
        by_family.entry(family).or_default().push((
            decade_index(&decade),
            decade,
            ids,
            cover,
        ));
    }
    let mut families: Vec<String> = by_family.keys().cloned().collect();
    families.sort();
    for family in families {
        let mut slices = by_family.remove(&family).unwrap_or_default();
        slices.sort_by_key(|slice| slice.0);
        for (_year, decade, ids, cover) in slices {
            // One decade, one station. Do not glue 80s through 2020s into a range.
            if ids.len() < cfg.min_station_tracks {
                log_ha(&format!(
                    "era skipped {decade} {family} ({} songs)",
                    ids.len()
                ));
                continue;
            }
            let label = format!("{decade} {family}");
            let station_id = format!("era-{decade}-{}", family_slug(&family));
            let is_fav = if existing_favorites.contains(&station_id) { 1 } else { 0 };
            let _ = conn.execute(
                "INSERT INTO stations (id, name, category, description, core_tracks_json, fringe_tracks_json, cover_id, is_favorite)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    station_id,
                    label,
                    "Era Stations",
                    format!("{} songs", ids.len()),
                    serde_json::to_string(&ids).unwrap(),
                    "[]",
                    cover,
                    is_fav
                ],
            );
            stations_built += 1;
            log_ha(&format!("era station {label} songs={}", ids.len()));
        }
    }

    let mut by_artist: HashMap<String, Vec<String>> = HashMap::new();
    let mut artist_display: HashMap<String, String> = HashMap::new();
    let mut artist_cover: HashMap<String, String> = HashMap::new();
    for track in &all_tracks {
        let key = track.2.to_lowercase();
        by_artist.entry(key.clone()).or_default().push(track.0.clone());
        artist_display.entry(key.clone()).or_insert_with(|| track.2.clone());
        artist_cover.entry(key).or_insert_with(|| track.8.clone());
    }
    let artist_floor = cfg.min_station_tracks.max(20);
    let mut seeds: Vec<String> = by_artist
        .iter()
        .filter(|(_, ids)| ids.len() >= 8)
        .map(|(name, _)| name.clone())
        .collect();
    seeds.sort_by(|a, b| {
        by_artist
            .get(b)
            .map(|ids| ids.len())
            .unwrap_or(0)
            .cmp(&by_artist.get(a).map(|ids| ids.len()).unwrap_or(0))
    });
    let mut covered: HashSet<String> = HashSet::new();
    for artist_key in seeds {
        let Some(own) = by_artist.get(&artist_key).cloned() else {
            continue;
        };
        let mut mix = own.clone();
        let mut heard_weight: HashMap<String, f64> = HashMap::new();
        for song_id in &own {
            let Some(links) = neighbors.get(song_id) else {
                continue;
            };
            for (other, weight) in links {
                let Some(other_artist) = artist_of.get(other) else {
                    continue;
                };
                if other_artist == &artist_key {
                    continue;
                }
                *heard_weight.entry(other_artist.clone()).or_default() += *weight;
                mix.push(other.clone());
            }
        }
        mix.sort();
        mix.dedup();
        let artist_name = artist_display
            .get(&artist_key)
            .map(|name| name.as_str())
            .unwrap_or(artist_key.as_str());
        if mix.len() < artist_floor {
            log_ha(&format!(
                "artist station skipped {artist_name} ({} songs)",
                mix.len()
            ));
            continue;
        }
        let already = mix.iter().filter(|id| covered.contains(*id)).count();
        if already * 2 >= mix.len() {
            log_ha(&format!(
                "artist station skipped {artist_name} because that sound is already covered"
            ));
            continue;
        }
        for id in &mix {
            covered.insert(id.clone());
        }
        let mut heard: Vec<(u32, String)> = heard_weight
            .into_iter()
            .map(|(key, weight)| ((weight * 100.0) as u32, key))
            .collect();
        heard.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
        let shown = heard
            .iter()
            .take(4)
            .filter_map(|(_, key)| artist_display.get(key))
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let description = if shown.is_empty() {
            format!("{} songs", mix.len())
        } else {
            format!("{} songs, also {shown}", mix.len())
        };
        let st_id = format!("artist-{}", artist_key.replace([' ', '.', '&', '/'], "-"));
        let is_fav = if existing_favorites.contains(&st_id) { 1 } else { 0 };
        let cover = artist_cover.get(&artist_key).cloned().unwrap_or_default();
        let _ = conn.execute(
            "INSERT INTO stations (id, name, category, description, core_tracks_json, fringe_tracks_json, cover_id, is_favorite)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                st_id,
                format!("{artist_name} Radio"),
                "Artist Stations",
                description,
                serde_json::to_string(&mix).unwrap(),
                "[]",
                cover,
                is_fav
            ],
        );
        stations_built += 1;
        log_ha(&format!(
            "artist station {artist_name} songs={}",
            mix.len()
        ));
    }

    log_ha(&format!(
        "stations={} genre_songs={} indexed={}",
        stations_built,
        placed,
        all_tracks.len()
    ));
    if !library_stamp.is_empty() {
        save_setting(&conn, "library_stamp", &library_stamp);
    }

    Ok((all_tracks.len(), stations_built))
}

async fn newest_library_stamp(state: &AppState) -> Result<String, String> {
    let cfg = {
        let conn = state.db.lock().unwrap();
        load_config(&conn)
    };
    if cfg.navidrome_url.is_empty() || cfg.navidrome_user.is_empty() || cfg.navidrome_pass.is_empty() {
        return Ok(String::new());
    }
    let auth = navidrome_login(
        state,
        &cfg.navidrome_url,
        &cfg.navidrome_user,
        &cfg.navidrome_pass,
        false,
    )
    .await?;
    let mut params = nd_params(&auth);
    params.push(("type".into(), "newest".into()));
    params.push(("size".into(), "40".into()));
    let json = subsonic_get(state, &cfg.navidrome_url, "getAlbumList2.view", params).await?;
    Ok(json_items(&json["subsonic-response"]["albumList2"]["album"])
        .iter()
        .map(|album| format!("{}:{}", json_str(&album["id"]), json_str(&album["songCount"])))
        .collect::<Vec<_>>()
        .join(","))
}

async fn watch_library(state: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(600)).await;
        if state.scan.lock().unwrap().running {
            continue;
        }
        let current = match newest_library_stamp(&state).await {
            Ok(stamp) => stamp,
            Err(err) => {
                log_ha(&format!("library check failed: {err}"));
                continue;
            }
        };
        if current.is_empty() {
            continue;
        }
        let previous = {
            let conn = state.db.lock().unwrap();
            conn.query_row(
                "SELECT value FROM settings WHERE key = 'library_stamp'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_default()
        };
        if previous != current {
            log_ha("Navidrome library changed. Rebuilding stations.");
            run_scan(state.clone(), ScanRequest::default()).await;
        }
    }
}

async fn run_scan(state: AppState, req: ScanRequest) {
    {
        let mut job = state.scan.lock().unwrap();
        if job.running {
            return;
        }
        job.running = true;
        job.error.clear();
        job.message = "Checking the Navidrome login and scanning the library...".to_string();
    }
    let result = perform_scan_and_build(&state, &req).await;
    let mut job = state.scan.lock().unwrap();
    job.running = false;
    match result {
        Ok((tracks, stations)) => {
            job.tracks = tracks;
            job.stations = stations;
            job.message = format!(
                "Scan finished. {tracks} tracks indexed and {stations} stations created."
            );
        }
        Err(err) => {
            job.error = err.clone();
            job.message = format!("Error: {err}");
        }
    }
}

async fn admin_rescan(
    State(state): State<AppState>,
    Json(req): Json<ScanRequest>,
) -> impl IntoResponse {
    if state.scan.lock().unwrap().running {
        return Json(serde_json::json!({ "status": "running" })).into_response();
    }
    tokio::spawn(async move {
        run_scan(state, req).await;
    });
    Json(serde_json::json!({ "status": "started" })).into_response()
}

async fn admin_scan_status(State(state): State<AppState>) -> impl IntoResponse {
    let job = state.scan.lock().unwrap();
    Json(serde_json::json!({
        "running": job.running,
        "message": job.message,
        "tracks_indexed": job.tracks,
        "stations_created": job.stations,
        "error": job.error,
    }))
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
            .route("/api/admin/scan-status", get(admin_scan_status))
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
        .user_agent("NaviRadio/0.2.41 (https://github.com/BrunoNayrb/brunos-haos-apps)")
        .build()
        .unwrap();

    let scan = Arc::new(Mutex::new(ScanStatus {
        running: false,
        message: String::new(),
        tracks: 0,
        stations: 0,
        error: String::new(),
    }));
    let admin_state = AppState {
        db: db.clone(),
        http: http.clone(),
        is_admin_port: true,
        static_dir: static_dir.clone(),
        logins: logins.clone(),
        scan: scan.clone(),
    };

    let public_state = AppState {
        db: db.clone(),
        http: http.clone(),
        is_admin_port: false,
        static_dir,
        logins,
        scan,
    };

    let startup_state = admin_state.clone();
    tokio::spawn(async move {
        run_scan(startup_state, ScanRequest::default()).await;
    });
    let watch_state = admin_state.clone();
    tokio::spawn(async move {
        watch_library(watch_state).await;
    });

    let admin_app = build_router(admin_state);
    let public_app = build_router(public_state);

    let admin_addr = SocketAddr::from(([0, 0, 0, 0], ingress_port));
    let public_addr = SocketAddr::from(([0, 0, 0, 0], public_port));

    let admin_listener = tokio::net::TcpListener::bind(admin_addr).await.unwrap();
    let public_listener = tokio::net::TcpListener::bind(public_addr).await.unwrap();
    log_ha(&format!(
        "version 0.2.41 listening admin={ingress_port} public={public_port}"
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
