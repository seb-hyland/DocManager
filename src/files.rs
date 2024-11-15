#![allow(non_snake_case)]
use dioxus::prelude::*;
use homedir::my_home;
use rusqlite::{params, Connection, Result};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, LazyLock},
};
use tokio::process::Command;
use indexmap::IndexMap;

static DOC_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    my_home()
        .expect("Failed to get documentation directory")
        .unwrap()
        .join("Documents/iGEM-2025")
});

struct Files {
    current_path: PathBuf,
    path_contents: Vec<PathBuf>,
    directories: Vec<PathBuf>,
    filestructs: Vec<Vec<String>>,
    attributes: Vec<(String, String)>,
    err: Option<String>,
}

impl Files {
    fn new() -> Self {
        let mut files = Self {
            current_path: DOC_DIR.clone(),
            path_contents: vec![],
            directories: vec![],
            filestructs: vec![],
            attributes: vec![],
            err: None,
        };
        files.refresh_paths();
        files
    }

    fn refresh_paths(&mut self) {
        match std::fs::read_dir(&self.current_path) {
            Ok(entries) => {
                self.path_contents.clear();
                self.clear_err();
                for entry in entries {
                    if let Ok(entry) = entry {
                        self.path_contents.push(entry.path());
                    }
                }
                self.refresh_display();
            }
            Err(err) => {
                self.err = Some(format!("An error occurred: {:?}", err));
            }
        }
    }

    fn refresh_display(&mut self) {
        self.directories.clear();
        self.filestructs.clear();
        self.attributes.clear();
        let db_path = PathBuf::from(&self.current_path).join("database.db");
        let connection: Option<Connection> = match Connection::open(&db_path) {
            Ok(conn) => Some(conn),
            Err(_) => None,
        };
        for entry in self.path_contents.iter().enumerate() {
            let path = entry.1.clone();
            if path.is_dir() {
                self.directories.push(path);
            } else if path.extension().is_some() && path.extension().unwrap() == "md" {
                if let Some(conn) = &connection {
                    let file_name = path.file_name().unwrap().to_str().unwrap();
                    let mut file_data: Vec<String> = vec![file_name.to_string().clone()];
                    let query = "SELECT attribute_value FROM FileAttributes WHERE file_name = ?";
                    let mut stmt = conn.prepare(query).expect("Failed to prepare query");
                    let mut rows = stmt
                        .query(params![file_name])
                        .expect("Failed to execute query");
                    while let Some(row) = rows.next().expect("Failed to fetch row") {
                        let attribute_value: String =
                            row.get(0).expect("Failed to get attribute value");
                        file_data.push(attribute_value);
                    }
                    self.filestructs.push(file_data);
                } else {
                    self.filestructs
                        .push(vec![path.to_string_lossy().to_string()]);
                }
            }
            if let Ok(data) = fs::read_to_string(self.current_path.clone().join("attributes.json")) {
                let parsed: IndexMap<String, Value> = serde_json::from_str(&data).expect("Failed to parse JSON");
                let result: Vec<(String, String)> = parsed
                    .into_iter()
                    .map(|(key, value)| (key, value.to_string()))
                    .collect();
                self.attributes = result.clone();
            }
        }
    }

    fn back_dir(&mut self) {
        if let Some(parent) = self.current_path.parent() {
            self.current_path = parent.to_path_buf();
            self.refresh_paths();
        } else {
            self.err = Some("Cannot go up from the current directory".to_string());
        }
    }

    fn enter_dir(&mut self, dir_id: usize) {
        if let Some(path) = self.directories.get(dir_id) {
            if path.is_dir() {
                self.current_path = path.to_path_buf();
                self.refresh_paths();
            }
        }
    }

    fn goto(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.current_path = path;
            self.refresh_paths();
        }
    }

    fn current(&self) -> String {
        self.current_path.display().to_string()
    }

    fn clear_err(&mut self) {
        self.err = None;
    }
}

async fn marktext(filename: String) {
    Command::new("/apps/marktext")
        .arg(filename)
        .output()
        .await
        .expect("Failed to start marktext");
}

#[component]
pub fn FileExplorer() -> Element {
    let mut files = use_signal(Files::new);
    let current_path = files.read().current_path.to_owned();
    let mut doc_dir = DOC_DIR.clone();
    doc_dir.pop();
    let mut accumulated_path = doc_dir.clone();
    let relative_path = current_path.strip_prefix(doc_dir).unwrap();
    let breadcrumbs: Vec<(PathBuf, String)> = relative_path
        .components()
        .map(|component| {
            accumulated_path.push(component);
            (accumulated_path.clone(), component.as_os_str().to_string_lossy().into_owned())
        })
        .collect();
    rsx! {
        document::Link { rel: "stylesheet", href: asset!("/assets/main.css") }
        div {
            for (i, (path, name)) in breadcrumbs.clone().into_iter().enumerate() {
                button {
                    onclick: move |_| files.write().goto(path.clone()),
                    "{name}"
                }
                if i < breadcrumbs.len() - 1 {
                    span { " > " }
                }
            }
            br {}
            for (index, file_path) in files.read().directories.clone().into_iter().enumerate() {
                button {
                    onclick: move |_| files.write().enter_dir(index),
                    if let Some(dir_name) = file_path.file_name().expect("File name cannot be unwrapped").to_str() {
                        "{dir_name}"
                    }
                    span {
                        "> "
                    }
                }
            }
            br {}
            table {
                thead {
                    tr {
                        if !files.read().attributes.is_empty() {
                            th { "" }
                        }
                        for attribute_name in files.read().attributes.clone().into_iter() {
                            th {
                                "{attribute_name.0}"
                            }
                        }
                    }
                }
                tbody {
                    for (_index, file_data) in files.read().filestructs.clone().into_iter().enumerate() {
                        tr {
                            td {
                                button {
                                    onclick: move |_| {
                                        let filepath = {
                                            let mut current_path: PathBuf = files.read().current_path.clone();
                                            current_path.push(file_data.get(0).unwrap().clone());
                                            let path: String = current_path.to_string_lossy().to_string().clone();
                                            path
                                        };
                                        let _ = tokio::spawn(async move {
                                            marktext(filepath).await;
                                        });
                                    },
                                    "{file_data.get(0).unwrap()}"
                                }
                            }
                            for attribute in file_data.iter().skip(1) {
                                td {
                                    "{attribute}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
