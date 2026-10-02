//! Embed the previously built Leptos distribution; Node is never a runtime dependency.
use sha2::{Digest, Sha256};
use std::{env, fs, io, path::Path};

fn collect(directory: &Path, root: &Path, output: &mut Vec<(String, String)>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::other(
                "UI assets must not contain symbolic links",
            ));
        }
        if kind.is_dir() {
            collect(&path, root, output)?;
        } else if kind.is_file() {
            let relative = path.strip_prefix(root).map_err(io::Error::other)?;
            let name = format!("/{}", relative.to_string_lossy().replace('\\', "/"));
            let canonical = path.canonicalize()?;
            let filename = canonical
                .to_str()
                .ok_or_else(|| io::Error::other("UI asset path is not UTF-8"))?;
            output.push((name, filename.to_owned()));
        } else {
            return Err(io::Error::other("UI assets must be ordinary files"));
        }
    }
    Ok(())
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = env::var("CARGO_MANIFEST_DIR")?;
    let root = Path::new(&manifest).join("../../dist/ui");
    println!("cargo:rerun-if-changed={}", root.display());
    if !root.join("index.html").is_file() {
        return Err(
            "Missing dist/ui/index.html; run `python3 scripts/build_ui.py` before Cargo".into(),
        );
    }
    for name in [
        "bootstrap.js",
        "usage_lens_ui.js",
        "usage_lens_ui_bg.wasm",
        "frontend-build.json",
    ] {
        if !root.join(name).is_file() {
            return Err(
                format!("Missing Leptos asset {name}; run python3 scripts/build_ui.py").into(),
            );
        }
    }
    if !fs::read(root.join("usage_lens_ui_bg.wasm"))?.starts_with(b"\0asm") {
        return Err("Embedded frontend is not WebAssembly".into());
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("frontend-build.json"))?)?;
    if manifest["schemaVersion"] != 1
        || manifest["framework"] != "leptos"
        || manifest["frameworkVersion"] != "0.8.21"
        || manifest["wasmBindgenVersion"] != "0.2.129"
    {
        return Err("Invalid or stale Leptos build identity".into());
    }
    let hashes = manifest["assets"]
        .as_object()
        .ok_or("Missing frontend asset hashes")?;
    let mut assets = Vec::new();
    collect(&root, &root, &mut assets)?;
    assets.sort();
    if hashes.len() + 1 != assets.len() {
        return Err("Frontend asset manifest does not match its directory".into());
    }
    for (name, filename) in &assets {
        if name == "/frontend-build.json" {
            continue;
        }
        let digest: String = Sha256::digest(fs::read(filename)?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if hashes.get(&name[1..]).and_then(serde_json::Value::as_str) != Some(digest.as_str()) {
            return Err(format!("Stale or modified frontend asset: {name}").into());
        }
    }
    let mut source = String::from("&[\n");
    for (name, filename) in assets {
        source.push_str(&format!(
            "({name:?}, {:?}, include_bytes!({filename:?}) as &'static [u8]),\n",
            mime(&name)
        ));
    }
    source.push_str("]\n");
    fs::write(
        Path::new(&env::var("OUT_DIR")?).join("embedded_assets.rs"),
        source,
    )?;
    Ok(())
}
