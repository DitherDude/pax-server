use actix_files::NamedFile;
use actix_web::{
    App, HttpResponse, HttpServer, body::BoxBody, error::InternalError, get, http::StatusCode, web,
};
use semver::Version as SemVer;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, DirEntry},
    io::Read,
    path::{Component, Path, PathBuf},
};

#[get("/packages/metadata/{name}")]
async fn metadata(
    name: web::Path<String>,
    data: web::Data<CoreData>,
    info: web::Query<Version>,
) -> Result<HttpResponse, actix_web::Error> {
    let location = if let Some(location) = path_check(&name, &data.directory) {
        if location.is_dir() {
            location
        } else {
            return Err(InternalError::new(
                "Requested package could not be found.",
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
    };
    let location = if let Some(ver) = &info.v {
        get_version(&location, ver)
    } else {
        get_latest(&location)
    };
    if let Some(location) = location
        && location.is_file()
    {
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
            "Requested package's version's metadata could not be found.",
            StatusCode::NOT_FOUND,
        )
        .into())
    }
}

fn get_latest(path: &Path) -> Option<PathBuf> {
    let mut dirs = path
        .read_dir()
        .ok()?
        .filter_map(|x| x.ok().filter(|x| x.path().is_dir()))
        .collect::<Vec<DirEntry>>();
    dirs.sort_by_key(|x| {
        SemVer::parse(&x.file_name().to_string_lossy()).unwrap_or(SemVer::new(0, 0, 0))
    });
    let mut latest = dirs.last()?.path();
    latest.push(Path::new("metadata.yaml"));
    if latest.is_file() { Some(latest) } else { None }
}

fn get_version(path: &Path, ver: &str) -> Option<PathBuf> {
    let dirs = path
        .read_dir()
        .ok()?
        .filter_map(|x| x.ok().filter(|x| x.path().is_dir()));
    let split = ver.split('.').collect::<Vec<&str>>();
    let dirs = match split.len() {
        1 => Some(
            dirs.filter(|x| {
                x.file_name()
                    .into_string()
                    .is_ok_and(|x| x.starts_with(&format!("{}.", split[0])))
            })
            .collect::<Vec<DirEntry>>(),
        ),
        2 => Some(
            dirs.filter(|x| {
                x.file_name()
                    .into_string()
                    .is_ok_and(|x| x.starts_with(&format!("{}.{}.", split[0], split[1])))
            })
            .collect(),
        ),
        3 => Some(
            dirs.filter(|x| x.file_name().into_string().is_ok_and(|x| x == ver))
                .collect(),
        ),
        _ => None,
    };
    let dirs = if let Some(mut dirs) = dirs {
        dirs.sort_by_key(|x| {
            SemVer::parse(&x.file_name().to_string_lossy()).unwrap_or(SemVer::new(0, 0, 0))
        });
        Some(dirs)
    } else {
        None
    };
    let mut latest = dirs?.last()?.path();
    latest.push(Path::new("metadata.yaml"));
    if latest.is_file() { Some(latest) } else { None }
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
    let body: PackageMetadata = serde_norway::from_str(&data).ok()?;
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

#[derive(Deserialize)]
struct Version {
    v: Option<String>,
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
    build_dependencies: Vec<String>,
    runtime_dependencies: Vec<String>,
    build: String,
    install: String,
    uninstall: String,
    purge: String,
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
