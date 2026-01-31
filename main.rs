use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::Frame;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;

// --- Config ---

#[derive(Clone)]
struct Config {
    upload_dir: PathBuf,
    key: Option<String>,
    id_length: usize,
    max_file_size: u64,
    allowed_file_types: Option<Vec<String>>,
    retention_minutes: i64,
}

impl Config {
    fn from_env() -> Self {
        // Simple error handling: panic if something essential breaks on startup or use defaults
        let upload_dir = env::var("UPLOAD_DIR").unwrap_or_else(|_| "uploads".to_string());
        let key = env::var("KEY").ok();
        let id_length = env::var("ID_LENGTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);
        let max_file_size = env::var("MAX_FILE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1048576);
        let allowed_file_types = env::var("ALLOWED_FILE_TYPES").ok().filter(|s| !s.is_empty()).map(|s| {
            s.split(',')
                .map(|t| t.trim().to_lowercase())
                .collect()
        });
        let retention_minutes = env::var("RETENTION_MINUTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);

        Config {
            upload_dir: PathBuf::from(upload_dir),
            key,
            id_length,
            max_file_size,
            allowed_file_types,
            retention_minutes,
        }
    }
}

// --- App State ---

#[derive(Clone)]
struct AppState {
    config: Config,
}

// --- Basic Response Helpers ---

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, BoxError> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

fn empty() -> BoxBody<Bytes, BoxError> {
    Full::new(Bytes::new())
        .map_err(|never| match never {})
        .boxed()
}

// --- Main ---

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let _ = dotenvy::dotenv(); 

    let config = Config::from_env();
    
    // Ensure upload dir exists
    if !config.upload_dir.exists() {
        fs::create_dir_all(&config.upload_dir).await?;
    }

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string()).parse::<u16>().unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("Server listening on port {}", port);
    println!("Storage: {:?}", config.upload_dir);
    println!("Retention Policy: Files older than {} minutes will be deleted automatically.", config.retention_minutes);

    let app_state = AppState { config: config.clone() };

    // Background Cleanup Task
    let cleanup_state = app_state.clone();
    tokio::spawn(async move {
         if cleanup_state.config.retention_minutes <= 0 {
             return; // Disabled
         }
         let interval_ms = env::var("CLEANUP_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60000);
            
         println!("Cleanup Interval: {}ms", interval_ms);
         let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
         
         loop {
             interval.tick().await; 
             if let Err(e) = run_cleanup(&cleanup_state.config).await {
                 eprintln!("Cleanup Error: {}", e);
             }
         }
    });

    let listener = TcpListener::bind(addr).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let app_state = app_state.clone();

        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(move |req| handle_request(req, app_state.clone())))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

// --- Handlers ---

async fn handle_request(req: Request<hyper::body::Incoming>, state: AppState) -> Result<Response<BoxBody<Bytes, BoxError>>, BoxError> {
    
    // Minimal Logging (No chrono)
   let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs();
   println!("[{}] {} {}", now, req.method(), req.uri());

   let response = match (req.method(), req.uri().path()) {
        (&Method::GET, "/health") => Ok(Response::new(full("OK\n"))),
        (&Method::GET, "/robots.txt") => {
            let mut res = Response::new(full("User-agent: *\nDisallow: /"));
            res.headers_mut().insert("content-type", "text/plain".parse().unwrap());
            Ok(res)
        },
        (&Method::GET, "/favicon.ico") => {
             let mut res = Response::new(empty());
             *res.status_mut() = StatusCode::NO_CONTENT;
             Ok(res)
        },
        (&Method::GET, "/") => Ok(Response::new(empty())),
        (&Method::PUT, "/") => handle_upload(req, state, "upload.txt".to_string()).await,
        (&Method::PUT, path) => {
            let filename = path.trim_start_matches('/').to_string();
            handle_upload(req, state, filename).await
        },
        (&Method::GET, path) => {
             let id = path.trim_start_matches('/').to_string();
             handle_download(id, state).await
        },
        _ => {
            let mut res = Response::new(full("Not Found\n"));
            *res.status_mut() = StatusCode::NOT_FOUND;
            Ok(res)
        }
   };
   
   let res = match response {
       Ok(r) => r,
       Err(_e) => {
           let mut r = Response::new(full("Internal Server Error\n"));
           *r.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
           r
       }
   };

    // Security Headers
   let (mut parts, body) = res.into_parts();
   parts.headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
   parts.headers.insert("X-Frame-Options", "DENY".parse().unwrap());
   parts.headers.insert("Strict-Transport-Security", "max-age=31536000; includeSubDomains".parse().unwrap());
   
   Ok(Response::from_parts(parts, body))
}

async fn handle_upload(req: Request<hyper::body::Incoming>, state: AppState, filename_hint: String) -> Result<Response<BoxBody<Bytes, BoxError>>, BoxError> {
    // Check API Key
    if let Some(ref key) = state.config.key {
        let authorized = req.headers().get("x-key")
            .and_then(|v| v.to_str().ok())
            .map(|v| v == key)
            .unwrap_or(false);
            
        if !authorized {
            let res = Response::new(empty());
             return Ok(res);
        }
    }

    // Check extension
    let extension = Path::new(&filename_hint).extension().and_then(|s| s.to_str()).unwrap_or("txt").to_lowercase();
    let original_ext = format!(".{}", extension); // ensure dot

    if let Some(ref allowed) = state.config.allowed_file_types {
         if !allowed.contains(&original_ext) {
             let mut res = Response::new(full(format!("File type not allowed. Allowed: {:?}\n", allowed)));
             *res.status_mut() = StatusCode::BAD_REQUEST;
             return Ok(res);
         }
    }
    
    // Check Content-Length (Early rejection)
    if state.config.max_file_size > 0 {
         if let Some(len) = req.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()) {
             if len > state.config.max_file_size {
                 let mut res = Response::new(full(format!("File too large. Max size: {} bytes\n", state.config.max_file_size)));
                 *res.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
                 return Ok(res);
             }
         }
    }

    // Generate ID
    let id = generate_id(state.config.id_length);
    let new_filename = format!("{}{}", id, original_ext);
    let file_path = state.config.upload_dir.join(&new_filename);

    // Extract host/proto BEFORE consuming body
    let host = req.headers().get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:3000")
        .to_string();
    let proto = req.headers().get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("https")
        .to_string();

    match File::create(&file_path).await {
        Ok(mut file) => {
            let mut body = req.into_body();
            let mut uploaded_size = 0u64;
            let mut failed = false;

            while let Some(frame) = body.frame().await {
                if failed { continue; } // Drain/ignore remainder
                let frame = match frame {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Body frame error: {:?}", e);
                        failed = true;
                        continue;
                    }
                };

                if let Ok(data) = frame.into_data() {
                    uploaded_size += data.len() as u64;
                    if state.config.max_file_size > 0 && uploaded_size > state.config.max_file_size {

                        let mut res = Response::new(full(format!("File too large. Max size: {} bytes\n", state.config.max_file_size)));
                        *res.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
                        // Cleanup
                        let _ = file.shutdown().await;
                        let _ = fs::remove_file(&file_path).await;
                        return Ok(res);
                    }
                    if let Err(e) = file.write_all(&data).await {
                        eprintln!("Write error: {}", e);
                        failed = true;
                    }
                }
            }
            
            if failed {
                 let _ = file.shutdown().await;
                 let _ = fs::remove_file(&file_path).await;
                 let mut res = Response::new(full("Error writing to file\n"));
                 *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                 return Ok(res);
            }

            if let Err(_) = file.flush().await {
                let _ = fs::remove_file(&file_path).await;
                 let mut res = Response::new(full("Error writing to file\n"));
                 *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                 return Ok(res);
            }
        },
        Err(e) => {
            eprintln!("Failed to create file: {}", e);
             let mut res = Response::new(full("Error saving file\n"));
             *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
             return Ok(res);
        }
    }
    
