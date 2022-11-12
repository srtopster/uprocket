#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use] extern crate rocket;
use rocket::request::{FromRequest,Outcome};
use rocket::config::Config;
use rocket::Request;
use rocket::State;
use rocket::http::{Status,ContentType};
use rocket::data::{Data,ToByteUnit};
use rocket::response::Redirect;
use rocket_seek_stream::SeekStream;
use std::path::{Path,PathBuf};
use std::net::{IpAddr,Ipv4Addr};
use std::fs::{File,read_dir};
use std::env;
use std::io::prelude::*;
use std::io::Cursor;
use regex::Regex;
use serde::Serialize;
use tera::{Tera,Context};

struct TeraTemplates {
    template: Tera
}

struct CurrentDir(PathBuf);

struct RequestSauce {
    content_type: String,
    content_length: f64,
}

#[derive(Debug)]
enum RequestSauceError {
    ShitHitTheFan,
}

#[derive(Serialize)]
struct TemplateData {
    name: String,
    data_type: String,
    url: String
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for RequestSauce {
    type Error = RequestSauceError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let c_type = req.headers().get_one("Content-Type");
        let c_length = req.headers().get_one("Content-Length");
        if c_type.is_some() && c_length.is_some() {
            return Outcome::Success(RequestSauce {
                content_type: c_type.unwrap().to_string(),
                content_length: c_length.unwrap().parse::<f64>().unwrap()
            });
        }else {
            Outcome::Failure((Status::BadRequest,RequestSauceError::ShitHitTheFan))
        }
    }
}

#[post("/",data="<file_data>")]
async fn upload_handler(tera: &State<TeraTemplates>, headers: RequestSauce,file_data: Data<'_>) -> Result<(ContentType, String),Status> {
    println!("[+] Recebendo arquivo...\n[>]{:.2} Mb",headers.content_length/1000000_f64);
    let buffer = file_data.open(512.megabytes()).into_bytes().await.unwrap();
    let end_boundary = format!("--{}--",headers.content_type.split("=").nth(1).expect("Erro ao pegar boundary").to_owned());
    let split_pos = buffer.windows(4).position(|pos|pos == b"\r\n\r\n").unwrap()+4;
    let split_end = buffer.len()-end_boundary.len()-4;
    let headers = String::from_utf8_lossy(&buffer[0..split_pos]);
    let content = &buffer[split_pos..split_end];

    let re = Regex::new("filename=\"(?P<filename>.*)\"").unwrap();
    let captures = re.captures(&headers).unwrap();
    let filename = &captures["filename"].to_owned();

    let cwd = env::current_dir().unwrap();
    if Path::exists(&Path::new(&cwd).join(filename)) {
        println!("[-] Arquivo existente !");
        return Err(Status::Forbidden);
    }
    let mut local_file = File::create(Path::new(&cwd).join(filename)).unwrap();
    local_file.write(content).unwrap();
    println!("[+] Arquivo salvo.");
    println!("[+]Filename: {}",filename);
    let mut context = Context::new();
    context.insert("file",filename);
    match tera.template.render("upload", &context) {
        Ok(template) => return Ok((ContentType::HTML,template)),
        Err(_) => return Ok((ContentType::HTML,"Deu merda".to_owned()))
    };
}

#[get("/<request_path..>")]
async fn home<'a>(tera: &State<TeraTemplates>,request_path: PathBuf, current_dir: &State<CurrentDir>) -> Result<SeekStream<'a>,Status> {
    println!("[+]Conexão.");
    let local_request_path = current_dir.0.join(&request_path);

    if !local_request_path.exists() {
        return Err(Status::NotFound)
    }

    if local_request_path.is_file() {
        match SeekStream::from_path(&local_request_path){
            Ok(response) => return Ok(response),
            Err(_) => return Err(Status::InternalServerError)
        };
    }else if local_request_path.is_dir() {
        let mut template_data: Vec<TemplateData> = Vec::new();
        for entry in read_dir(local_request_path).unwrap() {
            let file_name = entry.as_ref().unwrap().file_name();
            if file_name.to_string_lossy().starts_with(".") {
                continue;
            }
            let file_url = request_path.join(&file_name).to_string_lossy().to_string().replace(r"\", "/");
            let entry = entry.unwrap().path();
            if entry.is_file() {
                let file_type: String = match Path::new(&file_name.as_os_str()).extension() {
                    Some(ext) => {
                        let ext = ext.to_string_lossy().to_string();
                        match ext.as_str() {
                            "mp4"|"webm"|"mov" => "video".into(),
                            "png"|"jpg"|"gif"|"jpeg" => "image".into(),
                            "mp3" => "audio".into(),
                            _ => "unknow".into()
                        }
                    }
                    None => "noext".into()
                };

                template_data.push(TemplateData { 
                    name: file_name.to_string_lossy().into(), 
                    data_type: file_type,
                    url: format!("/{}",file_url)
                });
            }else if entry.is_dir() {
                template_data.push(TemplateData { 
                    name: file_name.to_string_lossy().into(), 
                    data_type: "dir".into(),
                    url: format!("/{}",file_url)
                });
            }
        }
        let mut context = Context::new();
        context.insert("data", &template_data);
        match tera.template.render("index",&context) {
            Ok(template) => {
                let template_bytes = template.as_bytes().to_owned();
                return Ok(SeekStream::with_opts(Cursor::new(template_bytes.to_owned()), template_bytes.len() as u64, "text/html"))
            }
            Err(err) => {
                println!("{}",err);
                return Err(Status::InternalServerError)
            }
        };
    }else {
        Err(Status::InternalServerError)
    }
}

#[get("/static/<file..>")]
async fn return_static(file: PathBuf) -> Result<(ContentType,Vec<u8>),Status> {
    match file.to_str().unwrap() {
        "script.js" => return Ok((ContentType::JavaScript,include_bytes!("../static/script.js").to_vec())),
        "style.css" => return Ok((ContentType::CSS,include_bytes!("../static/style.css").to_vec())),
        "favicon.png" => return Ok((ContentType::PNG,include_bytes!("../static/favicon.png").to_vec())),
        "404.html" => return Ok((ContentType::HTML,include_bytes!("../static/404.html").to_vec())),
        _ => Err(Status::NotFound)
    }
}

#[catch(404)]
async fn not_found() -> Redirect {
    Redirect::to("/static/404.html")
}

#[launch]
fn rocket() -> _  {
    //pega porta dos args, se não tiver usa 8080;
    let port: u16 = match env::args().nth(1) {
        Some(arg) => arg.parse().unwrap_or(8080),
        None => 8080
    };

    //pega current dir
    let current_dir = env::current_dir().unwrap();

    //loading templates
    let mut tera = Tera::default();
    tera.add_raw_template("index", &include_str!("../templates/index.html.tera")).unwrap();
    tera.add_raw_template("upload", &include_str!("../templates/upload.html.tera")).unwrap();
    let mut config = Config::release_default();
    config.address = IpAddr::V4(Ipv4Addr::new(0,0,0,0));
    config.port = port;
    println!("[+]Iniciando\n[>]Servindo em http://127.0.0.1:{}",port);

    rocket::build()
    .mount("/",routes![home,return_static,upload_handler])
    .manage(TeraTemplates{template:tera})
    .manage(CurrentDir(current_dir))
    .register("/",catchers![not_found])
    .configure(config)
}