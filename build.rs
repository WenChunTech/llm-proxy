use std::{env, path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/bun.lock");

    if env::var_os("LLM_PROXY_SKIP_FRONTEND_BUILD").is_some() {
        return;
    }

    let frontend = Path::new("frontend");
    let status = Command::new("bun")
        .arg("run")
        .arg("build")
        .current_dir(frontend)
        .status()
        .expect("failed to run frontend build command");

    if !status.success() {
        panic!("frontend build failed");
    }
}
