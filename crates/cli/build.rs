use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Same as your collect_scenario_files: go up twice to project root,
    // then into "scenarios".
    let project_root = manifest_dir.parent().unwrap().parent().unwrap();
    let scenarios_dir = project_root.join("scenarios");

    let mut files = Vec::new();
    let mut dirs = Vec::new();
    collect_files(&scenarios_dir, &mut files, &mut dirs);

    // Tell Cargo to rerun build.rs if scenarios/ directory or any subdirectory changes
    // (this detects new/deleted files)
    for dir in &dirs {
        println!("cargo:rerun-if-changed={}", dir.display());
    }
    // Also watch each individual file (detects modifications)
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let mut out = String::from("scenario_tests! {\n");

    for path in files {
        // Store paths relative to project_root so runtime resolution is identical
        // to your current helper.
        let rel = path.strip_prefix(project_root).unwrap();
        let rel_str = rel.to_string_lossy();

        // Turn "scenarios/foo/bar.toml" into a legal test name
        let test_name: String = rel_str
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();

        out.push_str(&format!("    {} => {:?},\n", test_name, rel_str));
    }

    out.push_str("}\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("generated_scenario_tests.rs"), out).unwrap();
}

fn collect_files(dir: &Path, acc: &mut Vec<PathBuf>, dirs: &mut Vec<PathBuf>) {
    dirs.push(dir.to_path_buf());
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, acc, dirs);
        } else {
            acc.push(path);
        }
    }
}
