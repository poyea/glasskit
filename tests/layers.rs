// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The layers of the crate. A module uses only the modules of its own layer and of the
//! layers below it. Thus a new feature has one place, and a change in a module can break
//! only the modules above it.
//!
//! To add a module, give it a layer here: the lowest layer that is above all modules that
//! it uses.

use std::path::Path;

/// The modules of each layer, from the lowest layer.
const LAYERS: [(&str, &[&str]); 7] = [
    ("base", &["error", "handle"]),
    (
        "system",
        &[
            "autostart",
            "clipboard",
            "edit",
            "icon",
            "native",
            "placement",
            "presence",
            "process",
            "theme",
        ],
    ),
    ("thread", &["render", "ui"]),
    ("loop", &["events", "menu"]),
    ("parts", &["popup", "widget"]),
    ("windows", &["dialog", "layout", "window"]),
    ("composites", &["kanban", "tray"]),
];

fn layer_of(module: &str) -> Option<usize> {
    LAYERS
        .iter()
        .position(|(_, modules)| modules.contains(&module))
}

/// The module of a source file: the first part of its path below `src`.
fn module_of(file: &Path, src: &Path) -> String {
    let first = file.strip_prefix(src).unwrap().components().next().unwrap();
    let name = first.as_os_str().to_string_lossy().into_owned();
    name.trim_end_matches(".rs").to_owned()
}

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The modules that a line of code names as `crate::module`. A comment names no module.
fn used(line: &str) -> Vec<&str> {
    let code = line.split("//").next().unwrap_or("");
    code.split("crate::")
        .skip(1)
        .map(|rest| {
            let end = rest
                .find(|c: char| !(c.is_ascii_lowercase() || c == '_'))
                .unwrap_or(rest.len());
            &rest[..end]
        })
        .filter(|name| !name.is_empty())
        .collect()
}

#[test]
fn a_module_uses_only_its_layer_and_the_layers_below() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut problems = Vec::new();
    for file in files {
        let module = module_of(&file, &src);
        if module == "lib" {
            continue;
        }
        let Some(layer) = layer_of(&module) else {
            problems.push(format!(
                "the module {module} has no layer in tests/layers.rs"
            ));
            continue;
        };
        let text = std::fs::read_to_string(&file).unwrap();
        for (number, line) in text.lines().enumerate() {
            for name in used(line) {
                if layer_of(name).is_some_and(|other| other > layer) {
                    problems.push(format!(
                        "{}:{}: {module} ({}) uses {name} ({})",
                        file.strip_prefix(&src).unwrap().display(),
                        number + 1,
                        LAYERS[layer].0,
                        LAYERS[layer_of(name).unwrap()].0,
                    ));
                }
            }
        }
    }
    problems.sort();
    problems.dedup();
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

#[test]
fn a_comment_names_no_module() {
    assert_eq!(used("use crate::error::{Call, Result};"), ["error"]);
    assert_eq!(used("/// See [`Window`](crate::window::Window)."), [""; 0]);
    assert_eq!(
        used("let a = crate::ui::notify(crate::ui::Notice::Popup(1)); // crate::popup"),
        ["ui", "ui"]
    );
    assert_eq!(used("impl From<crate::Error> for X {}"), [""; 0]);
}
