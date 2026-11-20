//! Conan installs dependencies explicitly. Discovery reads the generated .pc files.
use crate::builder::{BUILD_DIR, Options};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

static PACKAGE_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn configure(root: &Path, opts: &Options) {
    let _ = PACKAGE_DIR.set(root.join(BUILD_DIR).join("conan").join(opts.profile()));
}

pub fn package_dir() -> Option<&'static Path> {
    PACKAGE_DIR.get().map(PathBuf::as_path)
}

pub fn provides(name: &str) -> bool {
    package_dir().is_some_and(|dir| dir.join(format!("{name}.pc")).is_file())
}

pub fn metadata_files() -> Vec<PathBuf> {
    let mut files: Vec<_> = package_dir()
        .and_then(|dir| fs::read_dir(dir).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "pc"))
        .collect();
    files.sort();
    files
}

const MANIFEST: &str = "conanfile.txt";

fn needs_install(root: &Path) -> bool {
    let manifest = root.join(MANIFEST);
    if !manifest.is_file() {
        return false;
    }
    let installed = package_dir().unwrap().join(MANIFEST);
    let stale = installed.is_file() && fs::read(&manifest).ok() != fs::read(&installed).ok();
    stale || metadata_files().is_empty()
}

pub fn ensure_installed(root: &Path, opts: &Options) -> Result<(), String> {
    if needs_install(root) {
        crate::ui::status(
            "Installing",
            &format!("Conan dependencies for {}", opts.profile()),
        );
        install(&[], &["--build=missing".into()], opts)?;
    }
    Ok(())
}

pub fn require_installed(root: &Path, opts: &Options) -> Result<(), String> {
    if needs_install(root) {
        let release = if opts.release { " --release" } else { "" };
        return Err(format!(
            "Conan metadata is missing or out of date. Run `easypp deps install{release}`."
        ));
    }
    Ok(())
}

pub fn install(requested: &[String], flags: &[String], opts: &Options) -> Result<(), String> {
    if requested.iter().any(|r| {
        r.split_once('/')
            .is_none_or(|(name, version)| name.is_empty() || version.is_empty())
            || r.starts_with('-')
            || r.chars().any(char::is_whitespace)
    }) {
        return Err("usage: easypp deps add <name/version>... [--release]".into());
    }
    if flags.iter().any(|flag| flag != "--build=missing") {
        return Err("supported Conan option after --: --build=missing".into());
    }
    if !crate::system::in_path("pkg-config") {
        return Err("Conan dependencies require pkg-config".into());
    }

    let dir = package_dir().expect("configured project root");
    let parent = dir.parent().unwrap();
    fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    // Both profiles share the manifest, so serialize dependency operations.
    let staging = parent.join(".install");
    fs::create_dir(&staging).map_err(|e| format!("cannot create {}: {e}", staging.display()))?;
    let result = install_into(dir, &staging, requested, flags, opts);
    // Keep the backup if rollback itself failed.
    if result.is_ok() || !staging.join("previous").exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// Find or install an isolated user-local Conan without changing PATH.
fn conan_program() -> Result<PathBuf, String> {
    if crate::system::in_path("conan") {
        return Ok("conan".into());
    }

    let (tool, bin_args, install_args): (&str, &[_], &[_]) = if crate::system::in_path("pipx") {
        (
            "pipx",
            &["environment", "--value", "PIPX_BIN_DIR"],
            &["install"],
        )
    } else if crate::system::in_path("uv") {
        ("uv", &["tool", "dir", "--bin"], &["tool", "install"])
    } else {
        return Err("Conan 2 is required. Install pipx or uv so easypp can set up Conan, or put Conan 2 on PATH.".into());
    };

    let bin = Command::new(tool)
        .args(bin_args)
        .output()
        .map_err(|e| format!("cannot find the {tool} executable directory: {e}"))?;
    if !bin.status.success() {
        return Err(format!(
            "cannot find the {tool} executable directory: {}",
            String::from_utf8_lossy(&bin.stderr).trim()
        ));
    }
    let path = Path::new(String::from_utf8_lossy(&bin.stdout).trim()).join("conan");
    if !path.is_file() {
        crate::ui::status("Installing", &format!("Conan 2 with {tool}"));
        let status = Command::new(tool)
            .args(install_args)
            .arg("conan>=2,<3")
            .status()
            .map_err(|e| format!("cannot run {tool}: {e}"))?;
        if !status.success() || !path.is_file() {
            return Err(format!(
                "{tool} could not install Conan 2 at {} ({status})",
                path.display()
            ));
        }
    }
    Ok(path)
}

/// Use the configured default profile when available. Bootstrap a project-local
/// Clang profile on a fresh Conan installation.
fn fallback_profile(conan: &Path, staging: &Path) -> Result<Option<PathBuf>, String> {
    let default = Command::new(conan)
        .args(["profile", "path", "default"])
        .output()
        .map_err(|e| format!("cannot run Conan 2: {e}"))?;
    if default.status.success() {
        return Ok(None);
    }

    let profile = staging.join("easypp.profile");
    crate::ui::status("Detecting", "a Conan profile for Clang");
    let status = Command::new(conan)
        .args(["profile", "detect", "--name"])
        .arg(&profile)
        .env("CC", "clang")
        .env("CXX", "clang++")
        .status()
        .map_err(|e| format!("cannot detect the Conan profile: {e}"))?;
    if !status.success() {
        return Err(format!("cannot detect the Conan profile ({status})"));
    }
    Ok(Some(profile))
}

fn install_into(
    dir: &Path,
    staging: &Path,
    requested: &[String],
    flags: &[String],
    opts: &Options,
) -> Result<(), String> {
    let project_recipe = Path::new(MANIFEST);
    let previous = match fs::read_to_string(project_recipe) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !requested.is_empty() => {
            // Import the selected profile's dependency list from older easypp builds.
            let legacy = dir.join("requirements.txt");
            match fs::read_to_string(&legacy) {
                Ok(text) => format!("[requires]\n{text}\n[generators]\nPkgConfigDeps\n"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    "[generators]\nPkgConfigDeps\n".into()
                }
                Err(e) => return Err(format!("cannot read {}: {e}", legacy.display())),
            }
        }
        Err(e) => return Err(format!("cannot read {MANIFEST}: {e}")),
    };

    let candidate = staging.join(MANIFEST);
    let recipe = if requested.is_empty() {
        previous
    } else {
        update_requires(&previous, requested)
    };
    let input = if requested.is_empty() {
        project_recipe
    } else {
        fs::write(&candidate, &recipe).map_err(|e| format!("cannot stage {MANIFEST}: {e}"))?;
        if Path::new("conan.lock").is_file() {
            fs::copy("conan.lock", staging.join("conan.lock"))
                .map_err(|e| format!("cannot stage conan.lock: {e}"))?;
        }
        &candidate
    };

    let output = staging.join("packages");
    let build_type = if opts.release { "Release" } else { "Debug" };
    let conan = conan_program()?;
    let mut command = Command::new(&conan);
    command.arg("install").arg(input);
    if let Some(profile) = fallback_profile(&conan, staging)? {
        command
            .arg("--profile:host")
            .arg(&profile)
            .arg("--profile:build")
            .arg(&profile)
            .args([
                "-s:h",
                "compiler.cppstd=23",
                "-c:a",
                r#"tools.build:compiler_executables={"c":"clang","cpp":"clang++"}"#,
            ])
            .env("CC", "clang")
            .env("CXX", "clang++");
    }
    if !section_lines(&recipe, "generators").any(|line| line.trim() == "PkgConfigDeps") {
        command.args(["-g", "PkgConfigDeps"]);
    }
    let status = command
        .args([
            "-s:h",
            &format!("build_type={build_type}"),
            "--output-folder",
        ])
        .arg(&output)
        .args(flags)
        .status()
        .map_err(|e| format!("cannot run Conan 2: {e}"))?;
    if !status.success() {
        return Err(format!("conan install failed ({status})"));
    }
    fs::write(output.join(MANIFEST), &recipe)
        .map_err(|e| format!("cannot record installed Conan manifest: {e}"))?;

    let old = staging.join("previous");
    let replacing = dir.exists();
    if replacing {
        fs::rename(dir, &old).map_err(|e| format!("cannot replace Conan metadata: {e}"))?;
    }
    if let Err(e) = fs::rename(&output, dir) {
        return Err(restore_metadata(dir, &old, replacing, e));
    }
    if !requested.is_empty()
        && let Err(e) = fs::rename(candidate, project_recipe)
    {
        if let Err(rollback) = fs::rename(dir, &output) {
            return Err(format!(
                "cannot write {MANIFEST}: {e}. Cannot move new metadata aside: {rollback}"
            ));
        }
        return Err(restore_metadata(dir, &old, replacing, e));
    }
    crate::ui::status(
        "Installed",
        &format!("Conan packages for {}", opts.profile()),
    );
    Ok(())
}

