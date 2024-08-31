use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use std::convert::Infallible;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

async fn handle_request(req: Request<Body>, root_folder: PathBuf) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path();
    let method = req.method();

    let mut file_path = root_folder.clone();
    file_path.push(&path[1..]); // Skip the leading '/'

    // Check if the request is for a script in the /scripts directory
    if file_path.starts_with(root_folder.join("scripts")) && method == &Method::POST {
        // Execute the script
        match execute_script(&file_path, &req).await {
            Ok(response) => return Ok(response),
            Err(status_code) => return Ok(create_error_response(status_code)),
        }
    }

    // Handle GET requests for files
    if method == &Method::GET {
        if file_path.is_dir() {
            // Directory listing
            return Ok(list_directory(&file_path));
        } else {
            // Serve file
            return Ok(serve_file(&file_path));
        }
    }

    // If the method is not supported
    Ok(Response::builder()
        .status(StatusCode::METHOD_NOT_ALLOWED)
        .body(Body::from("405 Method Not Allowed"))
        .unwrap())
}

// Function to serve a file
fn serve_file(file_path: &Path) -> Response<Body> {
    match File::open(file_path) {
        Ok(mut file) => {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).unwrap();

            let mime_type = match file_path.extension().and_then(|ext| ext.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("js") => "text/javascript; charset=utf-8",
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("txt") => "text/plain; charset=utf-8",
                Some("zip") => "application/zip",
                _ => "application/octet-stream",
            };

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime_type)
                .body(Body::from(contents))
                .unwrap()
        }
        Err(_) => create_error_response(StatusCode::NOT_FOUND),
    }
}

// Function to list the contents of a directory
fn list_directory(dir_path: &Path) -> Response<Body> {
    let paths = match fs::read_dir(dir_path) {
        Ok(paths) => paths,
        Err(_) => return create_error_response(StatusCode::FORBIDDEN),
    };

    let mut response_body = String::new();
    response_body.push_str("<html><h1>Directory listing</h1><ul>");
    response_body.push_str("<li><a href=\"../\">..</a></li>");

    for path in paths {
        let file_name = path.unwrap().file_name().into_string().unwrap();
        response_body.push_str(&format!("<li><a href=\"{}\">{}</a></li>", file_name, file_name));
    }

    response_body.push_str("</ul></html>");

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Body::from(response_body))
        .unwrap()
}

// Function to execute a script
async fn execute_script(script_path: &Path, req: &Request<Body>) -> Result<Response<Body>, StatusCode> {
    let output = Command::new(script_path)
        .envs(req.headers().iter().map(|(k, v)| {
            (
                k.to_string(),
                v.to_str().unwrap_or("").to_string(),
            )
        }))
        .env("Method", req.method().to_string())
        .env("Path", req.uri().path().to_string())
        .output();

    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(output.stdout))
                    .unwrap())
            } else {
                let stderr_output = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(stderr_output))
                    .unwrap())
            }
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

// Function to create error responses
fn create_error_response(status_code: StatusCode) -> Response<Body> {
    let status_text = match status_code {
        StatusCode::NOT_FOUND => "404 Not Found",
        StatusCode::FORBIDDEN => "403 Forbidden",
        StatusCode::INTERNAL_SERVER_ERROR => "500 Internal Server Error",
        StatusCode::METHOD_NOT_ALLOWED => "405 Method Not Allowed",
        _ => "400 Bad Request",
    };

    Response::builder()
        .status(status_code)
        .body(Body::from(status_text))
        .unwrap()
}

#[tokio::main]
async fn main() {
    // Get the PORT and ROOT_FOLDER from the command line arguments
    let args: Vec<String> = env::args().collect();
    let port = args.get(1).expect("Port number is required");
    let root_folder = args.get(2).expect("Root folder path is required");

    let root_folder = PathBuf::from(root_folder);
    let addr = SocketAddr::from(([0, 0, 0, 0], port.parse().unwrap()));

    // Print the startup log
    println!("Root folder: {:?}", root_folder.canonicalize().unwrap());
    println!("Server listening on 0.0.0.0:{}", port);

    // Create the server
    let make_svc = make_service_fn(move |_conn| {
        let root_folder = root_folder.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let root_folder = root_folder.clone();
                handle_request(req, root_folder)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    // Run the server
    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}
