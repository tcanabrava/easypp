//! Installs build artifacts and shared data into a local prefix.
use crate::{builder, project::Project, ui};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const LAUNCHER: &str = include_str!("launch.sh");

#[derive(Clone, Copy)]
enum Payload<'a> {
    File(&'a Path),
    Launcher,
}

pub fn run(project: &Project, opts: &builder::Options) -> Result<(), String> {
    let programs: Vec<_> = project.bins.iter().collect();
    builder::build(project, opts, &programs)?;

    let mut files = BTreeMap::new();
    for program in programs {
        files.insert(
            Path::new("bin").join(&program.name),
            builder::exe_path(opts, program),
        );
    }
    for name in project.libs.keys() {
        let path = builder::library_path(opts, name);
        if path.is_file() {
            files.insert(Path::new("lib").join(path.file_name().unwrap()), path);
        }
    }
    let data = Path::new("data");
    if fs::symlink_metadata(data).is_ok_and(|metadata| !metadata.is_dir()) {
        return Err("data/ must be a directory".into());
    }
    walk(data, &mut |path| {
        let relative = path.strip_prefix("data").unwrap();
        let dest = data_destination(relative);
        if let Some(other) = files.insert(dest.clone(), path.to_path_buf()) {
            return Err(format!(
                "{} and {} would both install as {}",
                other.display(),
                path.display(),
                dest.display()
            ));
        }
        Ok(())
    })?;
    if files.is_empty() {
        return Err("there are no programs, libraries, or data files to install".into());
    }

    let system = Path::new("/usr/local");
    let payloads = || {
        files
            .iter()
            .map(|(dest, src)| (dest.as_path(), Payload::File(src)))
    };
    let prefix = match stage(system, payloads()) {
        Ok(staged) => {
            commit(staged)?;
            system.to_path_buf()
        }
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
            ) =>
        {
            ui::warn(&format!("{e}; installing in ~/easypp"));
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .ok_or("set HOME to an absolute path to install in ~/easypp")?;
            let prefix = home.join("easypp");
            let staged = stage(
                &prefix,
                payloads().chain([(Path::new("launch.sh"), Payload::Launcher)]),
            )
            .map_err(|e| e.to_string())?;
            commit(staged)?;
            if !project.bins.is_empty() {
                ui::help(
                    "run ~/easypp/launch.sh (pass a program name if you have installed several)",
                );
            }
            prefix
        }
        Err(e) => return Err(e.to_string()),
    };
    ui::status(
        "Installed",
        &format!("{} file(s) to {}", files.len(), prefix.display()),
    );
    Ok(())
}

/// A data/ directory mirrors share/. Recognize conventional top-level desktop
/// metadata and KDE's size-apps-name image filenames as well.
fn data_destination(path: &Path) -> PathBuf {
    let share = Path::new("share");
    if path.components().count() == 1
        && let Some(name) = path.to_str()
    {
        if name.ends_with(".desktop") {
            return share.join("applications").join(path);
        }
        if name.ends_with(".metainfo.xml") || name.ends_with(".appdata.xml") {
            return share.join("metainfo").join(path);
        }
        if let Some((size, icon)) = name.split_once("-apps-")
            && size.parse::<u32>().is_ok_and(|n| n > 0)
            && icon.ends_with(".png")
        {
            return share
                .join("icons/hicolor")
                .join(format!("{size}x{size}/apps"))
                .join(icon);
        }
    }
    share.join(path)
}

fn walk(dir: &Path, visit: &mut impl FnMut(&Path) -> Result<(), String>) -> Result<(), String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound && dir == Path::new("data") => return Ok(()),
        Err(e) => return Err(format!("cannot read {}: {e}", dir.display())),
    };
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("cannot inspect {}: {e}", path.display()))?;
        if kind.is_dir() {
            walk(&path, visit)?;
        } else if kind.is_file() {
            visit(&path)?;
        } else {
            return Err(format!(
                "cannot install {}: expected a regular file",
                path.display()
            ));
        }
    }
    Ok(())
}

struct StagedFile {
    temporary: PathBuf,
    destination: PathBuf,
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.temporary);
    }
}