    let file_url = format!("{}://{}/{}\n", proto, host, id); 
    Ok(Response::new(full(file_url)))
}

async fn handle_download(id: String, state: AppState) -> Result<Response<BoxBody<Bytes, BoxError>>, BoxError> {
    // Check for extension in ID
    let target_path = if let Some(_ext) = Path::new(&id).extension() {
         let p = state.config.upload_dir.join(&id);
         if p.exists() { Some(p) } else { None }
    } else {
        // Search for file starting with ID
        let mut match_path = None;
        if let Ok(mut entries) = fs::read_dir(&state.config.upload_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                 if let Ok(name) = entry.file_name().into_string() {
                     if name.starts_with(&format!("{}.", id)) {
                         match_path = Some(state.config.upload_dir.join(name));
                         break;
                     }
                 }
            }
        }
        match_path
    };

    if let Some(path) = target_path {
        // Stream the file
        let file = match File::open(&path).await {
            Ok(file) => file,
            Err(_) => {
                let mut res = Response::new(full("Internal Server Error\n"));
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                return Ok(res);
            }
        };

        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        
        // Convert File to Stream
        let reader_stream = ReaderStream::new(file);
        
        // Convert stream items to Frames
        let stream_body = StreamBody::new(reader_stream.map(|result| {
            match result {
                Ok(bytes) => Ok(Frame::data(bytes)),
                Err(e) => Err(Box::new(e) as BoxError),
            }
        }));
        
        let boxed_body = BodyExt::boxed(stream_body);

        let mut res = Response::new(boxed_body);
        res.headers_mut().insert("Content-Disposition", format!("attachment; filename=\"{}\"", filename).parse().unwrap());
        Ok(res)
    } else {
        let mut res = Response::new(full("File not found\n"));
        *res.status_mut() = StatusCode::NOT_FOUND;
        Ok(res)
    }
}

// Helpers
fn generate_id(len: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

async fn run_cleanup(config: &Config) -> Result<(), BoxError> {
    let mut entries = fs::read_dir(&config.upload_dir).await?;
    let now = SystemTime::now();
    let max_age = Duration::from_secs((config.retention_minutes * 60) as u64);

    while let Ok(Some(entry)) = entries.next_entry().await {
        let metadata = entry.metadata().await?;
        if let Ok(modified) = metadata.modified() {
            if let Ok(age) = now.duration_since(modified) {
                 if age > max_age {
                     let path = entry.path();
                     println!("[Auto-Delete] Removed expired file: {:?}", path);
                     fs::remove_file(path).await?;
                 }
            }
        }
    }
    Ok(())
}
