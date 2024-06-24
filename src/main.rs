// File: src/main.rs

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::thread;

fn main() {
    
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: rustwebserver PORT ROOT_FOLDER");
        return;
    }
    let port = &args[1];
    let root_folder = &args[2];

    // existance du root file
    if !Path::new(root_folder).exists() {
        eprintln!("Error: The specified root folder does not exist: {}", root_folder);
        return;
    }

    //  startup log
    println!("Root folder: {}", fs::canonicalize(root_folder).unwrap().display());
    println!("Server listening on 0.0.0.0:{}", port);

    // Start TCP listener
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).unwrap();
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        let root_folder = root_folder.clone();
        thread::spawn(move || {
            handle_connection(stream, &root_folder);
        });
    }
}

fn handle_connection(mut stream: TcpStream, root_folder: &str) {
    let mut buffer = [0; 8192];
    if let Err(e) = stream.read(&mut buffer) {
        eprintln!("Failed to read from stream: {}", e);
        return;
    }

    let request = String::from_utf8_lossy(&buffer[..]);
    let request_line = match request.lines().next() {
        Some(line) => line,
        None => {
            eprintln!("Failed to parse request line");
            respond_with_error(&mut stream, 400, "Bad Request");
            return;
        }
    };

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap();
    let path = parts.next().unwrap();
    let path = path.trim_start_matches('/');

    println!("Received {} request for path: {}", method, path);

    if method == "GET" {
        handle_get(&mut stream, root_folder, path);
    } else if method == "POST" {
        handle_post(&mut stream, root_folder, path, &request);
    } else {
        respond_with_error(&mut stream, 405, "Method Not Allowed");
    }

    // Log the request and response
    if let Ok(client_address) = stream.peer_addr() {
        println!("{} {} -> {}", method, client_address, path);
    }
}

fn handle_get(stream: &mut TcpStream, root_folder: &str, path: &str) {
    let file_path = format!("{}/{}", root_folder, path);
    println!("Handling GET request for: {}", file_path);

    if !Path::new(&file_path).exists() {
        println!("File not found: {}", file_path);
        respond_with_error(stream, 404, "Not Found");
        return;
    }

    match fs::read(&file_path) {
        Ok(contents) => {
            let content_type = get_content_type(&file_path);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-type: {}\r\nConnection: close\r\n\r\n",
                content_type
            );
            if let Err(e) = stream.write(response.as_bytes()) {
                eprintln!("Failed to write response header: {}", e);
            }
            if let Err(e) = stream.write(&contents) {
                eprintln!("Failed to write response body: {}", e);
            }
        }
        Err(e) => {
            println!("Forbidden: Cannot read file: {}, error: {}", file_path, e);
            respond_with_error(stream, 403, "Forbidden");
        }
    }
}

fn handle_post(stream: &mut TcpStream, root_folder: &str, path: &str, request: &str) {
    let script_path = format!("{}/{}", root_folder, path);
    println!("Handling POST request for: {}", script_path);

    if !path.starts_with("scripts/") {
        println!("Script not found: {}", script_path);
        respond_with_error(stream, 404, "Not Found");
        return;
    }

    if !Path::new(&script_path).exists() {
        println!("Script not found: {}", script_path);
        respond_with_error(stream, 404, "Not Found");
        return;
    }

    // preparer variable
    let mut env_vars = vec![
        ("Method", "POST"),
        ("Path", path),
    ];
    for line in request.lines().skip(1) {
        if let Some((key, value)) = line.split_once(':') {
            env_vars.push((key.trim(), value.trim()));
        }
    }

    // executer le script
    match Command::new(&script_path)
        .envs(env_vars)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}",
                    String::from_utf8_lossy(&output.stdout)
                );
                if let Err(e) = stream.write(response.as_bytes()) {
                    eprintln!("Failed to write response: {}", e);
                }
            } else {
                let response = format!(
                    "HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                if let Err(e) = stream.write(response.as_bytes()) {
                    eprintln!("Failed to write error response: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to execute script: {}, error: {}", script_path, e);
            respond_with_error(stream, 500, "Internal Server Error");
        }
    }
}

fn respond_with_error(stream: &mut TcpStream, status_code: u16, status_text: &str) {
    let response = format!(
        "HTTP/1.1 {} {}\r\nConnection: close\r\n\r\n",
        status_code, status_text
    );
    if let Err(e) = stream.write(response.as_bytes()) {
        eprintln!("Failed to write error response: {}", e);
    }
}

fn get_content_type(file_path: &str) -> &str {
    if file_path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if file_path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if file_path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if file_path.ends_with(".png") {
        "image/png"
    } else if file_path.ends_with(".jpg") || file_path.ends_with(".jpeg") {
        "image/jpeg"
    } else if file_path.ends_with(".zip") {
        "application/zip"
    } else {
        "application/octet-stream"
    }
}