fn restore_metadata(dir: &Path, old: &Path, replacing: bool, error: std::io::Error) -> String {
    if replacing && let Err(e) = fs::rename(old, dir) {
        return format!(
            "cannot publish Conan changes: {error}. Previous metadata is at {}: {e}",
            old.display()
        );
    }
    format!("cannot publish Conan changes: {error}")
}

fn section_lines<'a>(text: &'a str, section: &str) -> impl Iterator<Item = &'a str> {
    let header = format!("[{section}]");
    text.lines()
        .skip_while(move |line| line.trim() != header)
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
}

fn update_requires(text: &str, requested: &[String]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let start = if let Some(index) = lines.iter().position(|line| line.trim() == "[requires]") {
        index + 1
    } else {
        lines.splice(0..0, ["[requires]".to_string(), String::new()]);
        1
    };
    for reference in requested {
        let name = reference.split_once('/').unwrap().0;
        let section = lines[start..]
            .iter()
            .take_while(|line| !line.trim_start().starts_with('['));
        if let Some(index) = section.enumerate().find_map(|(i, line)| {
            let line = line.trim();
            (!line.starts_with('#') && line.split_once('/').is_some_and(|(n, _)| n == name))
                .then_some(start + i)
        }) {
            lines[index] = reference.clone();
        } else {
            let count = lines[start..]
                .iter()
                .take_while(|line| !line.trim_start().starts_with('['))
                .count();
            let end = start + count;
            let insert = lines[start..end]
                .iter()
                .rposition(|line| !line.trim().is_empty())
                .map_or(start, |index| start + index + 1);
            lines.insert(insert, reference.clone());
        }
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::update_requires;

    #[test]
    fn updates_only_direct_requirements() {
        let before = "[requires]\n# compression\nzlib/1.3\n\n[tool_requires]\nzlib/1.2\n\n[options]\nzlib/*:shared=True\n";
        let after = update_requires(before, &["zlib/1.3.1".into(), "gtest/1.17.0".into()]);
        assert_eq!(
            after,
            "[requires]\n# compression\nzlib/1.3.1\ngtest/1.17.0\n\n[tool_requires]\nzlib/1.2\n\n[options]\nzlib/*:shared=True\n"
        );
    }
}