/// Prepare every file before replacing any installed files. A failed permission
/// check leaves the existing installation intact.
fn stage<'a>(
    prefix: &Path,
    files: impl Iterator<Item = (&'a Path, Payload<'a>)>,
) -> io::Result<Vec<StagedFile>> {
    files
        .map(|(relative, payload)| {
            let destination = prefix.join(relative);
            stage_file(destination, relative.starts_with("bin"), payload)
        })
        .collect()
}

fn stage_file(
    destination: PathBuf,
    executable: bool,
    payload: Payload<'_>,
) -> io::Result<StagedFile> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let parent = destination.parent().unwrap();
    let context = |e: io::Error| {
        io::Error::new(
            e.kind(),
            format!("cannot install {}: {e}", destination.display()),
        )
    };
    fs::create_dir_all(parent).map_err(&context)?;
    match fs::symlink_metadata(&destination) {
        Ok(meta) if meta.is_dir() => {
            return Err(io::Error::other(format!(
                "{} is a directory",
                destination.display()
            )));
        }
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(context(e)),
    }

    let (temporary, mut file) = loop {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".easypp-install-{}-{serial}", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => break (path, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(context(e)),
        }
    };
    let staged = StagedFile {
        temporary,
        destination: destination.clone(),
    };
    match payload {
        Payload::File(src) => {
            let mut source = File::open(src)
                .map_err(|e| io::Error::other(format!("cannot read {}: {e}", src.display())))?;
            io::copy(&mut source, &mut file).map_err(&context)?;
        }
        Payload::Launcher => file.write_all(LAUNCHER.as_bytes()).map_err(&context)?,
    }
    let mode = if executable || matches!(payload, Payload::Launcher) {
        0o755
    } else {
        0o644
    };
    file.set_permissions(fs::Permissions::from_mode(mode))
        .map_err(&context)?;
    Ok(staged)
}

fn commit(mut staged: Vec<StagedFile>) -> Result<(), String> {
    // Publish launchers and binaries after the libraries and data they use.
    staged.sort_by_key(|file| {
        if file.destination.ends_with("launch.sh") {
            2
        } else {
            usize::from(
                file.destination
                    .parent()
                    .is_some_and(|p| p.ends_with("bin")),
            )
        }
    });
    for file in staged {
        fs::rename(&file.temporary, &file.destination)
            .map_err(|e| format!("cannot replace {}: {e}", file.destination.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn installs_launchable_files_and_xdg_data() {
        let dir = std::env::temp_dir().join(format!("easypp-install-test-{}", std::process::id()));
        let prefix = dir.join("home with spaces/easypp");
        fs::create_dir_all(&dir).unwrap();
        let program = dir.join("program");
        fs::write(
            &program,
            "#!/bin/sh\nprintf '%s\\n' \"$1\"\nIFS=: read -r share rest <<EOF\n$XDG_DATA_DIRS\nEOF\ncat \"$share/demo/asset\"\n",
        ).unwrap();
        let asset = dir.join("asset");
        fs::write(&asset, "installed data\n").unwrap();
        let files = [
            (Path::new("bin/demo"), Payload::File(&program)),
            (Path::new("share/demo/asset"), Payload::File(&asset)),
            (Path::new("launch.sh"), Payload::Launcher),
        ];
        commit(stage(&prefix, files.into_iter()).unwrap()).unwrap();
        let output = Command::new(prefix.join("launch.sh"))
            .args(["demo", "argument with spaces"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(output.stdout, b"argument with spaces\ninstalled data\n");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn maps_desktop_assets_into_xdg_directories() {
        for (input, output) in [
            ("demo.desktop", "share/applications/demo.desktop"),
            ("demo.appdata.xml", "share/metainfo/demo.appdata.xml"),
            (
                "32-apps-demo.png",
                "share/icons/hicolor/32x32/apps/demo.png",
            ),
            (
                "demo/themes/classic.desktop",
                "share/demo/themes/classic.desktop",
            ),
        ] {
            assert_eq!(data_destination(Path::new(input)), Path::new(output));
        }
    }
}
