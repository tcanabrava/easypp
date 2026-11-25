//! The ARM64 Linux toolchain and its QEMU user-mode runner.
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const TRIPLE: &str = "aarch64-linux-gnu";
pub const DEFAULT_SYSROOT: &str = "/usr/aarch64-linux-gnu";
const LIB_DIRS: &[&str] = &[
    "lib/aarch64-linux-gnu",
    "usr/lib/aarch64-linux-gnu",
    "lib64",
    "usr/lib64",
    "lib",
    "usr/lib",
];

#[derive(Debug)]
pub struct Target {
    pub sysroot: PathBuf,
}

impl Target {
    pub fn new(path: Option<PathBuf>) -> Result<Self, String> {
        let requested = path.unwrap_or_else(|| DEFAULT_SYSROOT.into());
        let sysroot = requested.canonicalize().ok().filter(|path| path.is_dir())
            .ok_or_else(|| format!("ARM64 sysroot {} is unavailable. Install the ARM64 Linux SDK or pass --sysroot DIR.", requested.display()))?;
        if sysroot == Path::new("/") {
            return Err(
                "The ARM64 sysroot must be a separate target directory. Pass --sysroot DIR.".into(),
            );
        }
        Ok(Self { sysroot })
    }

    pub fn flags(&self) -> [String; 2] {
        [
            format!("--target={TRIPLE}"),
            format!("--sysroot={}", self.sysroot.display()),
        ]
    }

    pub fn build_dir(&self) -> PathBuf {
        let mut hash = DefaultHasher::new();
        self.sysroot.hash(&mut hash);
        Path::new(crate::builder::BUILD_DIR)
            .join(TRIPLE)
            .join(format!("{:016x}", hash.finish()))
    }

    pub fn pkg_config_dirs(&self) -> Vec<PathBuf> {
        LIB_DIRS
            .iter()
            .copied()
            .chain(["usr/share", "share"])
            .map(|dir| self.sysroot.join(dir).join("pkgconfig"))
            .collect()
    }

    pub fn link_flags(&self) -> impl Iterator<Item = String> + '_ {
        let search_dirs = LIB_DIRS
            .iter()
            .map(|dir| self.sysroot.join(dir))
            .filter(|dir| dir.is_dir())
            .map(|dir| format!("-L{}", dir.display()));
        self.flags().into_iter().chain(search_dirs)
    }

    pub fn pkg_config(&self, dirs: &[PathBuf]) -> Command {
        let mut command = Command::new("pkg-config");
        command
            .env_remove("PKG_CONFIG_PATH")
            .env(
                "PKG_CONFIG_LIBDIR",
                std::env::join_paths(dirs).unwrap_or_default(),
            )
            .env("PKG_CONFIG_SYSROOT_DIR", &self.sysroot);
        command
    }

    pub fn emulator(&self) -> Result<PathBuf, String> {
        ["qemu-aarch64", "qemu-aarch64-static"].into_iter()
            .find_map(crate::system::find_in_path)
            .ok_or_else(|| "ARM64 programs need qemu-aarch64 on PATH. Install QEMU user-mode tools to run or test this target.".into())
    }
}

pub fn command(target: Option<&Target>, exe: &Path) -> Result<Command, String> {
    match target {
        Some(target) => {
            let mut command = Command::new(target.emulator()?);
            command.arg("-L").arg(&target.sysroot).arg(exe);
            Ok(command)
        }
        None => Ok(Command::new(exe)),
    }
}
