use chrono::Local;
use clappers::Clappers;
use comrak::{markdown_to_html, ComrakOptions};
use cwd::cwd;
use die::die;
use lazy_static::lazy_static;
use minifier::{css, js};
use minify::html;
use placeholder::render;
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::fs::{read_to_string, remove_file, write};
use tiny_http::{Response, Server, StatusCode};
use toml::{from_str, Value};
use walkdir::WalkDir;

lazy_static! {
    static ref COMRAK_OPTIONS: ComrakOptions = ComrakOptions::default();
    static ref SANITISE_URL: Regex = Regex::new("[.]{2}").unwrap();
}

fn main() {
    let clappers = Clappers::build()
        .set_flags(vec!["b|build", "c|clean", "s|serve", "v|version"])
        .set_singles(vec!["host", "port"])
        .parse();

    if clappers.get_flag("build") {
        generate_files()
    } else if clappers.get_flag("clean") {
        delete_generated_files()
    } else if clappers.get_flag("serve") {
        serve_htdocs(&clappers)
    } else if clappers.get_flag("version") {
        println!("{}", env!("CARGO_PKG_VERSION"))
    } else {
        show_help()
    }
}

fn generate_files() {
    let htdocs = format!("{}/htdocs", cwd());

    let filenames = WalkDir::new(htdocs)
        .into_iter()
        .filter(|f| f.is_ok())
        .map(|f| f.unwrap().path().display().to_string())
        .filter(|f| f.ends_with(".sssg"))
        .collect::<Vec<String>>();

    for filename in filenames {
        let contents = read_to_string(&filename)
            .unwrap_or_else(|err| die!("Error reading '{}' ({})", filename, err));

        let output = match filename.rsplit('.').skip(1).take(1).next() {
            None => die!(
                "Filename '{}' not in the form <name>.(css|html|js).sssg",
                filename
            ),
            Some(filetype) => match filetype {
                "css" => css::minify(&contents).map_err(|e| e.to_string()),
                "html" => generate_html(&contents),
                "js" => Ok(js::minify(&contents)),
                _ => die!(
                    "Filename '{}' not in the form <name>.(css|html|js).sssg",
                    filename
                ),
            },
        };

        match output {
            Err(err) => die!("Error generating content for '{}' ({})", filename, err),
            Ok(o) => write(&filename.strip_suffix(".sssg").unwrap(), &o)
                .unwrap_or_else(|err| die!("Error writing to '{}' ({})", filename, err)),
        }
    }
}

fn generate_html(contents: &str) -> Result<String, String> {
    let document = from_str(contents).map_err(|_| "TOML parse error")?;
    let config = get_section("config", &document);
    let mut plaintext = get_section("plaintext", &document);

    for (name, markdown) in get_section("markdown", &document) {
        plaintext.insert(name, markdown_to_html(&markdown, &COMRAK_OPTIONS));
    }

    let template_filename = config
        .get("template")
        .ok_or("Template file not defined in 'config' section")?;

    let template = read_to_string(format!("{}/templates/{template_filename}", cwd())).unwrap();

    let output = render(&template, &plaintext)
        .map_err(|e| format!("Template variable '{}' is missing its value", e))?;

    Ok(html::minify(&output))
}

fn get_section(name: &str, document: &Value) -> HashMap<String, String> {
    let mut values = HashMap::new();

    match document.get(name) {
        None => values,
        Some(c) => match c.as_table() {
            None => values,
            Some(t) => {
                for v in t.iter() {
                    values.insert(v.0.to_string(), v.1.as_str().unwrap_or("").to_string());
                }

                values
            }
        },
    }
}

fn serve_htdocs(clappers: &Clappers) {
    let host = match clappers.get_single("host").as_str() {
        "" => "0.0.0.0".to_string(),
        h => h.to_string(),
    };

    let port = match clappers.get_single("port").as_str() {
        "" => "1337".to_string(),
        p => p.to_string(),
    };

    let server = Server::http(format!("{host}:{port}")).unwrap();

    for request in server.incoming_requests() {
        let url = SANITISE_URL.replace_all(request.url(), "_");
        let error_url = url.to_string();

        let (message, status_code) = if url.ends_with(".sssg") {
            (String::from("File not found"), 404)
        } else {
            match read_to_string(&format!("{}/htdocs{}", cwd(), url)) {
                Ok(contents) => (contents, 200),
                Err(_) => (String::from("File not found"), 404),
            }
        };

        let response = Response::from_data(message).with_status_code(StatusCode(status_code));

        println!(
            "[{}] {status_code} {} {}",
            Local::now().naive_local(),
            request.remote_addr(),
            &url
        );

        if request.respond(response).is_err() {
            die!("Error sending response for '{}'", error_url)
        }
    }
}

fn delete_generated_files() {
    let filenames = WalkDir::new(format!("{}/htdocs", cwd()))
        .into_iter()
        .filter(|f| f.is_ok())
        .map(|f| f.unwrap().path().display().to_string())
        .filter(|f| f.ends_with(".sssg"))
        .collect::<Vec<String>>();

    for filename in filenames {
        match filename.rsplit('.').skip(1).take(1).next() {
            None => die!(
                "Filename '{}' not in the form <name>.(css|html|js).sssg",
                filename
            ),
            Some(filetype) => match filetype {
                "css" | "html" | "js" => {
                    let generated_filename = filename.strip_suffix(".sssg").unwrap();

                    remove_file(generated_filename).unwrap_or_else(|err| {
                        die!("Error removing file '{}' ({})", generated_filename, err)
                    });
                }
                _ => die!(
                    "Filename '{}' not in the form <name>.(css|html|js).sssg",
                    filename
                ),
            },
        };
    }
}

fn show_help() {
    println!("TODO")
}
