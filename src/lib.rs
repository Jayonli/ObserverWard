use crate::cli::WardArgs;
use observer_ward_what_server::NmapFingerPrintLib;
use observer_ward_what_web::{RequestOption, TemplateResult, WhatWebResult};
use prettytable::csv::Reader;
use prettytable::{color, Attr, Cell, Row, Table};
use reqwest::redirect::Policy;
use reqwest::{header, Proxy};
use serde_json::json;
use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::Cursor;
use std::io::{BufRead, Read};
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{io, process};
use term::color::Color;
use tokio::process::Command;

pub mod cli;

pub fn print_color(mut string: String, color: Color, nl: bool) {
    if nl {
        string.push('\n')
    }
    if let Some(mut t) = term::stdout() {
        t.fg(color).unwrap();
        write!(t, "{}", string).unwrap();
        t.reset().unwrap();
    } else {
        print!("{}", string);
    };
}

pub fn print_what_web(what_web_result: &WhatWebResult) {
    let color_web_name: Vec<String> = what_web_result.name.iter().map(String::from).collect();
    let status_code =
        reqwest::StatusCode::from_u16(what_web_result.status_code).unwrap_or_default();
    if !what_web_result.name.is_empty() {
        print!("[ {} |", what_web_result.url);
        print_color(format!("{:?}", color_web_name), term::color::GREEN, false);
        print!(" | {} | ", what_web_result.length);
        if status_code.is_success() {
            print_color(format!("{:?}", status_code), term::color::GREEN, false);
        } else {
            print_color(format!("{:?}", status_code), term::color::RED, false);
        }
        println!(" | {} ]", what_web_result.title);
    } else {
        println!(
            "[ {} | {:?} | {} | {} | {} ]",
            what_web_result.url,
            color_web_name,
            what_web_result.length,
            what_web_result.status_code,
            what_web_result.title,
        );
    }
}

pub fn print_nuclei(what_web_result: &WhatWebResult) {
    for template in what_web_result.template_result.iter() {
        print_color(
            format!("[{}]", template.template_id),
            term::color::RED,
            false,
        );
        println!(" | [{}] ", template.matched_at);
    }
}

pub async fn webhook_results(what_web_result: WhatWebResult, webhook_url: &str) -> WhatWebResult {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    let ua = "Mozilla/5.0 (X11; Linux x86_64; rv:94.0) Gecko/20100101 Firefox/94.0";
    headers.insert(header::USER_AGENT, header::HeaderValue::from_static(ua));
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .pool_max_idle_per_host(0)
        .danger_accept_invalid_certs(true)
        .redirect(Policy::none())
        .timeout(Duration::new(10, 0));
    let what_web_result_json = json!(what_web_result)
        .as_object()
        .unwrap_or(&serde_json::Map::new())
        .clone();
    let _: Result<_, _> = client
        .build()
        .unwrap_or_default()
        .post(webhook_url)
        .json(&what_web_result_json)
        .send()
        .await;
    return what_web_result.clone();
}

pub fn print_opening() {
    let s = r#" __     __     ______     ______     _____
/\ \  _ \ \   /\  __ \   /\  == \   /\  __-.
\ \ \/ ".\ \  \ \  __ \  \ \  __<   \ \ \/\ \
 \ \__/".~\_\  \ \_\ \_\  \ \_\ \_\  \ \____-
  \/_/   \/_/   \/_/\/_/   \/_/ /_/   \/____/
Community based web fingerprint analysis tool."#;
    print_color(s.to_string(), term::color::GREEN, true);
    let info = r#"_______________________________________________
: https://github.com/0x727/FingerprintHub     :
: https://github.com/0x727/ObserverWard_0x727 :
 ----------------------------------------------"#;
    print_color(info.to_string(), term::color::YELLOW, true);
}

pub struct Helper {
    request_option: RequestOption,
    config: WardArgs,
}

impl Helper {
    pub fn new(config: &WardArgs) -> Self {
        let ro = RequestOption::new(&config.timeout, &config.proxy);
        Self {
            request_option: ro,
            config: config.clone(),
        }
    }
    pub async fn run(&self) {
        if self.config.update_fingerprint {
            self.download_file_from_github(
                "https://0x727.github.io/FingerprintHub/web_fingerprint_v3.json",
                "web_fingerprint_v3.json",
            )
            .await;
            process::exit(0);
        }
        if self.config.update_self {
            self.update_self().await;
            process::exit(0);
        }
        if self.config.update_plugins {
            self.download_file_from_github(
                "https://github.com/0x727/FingerprintHub/releases/download/default/plugins.zip",
                "plugins.zip",
            )
            .await;
            process::exit(0);
        }
    }
}

