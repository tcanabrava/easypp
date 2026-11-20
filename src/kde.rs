//! KDE's installed library metadata. Read literal imported-target properties;
//! never execute a project's CMake files or invoke CMake to build it.
use crate::system::Package;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub fn is_native(name: &str) -> bool {
    name.starts_with("KF6::") || name.starts_with("KDEGames6")
}

#[derive(Default, Debug)]
struct Target {
    includes: Vec<String>,
    links: Vec<String>,
    definitions: Vec<String>,
    location: Option<String>,
}

// Installed exports use one quoted, semicolon-separated property per line.
// This deliberately does not interpret the CMake language or generator expressions.
fn properties(text: &str, prefix: &Path, targets: &mut BTreeMap<String, Target>) {
    for block in text.split("set_target_properties(").skip(1) {
        let Some((name, body)) = block.split_once(" PROPERTIES") else {
            continue;
        };
        let target = targets.entry(name.trim().to_string()).or_default();
        for line in body.lines().take_while(|line| line.trim() != ")") {
            let Some((key, value)) = line.trim().split_once(' ') else {
                continue;
            };
            let value = value
                .trim()
                .trim_matches('"')
                .replace("${_IMPORT_PREFIX}", &prefix.to_string_lossy());
            let values = || {
                value
                    .split(';')
                    .filter(|v| !v.is_empty())
                    .map(String::from)
                    .collect()
            };
            match key {
                "INTERFACE_INCLUDE_DIRECTORIES" => target.includes = values(),
                "INTERFACE_LINK_LIBRARIES" => target.links = values(),
                "INTERFACE_COMPILE_DEFINITIONS" => target.definitions = values(),
                k if k.starts_with("IMPORTED_LOCATION_") => target.location = Some(value),
                _ => {}
            }
        }
    }
}

/// Add installed KDE targets to the same header index used for pkg-config.
/// Return metadata paths to include in the discovery-cache fingerprint.
pub fn packages(lib_dirs: &[PathBuf], packages: &mut Vec<Package>) -> Vec<PathBuf> {
    let mut targets = BTreeMap::new();
    let mut tracked = Vec::new();
    let mut visited = HashSet::new();
    for lib in lib_dirs {
        let lib = fs::canonicalize(lib).unwrap_or_else(|_| lib.clone());
        if !visited.insert(lib.clone()) {
            continue;
        }
        let cmake = lib.join("cmake");
        tracked.push(cmake.clone());
        let Ok(dirs) = fs::read_dir(&cmake) else {
            continue;
        };
        for dir in dirs.flatten().map(|e| e.path()) {
            let name = dir.file_name().unwrap_or_default().to_string_lossy();
            if !(name.starts_with("KF6") || name.starts_with("KDEGames6")) {
                continue;
            }
            tracked.push(dir.clone());
            let Ok(files) = fs::read_dir(&dir) else {
                continue;
            };
            let mut files: Vec<_> = files
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().contains("Targets"))
                        && p.extension().is_some_and(|e| e == "cmake")
                })
                .collect();
            files.sort();
            // /prefix/lib[/triplet]/cmake/Package: find the install prefix.
            let mut prefix = lib.as_path();
            while prefix
                .file_name()
                .is_some_and(|n| n != "lib" && n != "lib64")
            {
                let Some(parent) = prefix.parent() else { break };
                prefix = parent;
            }
            let prefix = prefix.parent().unwrap_or(Path::new("/usr"));
            for file in files {
                tracked.push(file.clone());
                if let Ok(text) = fs::read_to_string(file) {
                    properties(&text, prefix, &mut targets);
                }
            }
        }
    }
    for (name, target) in &targets {
        if !is_native(name) || target.includes.is_empty() {
            continue;
        }
        let mut cflags = Vec::new();
        let mut libs = Vec::new();
        if !collect(
            name,
            &targets,
            packages,
            &mut HashSet::new(),
            &mut cflags,
            &mut libs,
        ) {
            continue;
        }
        packages.push(Package {
            name: name.clone(),
            include_dirs: target.includes.iter().map(PathBuf::from).collect(),
            cflags,
            libs,
        });
    }
    tracked
}

