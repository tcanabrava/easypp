//! CUDA toolkit discovery for native builds.
use crate::project::Lang;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub fn contains_sources<'a>(sources: impl IntoIterator<Item = &'a PathBuf>) -> bool {
    sources
        .into_iter()
        .any(|source| Lang::of(source) == Some(Lang::Cuda))
}

pub const STANDARD: &str = "20";
pub const HOST_WARNINGS: &str = "-Xcompiler=-Wall,-Wextra,-Werror";

#[derive(Debug)]
pub struct Toolkit {
    pub nvcc: PathBuf,
    pub include: PathBuf,
    pub lib: PathBuf,
    architecture: OnceLock<String>,
}

impl Toolkit {
    pub fn discover() -> Result<Self, String> {
        let nvcc = crate::system::find_in_path("nvcc")
            .ok_or("CUDA sources require nvcc from the NVIDIA CUDA Toolkit on PATH. Install the toolkit or add its bin directory to PATH.")?;
        let root = nvcc
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| format!("cannot locate the CUDA Toolkit for {}", nvcc.display()))?;
        let target = root.join(format!("targets/{}-linux", std::env::consts::ARCH));
        let include = [root.join("include"), target.join("include")]
            .into_iter()
            .find(|dir| dir.join("cuda_runtime.h").is_file())
            .ok_or_else(|| toolkit_missing(&nvcc, root, "cuda_runtime.h"))?;
        let lib = [
            root.join("lib64"),
            root.join("lib"),
            target.join("lib"),
            root.join(format!("lib/{}-linux-gnu", std::env::consts::ARCH)),
        ]
        .into_iter()
        .find(|dir| dir.join("libcudart.so").is_file())
        .ok_or_else(|| toolkit_missing(&nvcc, root, "libcudart.so"))?;
        Ok(Self {
            nvcc,
            include,
            lib,
            architecture: OnceLock::new(),
        })
    }

    pub fn restore(&self, cache: &crate::cache::Cache) -> Result<(), String> {
        if let Some(value) = cache.cuda_architecture()?
            && (value == "default"
                || value.strip_prefix("sm_").is_some_and(|code| {
                    code.parse::<u16>().is_ok() && code.bytes().all(|byte| byte.is_ascii_digit())
                }))
        {
            let _ = self.architecture.set(value);
        }
        Ok(())
    }

    pub fn architecture(&self) -> Option<&str> {
        self.architecture.get().map(String::as_str)
    }

    pub fn architecture_flag(&self) -> Option<String> {
        self.architecture()
            .filter(|arch| *arch != "default")
            .map(|arch| format!("-arch={arch}"))
    }

    pub fn device_runtime(&self) -> Result<PathBuf, String> {
        let archive = self.lib.join("libcudadevrt.a");
        if !archive.is_file() {
            return Err(format!(
                "CUDA device linking needs {}. Install the CUDA development files that match {}.",
                archive.display(),
                self.nvcc.display()
            ));
        }
        Ok(archive)
    }

    pub fn configure(&self) -> Result<(), String> {
        if self.architecture.get().is_some() {
            return Ok(());
        }
        let cache = crate::cache::Cache::open(Path::new(crate::builder::BUILD_DIR))
            .map_err(|e| format!("cannot open the project database for CUDA configuration: {e}"))?;
        self.restore(&cache)?;
        if self.architecture.get().is_some() {
            return Ok(());
        }

        let supported = self.supported_architectures()?;
        let gpu = visible_gpu();
        let recommended = gpu
            .as_ref()
            .and_then(|(_, capability)| closest_architecture(&supported, *capability));
        let default = recommended.map_or_else(|| "default".into(), |code| format!("sm_{code}"));
        let chosen = if io::stdin().is_terminal() && io::stderr().is_terminal() {
            prompt_architecture(&supported, gpu.as_ref(), &default)?
        } else {
            default
        };
        cache.save_cuda_architecture(&chosen)?;
        crate::ui::status("Configured", &format!("CUDA architecture {chosen}"));
        let _ = self.architecture.set(chosen);
        Ok(())
    }

    fn supported_architectures(&self) -> Result<Vec<u16>, String> {
        let output = Command::new(&self.nvcc)
            .arg("--list-gpu-code")
            .output()
            .map_err(|e| format!("cannot query nvcc GPU architectures: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "nvcc --list-gpu-code failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let mut codes: Vec<_> = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .filter_map(|name| name.strip_prefix("sm_")?.parse().ok())
            .collect();
        codes.sort_unstable();
        codes.dedup();
        if codes.is_empty() {
            return Err("nvcc did not report any supported CUDA architectures".into());
        }
        Ok(codes)
    }
}

fn toolkit_missing(nvcc: &Path, root: &Path, file: &str) -> String {
    format!(
        "CUDA Toolkit at {} is missing {file}. Install the CUDA development files that match {}, or put a complete toolkit's bin directory first on PATH.",
        root.display(),
        nvcc.display()
    )
}

pub fn host_compiler_hint(diagnostics: &str) -> Option<&'static str> {
    let diagnostics = diagnostics.to_ascii_lowercase();
    let rejected = diagnostics.contains("unsupported gnu version")
        || diagnostics.contains("unsupported host compiler")
        || (diagnostics.contains("host_config.h")
            && diagnostics.lines().any(|line| {
                line.contains("error")
                    && (line.contains("unsupported") || line.contains("not supported"))
            }));
    rejected.then_some(
        "nvcc rejected its host compiler. Install a compiler version supported by this CUDA Toolkit and set NVCC_CCBIN to its path.",
    )
}

fn visible_gpu() -> Option<(String, u16)> {
    let mut command = Command::new("nvidia-smi");
    command.args(["--query-gpu=name,compute_cap", "--format=csv,noheader"]);
    if let Ok(visible) = std::env::var("CUDA_VISIBLE_DEVICES") {
        let id = visible.split(',').next()?.trim();
        if id.is_empty() || id == "-1" {
            return None;
        }
        command.arg(format!("--id={id}"));
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let (name, capability) = text.lines().next()?.rsplit_once(',')?;
    let (major, minor) = capability.trim().split_once('.')?;
    let code = major
        .parse::<u16>()
        .ok()?
        .checked_mul(10)?
        .checked_add(minor.parse().ok()?)?;
    Some((name.trim().to_owned(), code))
}

fn closest_architecture(supported: &[u16], capability: u16) -> Option<u16> {
    supported
        .iter()
        .copied()
        .take_while(|&code| code <= capability)
        .last()
}

fn prompt_architecture(
    supported: &[u16],
    gpu: Option<&(String, u16)>,
    default: &str,
) -> Result<String, String> {
    match gpu {
        Some((name, capability)) => eprintln!(
            "CUDA GPU: {name} (compute capability {}.{})",
            capability / 10,
            capability % 10
        ),
        None => eprintln!("No NVIDIA GPU detected. You can select a target for another machine."),
    }
    let choices: Vec<_> = supported.iter().map(|code| format!("sm_{code}")).collect();
    eprintln!(
        "Supported CUDA architectures: {}, default (nvcc default)",
        choices.join(", ")
    );
    loop {
        eprint!("CUDA architecture [{default}]: ");
        io::stderr().flush().map_err(|e| e.to_string())?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        let answer = answer.trim();
        if answer.is_empty() {
            return Ok(default.into());
        }
        if answer == "default" || choices.iter().any(|choice| choice == answer) {
            return Ok(answer.into());
        }
        eprintln!(
            "Enter a supported architecture such as {} or default.",
            choices[0]
        );
    }
}
