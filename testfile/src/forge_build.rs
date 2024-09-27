use std::io::{self, Write};
use std::process::Command;

// Ideally, we'd import forge as a library and run it directly in Rust, but it
// has conflicts with this project's dependencies that I have not figured out
// how to resolve.

/// Run `forge build` in the given project directory.
pub fn run_forge_build(project_path: &str) -> io::Result<()> {
    let output = Command::new("forge")
        .arg("build")
        .current_dir(project_path)
        .output()?;

    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forge_build() {
        std::fs::remove_dir_all("./contracts/out").unwrap_or_else(|error| {
            eprintln!("Failed to remove contract artifacts: {}", error);
        });
        run_forge_build("./contracts").unwrap();
        assert!(std::path::Path::new("./contracts/out").exists());
    }
}