fn collect(
    name: &str,
    targets: &BTreeMap<String, Target>,
    packages: &[Package],
    seen: &mut HashSet<String>,
    cflags: &mut Vec<String>,
    libs: &mut Vec<String>,
) -> bool {
    if !seen.insert(name.to_string()) {
        return true;
    }
    if let Some(t) = targets.get(name) {
        if t.includes
            .iter()
            .chain(&t.links)
            .chain(&t.definitions)
            .any(|v| v.contains('$'))
        {
            return false;
        }
        cflags.extend(t.includes.iter().map(|p| format!("-I{p}")));
        cflags.extend(t.definitions.iter().map(|p| format!("-D{p}")));
        if let Some(location) = &t.location {
            if location.contains('$') || !Path::new(location).is_file() {
                return false;
            }
            libs.push(location.clone());
            if let Some(dir) = Path::new(location).parent() {
                libs.push(format!("-Wl,-rpath,{}", dir.display()));
            }
        }
        for dep in &t.links {
            if !collect(dep, targets, packages, seen, cflags, libs) {
                return false;
            }
        }
        return true;
    }
    // Qt and non-KDE dependencies are resolved through their installed .pc metadata.
    let pkg = name.replace("::", "");
    let Some(p) = packages.iter().find(|p| {
        p.name.eq_ignore_ascii_case(&pkg)
            || name
                .split_once("::")
                .is_some_and(|(_, n)| p.name.eq_ignore_ascii_case(n))
    }) else {
        if name.starts_with('-') || Path::new(name).is_absolute() {
            libs.push(name.into());
            return true;
        }
        return false;
    };
    let run = |flag: &str| {
        crate::system::pkg_config_command()
            .args([flag, &p.name])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| crate::system::split_flags(&String::from_utf8_lossy(&o.stdout)))
    };
    cflags.extend(run("--cflags").unwrap_or_else(|| p.cflags.clone()));
    libs.extend(run("--libs").unwrap_or_else(|| p.libs.clone()));
    true
}

pub fn config_compiler() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for prefix in ["/usr", "/usr/local"] {
        for sub in ["lib/kf6", "lib64/kf6", "libexec/kf6", "bin"] {
            dirs.push(Path::new(prefix).join(sub));
        }
    }
    dirs.iter()
        .flat_map(|d| ["kconfig_compiler_kf6", "kconfig_compiler6"].map(|n| d.join(n)))
        .find(|p| p.is_file())
}

/// Locate the owning installed package for CMake export (e.g. ConfigGui -> KF6Config).
pub fn package_name(target: &str) -> Option<String> {
    for root in ["/usr/lib/cmake", "/usr/lib64/cmake", "/usr/local/lib/cmake"] {
        let Ok(dirs) = fs::read_dir(root) else {
            continue;
        };
        for dir in dirs.flatten().map(|e| e.path()) {
            let Ok(files) = fs::read_dir(&dir) else {
                continue;
            };
            for file in files.flatten().map(|e| e.path()) {
                if !file
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with("Targets.cmake"))
                {
                    continue;
                }
                if fs::read_to_string(file)
                    .is_ok_and(|s| s.contains(&format!("add_library({target} ")))
                {
                    return dir.file_name().map(|n| n.to_string_lossy().into_owned());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_split_installed_exports_and_indented_closing_parenthesis() {
        let mut targets = BTreeMap::new();
        properties(
            r#"set_target_properties(KF6::ConfigGui PROPERTIES
  IMPORTED_LOCATION_NOCONFIG "${_IMPORT_PREFIX}/lib/libKF6ConfigGui.so.6"
  )
"#,
            Path::new("/opt/kde"),
            &mut targets,
        );
        properties(
            r#"set_target_properties(KF6::ConfigGui PROPERTIES
  INTERFACE_INCLUDE_DIRECTORIES "${_IMPORT_PREFIX}/include/KF6/KConfig;${_IMPORT_PREFIX}/include/KF6/KConfigGui"
  INTERFACE_LINK_LIBRARIES "Qt6::Gui;KF6::ConfigCore"
)
"#,
            Path::new("/opt/kde"),
            &mut targets,
        );
        let t = &targets["KF6::ConfigGui"];
        assert_eq!(
            t.location.as_deref(),
            Some("/opt/kde/lib/libKF6ConfigGui.so.6")
        );
        assert_eq!(
            t.includes,
            [
                "/opt/kde/include/KF6/KConfig",
                "/opt/kde/include/KF6/KConfigGui"
            ]
        );
        assert_eq!(t.links, ["Qt6::Gui", "KF6::ConfigCore"]);
    }

    #[test]
    fn unsupported_expressions_and_missing_dependencies_are_not_silently_dropped() {
        let mut targets = BTreeMap::new();
        targets.insert(
            "KF6::Test".into(),
            Target {
                links: vec!["Missing::Library".into()],
                ..Target::default()
            },
        );
        assert!(!collect(
            "KF6::Test",
            &targets,
            &[],
            &mut HashSet::new(),
            &mut vec![],
            &mut vec![]
        ));
        targets.get_mut("KF6::Test").unwrap().links = vec!["$<LINK_ONLY:other>".into()];
        assert!(!collect(
            "KF6::Test",
            &targets,
            &[],
            &mut HashSet::new(),
            &mut vec![],
            &mut vec![]
        ));
    }
}