impl Helper {
    pub async fn update_self(&self) {
        let mut base_url =
            String::from("https://github.com/0x727/ObserverWard_0x727/releases/download/default/");
        let mut download_name = "observer_ward_amd64";
        if cfg!(target_os = "windows") {
            download_name = "observer_ward.exe";
        } else if cfg!(target_os = "linux") {
            download_name = "observer_ward_amd64";
        } else if cfg!(target_os = "macos") {
            download_name = "observer_ward_darwin";
        };
        base_url.push_str(download_name);
        let save_filename = "update_".to_owned() + download_name;
        self.download_file_from_github(&base_url, &save_filename)
            .await;
        println!(
            "Please rename the file {} => {}",
            save_filename, download_name
        );
    }
    pub fn read_results_file(&self) -> Vec<WhatWebResult> {
        let mut results: Vec<WhatWebResult> = Vec::new();
        let read_file_data = |path: &str| {
            let mut file = match File::open(path) {
                Err(err) => {
                    println!("{}", err.to_string());
                    std::process::exit(0);
                }
                Ok(file) => file,
            };
            let mut data = String::new();
            file.read_to_string(&mut data).unwrap();
            data
        };
        if !self.config.json.is_empty() {
            let data = read_file_data(&self.config.json);
            let wwr: Vec<WhatWebResult> = serde_json::from_str(&data).expect("BAD JSON");
            results.extend(wwr);
        }
        if !self.config.csv.is_empty() {
            let rdr = Reader::from_path(&self.config.csv).expect("BAD CSV");
            let iter: csv::DeserializeRecordsIntoIter<File, WhatWebResult> = rdr.into_deserialize();
            let wwr: Vec<WhatWebResult> = iter.filter_map(Result::ok).collect();
            results.extend(wwr);
        }
        results
    }
    pub async fn get_plugins_by_nuclei(&self, w: WhatWebResult) -> WhatWebResult {
        let mut wwr = w.clone();
        let mut plugins_set: HashSet<String> = HashSet::new();
        let mut exist_plugins: Vec<String> = Vec::new();
        for name in wwr.name.iter() {
            let plugins_name_path = Path::new(&self.config.plugins).join(name);
            if plugins_name_path.exists() {
                if let Some(p_path) = plugins_name_path.to_str() {
                    exist_plugins.push(p_path.to_string())
                }
            }
        }
        if exist_plugins.is_empty() {
            return wwr;
        }
        let mut command_line = Command::new("nuclei");
        command_line.args([
            "-u",
            &wwr.url,
            "-no-color",
            "-timeout",
            &(self.config.timeout + 5).to_string(),
        ]);
        command_line.args([
            "-H",
            "User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:94.0) Gecko/20100101 Firefox/94.0",
        ]);
        for p in exist_plugins.iter() {
            command_line.args(["-t", p]);
        }
        command_line.args(["-silent", "-json"]);
        let output = command_line.output().await.unwrap();
        if let Ok(template_output) = String::from_utf8(output.stdout) {
            let templates_output: Vec<String> = template_output
                .split_terminator('\n')
                .map(|s| s.to_string())
                .collect();
            for line in templates_output.iter() {
                let template: TemplateResult = serde_json::from_str(&line).unwrap();
                wwr.template_result.push(template.clone());
                plugins_set.insert(template.template_id);
            }
        }
        wwr.plugins = plugins_set;
        if !wwr.plugins.is_empty() {
            wwr.priority = wwr.priority + 1;
        }
        return wwr;
    }
    pub async fn download_file_from_github(&self, update_url: &str, filename: &str) {
        let proxy = self.request_option.proxy.clone();
        let proxy_obj = Proxy::custom(move |_url| proxy.clone());
        let client = reqwest::Client::builder().proxy(proxy_obj);
        match client.build().unwrap().get(update_url).send().await {
            Ok(response) => {
                let self_path: PathBuf = env::current_exe().unwrap_or(PathBuf::new());
                let path = Path::new(&self_path).parent().unwrap_or(Path::new(""));
                let mut file = std::fs::File::create(path.join(filename)).unwrap();
                let mut content = Cursor::new(response.bytes().await.unwrap());
                std::io::copy(&mut content, &mut file).unwrap();
                println!(
                    "Complete {} update: {} file size => {:?}",
                    filename,
                    filename,
                    file.metadata().unwrap().len()
                );
            }
            Err(_) => {
                println!(
                    "Update failed, please download {} to local directory manually.",
                    update_url
                );
            }
        };
    }
}

