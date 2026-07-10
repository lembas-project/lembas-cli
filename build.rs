use std::process::Command;

fn main() {
    // Re-run if git state changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");

    let version = git_version().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=LEMBAS_CLI_VERSION={}", version);
}

fn git_version() -> Option<String> {
    // Try to get version from git describe (like setuptools-scm)
    // Format: v2026.7.0 -> 2026.7.0
    //         v2026.7.0-5-g1234567 -> 2026.7.0.dev5+g1234567
    let output = Command::new("git")
        .args(["describe", "--tags", "--match", "v*"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let describe = String::from_utf8(output.stdout).ok()?.trim().to_string();

    // Strip leading 'v'
    let describe = describe.strip_prefix('v').unwrap_or(&describe);

    if describe.contains('-') {
        // v2026.7.0-5-g1234567 -> 2026.7.0.dev5+g1234567
        let parts: Vec<&str> = describe.splitn(3, '-').collect();
        if parts.len() == 3 {
            let base = parts[0];
            let commits = parts[1];
            let hash = parts[2];
            Some(format!("{}.dev{}+{}", base, commits, hash))
        } else {
            Some(describe.to_string())
        }
    } else {
        // Exact tag match: v2026.7.0 -> 2026.7.0
        Some(describe.to_string())
    }
}
