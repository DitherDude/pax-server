use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use actix_files::NamedFile;
use actix_web::{
    App, HttpResponse, HttpServer, body::BoxBody, error::InternalError, get, http::StatusCode, web,
};
use serde::{Deserialize, Serialize};

#[get("/packages/metadata/{name}")]
async fn metadata(
    name: web::Path<String>,
    data: web::Data<CoreData>,
) -> Result<HttpResponse, actix_web::Error> {
    if let Some(mut location) = path_check(&name, &data.directory) {
        if location.is_dir() {
            location.push(Path::new("metadata.yaml"));
            match yaml_file_to_json_str(&location) {
                Some(body) => Ok(HttpResponse::with_body(StatusCode::OK, BoxBody::new(body))),
                None => Err(InternalError::new(
                    "Error reading package metadata!",
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
                .into()),
            }
        } else {
            Err(InternalError::new(
                "Requested package could not be found.",
                StatusCode::NOT_FOUND,
            )
            .into())
        }
    } else {
        Err(InternalError::new(
            "You do not have access to this location.",
            StatusCode::FORBIDDEN,
        )
        .into())
    }
}

#[get("/package/{name}/{ver}")]
async fn package(
    blocks: web::Path<(String, String)>,
    data: web::Data<CoreData>,
) -> Result<NamedFile, actix_web::Error> {
    let (name, ver) = blocks.into_inner();
    if let Some(location) = path_check(&name, &data.directory) {
        if location.is_dir() {
            let name = format!("{name}-{ver}.pax");
            if let Some(file) = path_check(&name, &location) {
                match actix_files::NamedFile::open(file.as_os_str()) {
                    Ok(file) => return Ok(file),
                    Err(_) => {
                        return Err(InternalError::new(
                            "Error reading package!",
                            StatusCode::INTERNAL_SERVER_ERROR,
                        )
                        .into());
                    }
                };
            }
        } else {
            return Err(InternalError::new(
                "Requested file could not be found.",
                StatusCode::NOT_FOUND,
            )
            .into());
        }
    } else {
        return Err(InternalError::new(
            "You do not have access to this location.",
            StatusCode::FORBIDDEN,
        )
        .into());
    }
    Err(InternalError::new("Something went wrong.", StatusCode::INTERNAL_SERVER_ERROR).into())
}

fn yaml_file_to_json_str(path: &PathBuf) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut data = String::new();
    file.read_to_string(&mut data).ok()?;
    let body: PackageMetadata = serde_yml::from_str(&data).ok()?;
    serde_json::to_string(&body).ok()
}

#[get("/version")]
async fn version() -> Result<HttpResponse, actix_web::Error> {
    Ok(HttpResponse::with_body(
        StatusCode::OK,
        BoxBody::new(env!("CARGO_PKG_VERSION")),
    ))
}

#[derive(Clone)]
struct CoreData {
    directory: PathBuf,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut directory = std::env::current_dir()?;
    let mut port = 8080u16;
    let args = std::env::args().collect::<Vec<String>>();
    let mut args = args.iter().skip(1);
    while let Some(arg) = args.next() {
        if let Some(arg) = arg.strip_prefix("--") {
            match arg {
                "directory" => {
                    if let Some(loc) = args.next() {
                        directory = PathBuf::from(loc)
                    }
                }
                "port" => {
                    if let Some(Ok(val)) = args.next().map(|x| x.parse::<u16>()) {
                        port = val
                    }
                }
                _ => panic!("Unknown long-flag {arg}!"),
            }
        } else if let Some(arg) = arg.strip_prefix("-") {
            for arg in arg.chars() {
                match arg {
                    'd' => {
                        if let Some(loc) = args.next() {
                            directory = PathBuf::from(loc)
                        }
                    }
                    'p' => {
                        if let Some(Ok(val)) = args.next().map(|x| x.parse::<u16>()) {
                            port = val
                        }
                    }
                    _ => panic!("Unknown short-flag {arg}!"),
                }
            }
        } else {
            panic!("Unknown parameter {arg}!");
        }
    }
    println!("Using folder {}", directory.display());
    println!("Using port {port}");
    let data = CoreData { directory };
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(data.clone()))
            .service(metadata)
            .service(package)
            .service(version)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

#[derive(Serialize, Deserialize, Debug)]
struct PackageMetadata {
    name: String,
    description: String,
    version: String,
    origin: String,
    dependencies: Vec<String>,
    runtime_dependencies: Vec<String>,
    build: String,
    install: String,
    uninstall: String,
    hash: String,
}

fn path_check(subpath_str: &str, origpath: &Path) -> Option<PathBuf> {
    /*
    The following code is not my own, but adapted slightly to match my use-case.

    Project Title: tower-rs/tower-http
    Snippet Title: build_and_validate_path
    Author(s): carllerche and github:tower-rs:publish
    Date: 03/Jun/2025
    Date Accessed: 10/Aug/2025 01:30AM AEST
    Code version: 0.6.6
    Type: Source Code
    Availability: https://docs.rs/tower-http/latest/src/tower_http/services/fs/serve_dir/mod.rs.html#458-483
    Licence: MIT (docs.rs) / None (github.com)
     */
    let mut finalpath = origpath.to_path_buf();
    let subpath = subpath_str.trim_start_matches('/');
    let subpath = Path::new(subpath);
    for component in subpath.components() {
        match component {
            Component::Normal(comp) => {
                if Path::new(&comp)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
                {
                    finalpath.push(comp)
                } else {
                    return None;
                }
            }
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return None;
            }
        }
    }
    Some(finalpath)
}