pub fn read_file_to_target(file_path: &String) -> HashSet<String> {
    if let Ok(lines) = read_lines(file_path) {
        let target_list: Vec<String> = lines.filter_map(Result::ok).collect();
        return HashSet::from_iter(target_list);
    }
    return HashSet::from_iter([]);
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

pub fn print_results_and_save(
    json: &String,
    csv: &String,
    results: Vec<WhatWebResult>,
    has_plugins: bool,
) {
    if !json.is_empty() {
        let out = File::create(&json).expect("Failed to create file");
        serde_json::to_writer(out, &results).expect("Failed to save file")
    }
    let mut table = Table::new();
    let mut headers = vec![
        Cell::new("url"),
        Cell::new("name"),
        Cell::new("length"),
        Cell::new("status_code"),
        Cell::new("title"),
        Cell::new("priority"),
    ];
    if has_plugins {
        headers.push(Cell::new("plugins"))
    }
    table.set_titles(Row::new(headers.clone()));
    for res in &results {
        let wwn: Vec<String> = res.name.iter().map(String::from).collect();
        let status_code = reqwest::StatusCode::from_u16(res.status_code).unwrap_or_default();
        let mut status_code_color = Attr::ForegroundColor(color::RED);
        if status_code.is_success() {
            status_code_color = Attr::ForegroundColor(color::GREEN);
        }
        let mut rows = vec![
            Cell::new(&res.url.as_str()),
            Cell::new(&wwn.join("\n")).with_style(Attr::ForegroundColor(color::GREEN)),
            Cell::new(&res.length.to_string()),
            Cell::new(&res.status_code.to_string()).with_style(status_code_color),
            Cell::new(&textwrap::fill(res.title.as_str(), 40)),
            Cell::new(&res.priority.to_string()),
        ];
        if has_plugins {
            let wp: Vec<String> = res.plugins.iter().map(String::from).collect();
            rows.push(Cell::new(&wp.join("\n")).with_style(Attr::ForegroundColor(color::RED)))
        }
        table.add_row(Row::new(rows));
    }
    if !csv.is_empty() {
        let out = File::create(&csv).expect("Failed to create file");
        table.to_csv(out).expect("Failed to save file");
    }
    let mut table = Table::new();
    table.set_titles(Row::new(headers.clone()));
    for res in &results {
        let wwn: Vec<String> = res.name.iter().map(String::from).collect();
        let status_code = reqwest::StatusCode::from_u16(res.status_code).unwrap_or_default();
        let mut status_code_color = Attr::ForegroundColor(color::RED);
        if status_code.is_success() {
            status_code_color = Attr::ForegroundColor(color::GREEN);
        }
        let mut rows = vec![
            Cell::new(&res.url.as_str()),
            Cell::new(&wwn.join("\n")).with_style(Attr::ForegroundColor(color::GREEN)),
            Cell::new(&res.length.to_string()),
            Cell::new(&res.status_code.to_string()).with_style(status_code_color),
            Cell::new(&textwrap::fill(res.title.as_str(), 40)),
            Cell::new(&res.priority.to_string()),
        ];
        if has_plugins {
            let wp: Vec<String> = res.plugins.iter().map(String::from).collect();
            rows.push(Cell::new(&wp.join("\n")).with_style(Attr::ForegroundColor(color::RED)))
        }
        table.add_row(Row::new(rows));
    }
    if table.len() > 0 {
        print_color(
            String::from("Important technology:\n"),
            term::color::YELLOW,
            true,
        );
        table.printstd();
    }
}

pub fn read_nmap_fingerprint() -> Vec<NmapFingerPrintLib> {
    let self_path: PathBuf = env::current_exe().unwrap_or(PathBuf::new());
    let path = Path::new(&self_path).parent().unwrap_or(Path::new(""));
    let mut file = match File::open(path.join("nmap_service_probes.json")) {
        Err(_) => {
            println!("The nmap fingerprint library cannot be found in the current directory!");
            std::process::exit(0);
        }
        Ok(file) => file,
    };
    let mut data = String::new();
    file.read_to_string(&mut data).unwrap();
    let nmap_fingerprint: Vec<NmapFingerPrintLib> = serde_json::from_str(&data).expect("BAD JSON");
    return nmap_fingerprint;
}
