use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use dav_server::memls::MemLs;
use dav_server::DavHandler;
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;

use crate::cli::{Commands, ServeArgs};
use crate::webdav::{extract_basic_auth, SqliteFs};

pub fn execute(command: Commands) -> Result<()> {
    match command {
        Commands::Serve(args) => serve(args),
    }
}

struct ServerConfig {
    db_path: std::path::PathBuf,
    username: String,
    password: String,
    user_id: String,
}

/// Compare credentials in constant time to prevent timing attacks
fn verify_credentials(
    provided: &crate::webdav::auth::Credentials,
    expected_user: &str,
    expected_pass: &str,
) -> bool {
    let username_match = provided.username.as_bytes().ct_eq(expected_user.as_bytes());
    let password_match = provided.password.as_bytes().ct_eq(expected_pass.as_bytes());

    // Both must match - use constant-time AND
    (username_match & password_match).into()
}

fn serve(args: ServeArgs) -> Result<()> {
    // Validate database exists
    if !args.database.exists() {
        anyhow::bail!("Database file not found: {:?}", args.database);
    }

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    let user_id = args.user_id.unwrap_or_else(|| args.username.clone());
    let config = Arc::new(ServerConfig {
        db_path: args.database,
        username: args.username,
        password: args.password,
        user_id,
    });

    println!("Starting WebDAV server at http://{}", addr);
    println!("Database: {:?}", config.db_path);
    println!("Login: {} -> user_id: {}", config.username, config.user_id);

    // Run the async server
    tokio::runtime::Runtime::new()?.block_on(async { run_server(addr, config).await })?;

    Ok(())
}

async fn run_server(addr: SocketAddr, config: Arc<ServerConfig>) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let config = config.clone();

        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let config = config.clone();
                async move { handle_request(req, config).await }
            });

            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}

async fn handle_request(
    req: Request<Incoming>,
    config: Arc<ServerConfig>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    // Log every incoming request
    eprintln!("[HTTP] {} {} {:?}", req.method(), req.uri(), req.version());
    eprintln!("[HEADERS] Request headers:");
    for (name, value) in req.headers() {
        if let Ok(v) = value.to_str() {
            eprintln!("  {}: {}", name, v);
        }
    }

    // Extract credentials from Basic Auth
    let creds = match extract_basic_auth(req.headers()) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("[AUTH] No valid auth header found");
            // Don't leak error details to client
            let response = Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("WWW-Authenticate", "Basic realm=\"WebDAV\"")
                .body(Full::new(Bytes::from("Unauthorized")))
                .unwrap();
            return Ok(response);
        }
    };

    eprintln!("[AUTH] Extracted credentials - username: '{}', password: '{}' (len: {})",
        creds.username, creds.password, creds.password.len());
    eprintln!("[AUTH] Expected credentials - username: '{}', password: '{}' (len: {})",
        config.username, config.password, config.password.len());

    // Validate username and password using constant-time comparison
    if !verify_credentials(&creds, &config.username, &config.password) {
        eprintln!("[AUTH] Credential verification failed");
        let response = Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("WWW-Authenticate", "Basic realm=\"WebDAV\"")
            .body(Full::new(Bytes::from("Invalid credentials")))
            .unwrap();
        return Ok(response);
    }

    eprintln!("[AUTH] Credential verification succeeded");

    // Create filesystem using the configured user_id
    let fs = SqliteFs::new(config.db_path.clone(), config.user_id.clone());

    // Create DAV handler with autoindex and locking support
    let dav = DavHandler::builder()
        .filesystem(Box::new(fs))
        .locksystem(MemLs::new())
        .autoindex(true)
        .build_handler();

    // Convert request body for dav-server
    let (parts, body) = req.into_parts();
    let body_bytes = match http_body_util::BodyExt::collect(body).await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            let response = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from("Bad Request")))
                .unwrap();
            return Ok(response);
        }
    };

    // Check for forbidden operations on root
    if parts.method == http::Method::DELETE {
        let path = parts.uri.path().trim_matches('/');
        if path.is_empty() {
            eprintln!("[HTTP] Blocked: DELETE on root is forbidden");
            let response = Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Full::new(Bytes::from("Cannot delete root directory")))
                .unwrap();
            return Ok(response);
        }
    }

    let req = Request::from_parts(parts, dav_server::body::Body::from(body_bytes));

    // Handle the WebDAV request
    let response = dav.handle(req).await;

    // Convert response body back
    let (mut parts, body) = response.into_parts();
    let body_bytes = match http_body_util::BodyExt::collect(body).await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => Bytes::new(),
    };

    // Fix Content-Type to include charset=utf-8 for text/* types
    if let Some(content_type) = parts.headers.get("content-type") {
        if let Ok(ct_str) = content_type.to_str() {
            if ct_str.starts_with("text/") && !ct_str.contains("charset") {
                let new_ct = format!("{}; charset=utf-8", ct_str);
                parts.headers.insert("content-type", new_ct.parse().unwrap());
            }
        }
    }

    // Log response
    eprintln!("[HTTP] Response: {} (body: {} bytes)", parts.status, body_bytes.len());
    eprintln!("[HEADERS] Response headers:");
    for (name, value) in &parts.headers {
        if let Ok(v) = value.to_str() {
            eprintln!("  {}: {}", name, v);
        }
    }

    Ok(Response::from_parts(parts, Full::new(body_bytes)))
}
