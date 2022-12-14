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
use std::ffi::OsStr;
use std::env;
use std::io::prelude::*;
use regex::Regex;
use tera::{Tera,Context};

struct TeraTemplates {
    template: Tera
}

struct RequestSauce {
    content_type: String,
    content_length: f64,
}

#[derive(Debug)]
enum RequestSauceError {
    ShitHitTheFan,
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

#[get("/")]
async fn home(tera: &State<TeraTemplates>) -> (ContentType, String) {
    println!("[+]Conex??o.");
    let mut files: Vec<(String,String)> = Vec::new();
    for file in read_dir(env::current_dir().unwrap()).unwrap() {
        let file_name = Path::new(file.as_ref().unwrap().file_name().to_str().unwrap()).to_owned();
        if Path::is_file(&file_name) {
            let file_type = Path::new(file.unwrap().file_name().as_os_str()).extension().unwrap_or_else(||OsStr::new("FUCK")).to_str().unwrap().to_owned();
            files.push((file_name.to_str().unwrap().to_owned(),file_type))
        }
    }
    let mut context = Context::new();
    context.insert("results".to_owned(),&files);
    match tera.template.render("index", &context) {
        Ok(template) => return (ContentType::HTML,template),
        Err(_) => return (ContentType::HTML,"Deu merda".to_owned())
    };
}

#[get("/static/<file..>")]
async fn return_static(file: PathBuf) -> Result<(ContentType,Vec<u8>),Status> {
    match file.to_str().unwrap() {
        "style.css" => return Ok((ContentType::CSS,include_bytes!("../static/style.css").to_vec())),
        "favicon.png" => return Ok((ContentType::PNG,include_bytes!("../static/favicon.png").to_vec())),
        "404.html" => return Ok((ContentType::HTML,include_bytes!("../static/404.html").to_vec())),
        _ => Err(Status::NotFound)
    }
}

#[get("/<file..>")]
async fn return_files<'a>(file: PathBuf) -> Result<SeekStream<'a>,Status> {
    let path = Path::new(&env::current_dir().unwrap()).join(file);
    match SeekStream::from_path(&path){
        Ok(response) => Ok(response),
        Err(_) => Err(Status::NotFound)
    }
}

#[catch(404)]
async fn not_found() -> Redirect {
    Redirect::to("/static/404.html")
}

#[launch]
fn rocket() -> _  {
    //loading templates
    let mut tera = Tera::default();
    tera.add_raw_template("index", &include_str!("../templates/index.html.tera")).unwrap();
    tera.add_raw_template("upload", &include_str!("../templates/upload.html.tera")).unwrap();
    let mut config = Config::release_default();
    config.address = IpAddr::V4(Ipv4Addr::new(0,0,0,0));
    config.port = 80;
    println!("[+]Iniciando\n[>]Servindo em http://127.0.0.1:80");

    rocket::build()
    .mount("/",routes![home,return_static,return_files,upload_handler])
    .manage(TeraTemplates{template:tera})
    .register("/",catchers![not_found])
    .configure(config)
}