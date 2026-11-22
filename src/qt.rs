//! Qt 6 code generation: moc, uic and rcc, with nothing to configure.
//! Qt 5 is not supported: its packages are ignored when resolving headers.
//!
//! - A file with `Q_OBJECT`, `Q_GADGET` or `Q_NAMESPACE` goes through moc. A
//!   header `a.hpp` gives `moc_a.cpp`, compiled into its library or program.
//!   A source `a.cpp` gives `a.moc`, for the usual `#include "a.moc"` at its end.
//! - `form.ui` gives `ui_form.h` (uic).
//! - `res.qrc` gives `qrc_res.cpp` (rcc). Resources of a library are linked
//!   straight into each program that uses it: inside a static archive their
//!   self-registering object would be dropped by the linker.
//!
//! Generated files go to `build/<profile>/gen/<component>/`, which is on the
//! include path of that component and of everything that uses it.

use crate::system::SysDep;
use quick_xml::{Reader, events::Event};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// What `includes()` reports for a file that declares a Qt meta-object.
pub const MOC_MARKER: &str = "#qt moc";

/// Lines that make a file need moc.
pub fn needs_moc(line: &str) -> bool {
    let line = line.trim_start();
    ["Q_OBJECT", "Q_GADGET", "Q_NAMESPACE"]
        .iter()
        .any(|m| line.starts_with(m))
}

/// The Qt inputs of one library or program.
#[derive(Debug, Default, Clone)]
pub struct QtFiles {
    /// Headers and sources containing Q_OBJECT and friends.
    pub moc: Vec<PathBuf>,
    pub ui: Vec<PathBuf>,
    pub qrc: Vec<PathBuf>,
    pub kconfig: Vec<PathBuf>,
    pub gresource: Vec<PathBuf>,
}

impl QtFiles {
    pub fn from_files(files: &[PathBuf], moc: Vec<PathBuf>) -> QtFiles {
        let with_ext = |ext: &str| -> Vec<PathBuf> {
            files
                .iter()
                .filter(|f| f.extension().is_some_and(|e| e == ext))
                .cloned()
                .collect()
        };
        QtFiles {
            moc,
            ui: with_ext("ui").into_iter().filter(|p| is_qt_ui(p)).collect(),
            qrc: with_ext("qrc"),
            kconfig: with_ext("kcfgc"),
            gresource: files
                .iter()
                .filter(|p| p.to_string_lossy().ends_with(".gresource.xml"))
                .cloned()
                .collect(),
        }
    }

    pub fn needs_qt(&self) -> bool {
        !(self.moc.is_empty()
            && self.ui.is_empty()
            && self.qrc.is_empty()
            && self.kconfig.is_empty())
    }

    pub fn is_empty(&self) -> bool {
        !self.needs_qt() && self.gresource.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        for (tool, files) in [
            ("moc", &self.moc),
            ("uic", &self.ui),
            ("rcc", &self.qrc),
            ("kconfig", &self.kconfig),
            ("gresource", &self.gresource),
        ] {
            if !files.is_empty() {
                let names: Vec<String> = files
                    .iter()
                    .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
                    .collect();
                parts.push(format!("{tool} {}", names.join(" ")));
            }
        }
        parts.join("; ")
    }
}

fn is_qt_ui(path: &Path) -> bool {
    let Ok(mut reader) = Reader::from_file(path) else {
        return false;
    };
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(root) | Event::Empty(root)) => return root.name().as_ref() == "ui",
            Ok(Event::Eof) | Err(_) => return false,
            _ => buffer.clear(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QtTools {
    pub version: String,
    pub moc: PathBuf,
    pub uic: PathBuf,
    pub rcc: PathBuf,
}

/// Only Qt 6 is supported.
pub fn is_qt(dep: &SysDep) -> bool {
    matches!(dep, SysDep::Pkg(name) if name.starts_with("Qt6") || crate::kde::is_native(name))
}

/// Qt 5 packages are ignored entirely when resolving headers.
pub fn is_unsupported_package(name: &str) -> bool {
    name.starts_with("Qt5")
}

/// Finds moc, uic and rcc when `deps` use Qt 6.
pub fn find_tools<'a>(mut deps: impl Iterator<Item = &'a SysDep>) -> Option<QtTools> {
    if !deps.any(is_qt) {
        return None;
    }
    // Qt 6 keeps its tools in libexecdir, which is not on PATH.
    let mut dirs: Vec<PathBuf> = ["libexecdir", "bindir"]
        .iter()
        .filter_map(|var| {
            let out = crate::system::pkg_config_command()
                .args([&format!("--variable={var}"), "Qt6Core"])
                .stdin(Stdio::null())
                .output()
                .ok()?;
            let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (out.status.success() && !dir.is_empty()).then(|| PathBuf::from(dir))
        })
        .collect();
    if let Some(paths) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&paths));
    }

    let find = |tool: &str| {
        dirs.iter().find_map(|dir| {
            [tool.to_string(), format!("{tool}-qt6")]
                .iter()
                .map(|name| dir.join(name))
                .find(|p| p.is_file())
        })
    };
    Some(QtTools {
        version: "Qt6".to_string(),
        moc: find("moc")?,
        uic: find("uic")?,
        rcc: find("rcc")?,
    })
}

/// One code-generation command.
pub struct Step {
    pub tool: &'static str,
    pub input: PathBuf,
    pub output: PathBuf,
    pub cmd: Vec<String>,
    /// Files whose change makes the output stale.
    pub inputs: Vec<PathBuf>,
    pub additional_outputs: Vec<PathBuf>,
}

/// Everything generated for one component.
#[derive(Default)]
pub struct Plan {
    /// uic: run first, sources and moc may include the headers.
    pub ui: Vec<Step>,
    /// moc and rcc.
    pub generate: Vec<Step>,
    /// Generated sources compiled into the component itself (moc_*.cpp).
    pub sources: Vec<PathBuf>,
    /// Generated resource sources (qrc_*.cpp), linked directly into programs.
    pub resources: Vec<PathBuf>,
}

/// Plans the generated files of a component. `moc_flags` are the component's
/// -I/-D flags (moc needs them to see macros and base classes).
pub fn plan(tools: &QtTools, files: &QtFiles, gen_dir: &Path, moc_flags: &[String]) -> Plan {
    let mut plan = Plan::default();
    let mut names = HashSet::new();
    // moc_a.cpp, or moc_a_2.cpp when two inputs share a stem.
    let mut unique = |stem: String, ext: &str| {
        let mut name = format!("{stem}{ext}");
        let mut n = 2;
        while !names.insert(name.clone()) {
            name = format!("{stem}_{n}{ext}");
            n += 1;
        }
        gen_dir.join(name)
    };
    let stem = |p: &Path| p.file_stem().unwrap().to_string_lossy().into_owned();
    let s = |p: &Path| p.to_string_lossy().into_owned();

    for ui in &files.ui {
        let output = unique(format!("ui_{}", stem(ui)), ".h");
        plan.ui.push(Step {
            tool: "uic",
            additional_outputs: vec![],
            cmd: vec![s(&tools.uic), s(ui), "-o".into(), s(&output)],
            input: ui.clone(),
            inputs: vec![ui.clone()],
            output,
        });
    }

    for config in &files.kconfig {
        let schema = kconfig_schema(config).expect("validated KConfig input");
        let base = stem(config);
        let output = gen_dir.join(format!("{base}.cpp"));
        let header = gen_dir.join(format!("{base}.h"));
        plan.sources.push(output.clone());
        plan.ui.push(Step {
            tool: "kconfig",
            input: config.clone(),
            cmd: vec![
                s(&crate::kde::config_compiler().expect("validated KConfig compiler")),
                s(&schema),
                s(config),
                "-d".into(),
                s(gen_dir),
            ],
            inputs: vec![config.clone(), schema],
            additional_outputs: vec![header.clone()],
            output,
        });
        // KConfig includes this file itself when it generates a Q_OBJECT class.
        let output = gen_dir.join(format!("moc_{base}.cpp"));
        let mut cmd = vec![s(&tools.moc)];
        cmd.extend(moc_flags.iter().cloned());
        cmd.extend([s(&header), "-o".into(), s(&output)]);
        plan.generate.push(Step {
            tool: "moc",
            input: header.clone(),
            inputs: vec![header],
            output,
            cmd,
            additional_outputs: vec![],
        });
    }

    for input in &files.moc {
        let is_source = crate::project::Lang::of(input).is_some();
        let output = if is_source {
            gen_dir.join(format!("{}.moc", stem(input)))
        } else {
            unique(format!("moc_{}", stem(input)), ".cpp")
        };
        let mut cmd = vec![s(&tools.moc)];
        cmd.extend(moc_flags.iter().cloned());
        cmd.extend([s(input), "-o".into(), s(&output)]);
        if !is_source {
            plan.sources.push(output.clone());
        }
        plan.generate.push(Step {
            tool: "moc",
            additional_outputs: vec![],
            input: input.clone(),
            inputs: vec![input.clone()],
            output,
            cmd,
        });
    }

    for qrc in &files.qrc {
        let output = unique(format!("qrc_{}", stem(qrc)), ".cpp");
        let mut inputs = vec![qrc.clone()];
        inputs.extend(resource_files(qrc));
        plan.resources.push(output.clone());
        plan.generate.push(Step {
            tool: "rcc",
            additional_outputs: vec![],
            cmd: vec![
                s(&tools.rcc),
                "--name".into(),
                stem(qrc),
                s(qrc),
                "-o".into(),
                s(&output),
            ],
            input: qrc.clone(),
            inputs,
            output,
        });
    }
    plan
}

/// GLib resources, like Qt resources, register themselves at program startup.
pub fn glib_resources(plan: &mut Plan, files: &QtFiles, dir: &Path) {
    for (i, manifest) in files.gresource.iter().enumerate() {
        let output = dir.join(format!("gresource_{i}.c"));
        let depfile = output.with_extension("d");
        let mut inputs = vec![manifest.clone()];
        if let Ok(text) = fs::read_to_string(&depfile) {
            inputs.extend(crate::builder::depfile_paths(&text));
        }
        plan.resources.push(output.clone());
        plan.generate.push(Step {
            tool: "glib-compile-resources",
            input: manifest.clone(),
            output: output.clone(),
            cmd: vec![
                "glib-compile-resources".into(),
                "--generate-source".into(),
                "--c-name".into(),
                format!(
                    "easypp_{}_gresource_{i}",
                    dir.to_string_lossy()
                        .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
                ),
                "--sourcedir".into(),
                manifest.parent().unwrap().display().to_string(),
                "--target".into(),
                output.display().to_string(),
                "--dependency-file".into(),
                depfile.display().to_string(),
                manifest.display().to_string(),
            ],
            inputs,
            additional_outputs: vec![depfile],
        });
    }
}

/// Resolve File= relative to the .kcfgc, as KConfig's build integration does.
pub fn kconfig_schema(config: &Path) -> Result<PathBuf, String> {
    let text = fs::read_to_string(config).map_err(|e| format!("{}: {e}", config.display()))?;
    let file = text
        .lines()
        .filter_map(|l| l.trim().split_once('='))
        .find_map(|(k, v)| (k.trim() == "File").then(|| v.trim()))
        .ok_or_else(|| format!("{}: missing File= schema", config.display()))?;
    let schema = config.parent().unwrap_or(Path::new(".")).join(file);
    if !schema.is_file() {
        return Err(format!(
            "{}: KConfig schema {} does not exist",
            config.display(),
            schema.display()
        ));
    }
    Ok(schema)
}

/// moc understands -I and -D but not -isystem.
pub fn moc_flags(compile_flags: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = compile_flags.iter();
    while let Some(flag) = it.next() {
        if flag == "-isystem" {
            if let Some(dir) = it.next() {
                out.push(format!("-I{dir}"));
            }
        } else if flag.starts_with("-I") || flag.starts_with("-D") {
            out.push(flag.clone());
        }
    }
    out
}

/// Files listed in Qt resource manifests.
fn resource_files(qrc: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(qrc) else {
        return Vec::new();
    };
    let base = qrc.parent().unwrap_or(Path::new("."));
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("<file") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let Some(close) = rest.find("</file>") else {
            break;
        };
        if open_end < close {
            out.push(base.join(rest[open_end + 1..close].trim()));
        }
        rest = &rest[close + "</file>".len()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_meta_object_macros() {
        assert!(needs_moc("    Q_OBJECT"));
        assert!(needs_moc("Q_GADGET"));
        assert!(needs_moc("  Q_NAMESPACE_EXPORT(X)"));
        assert!(!needs_moc("// Q_OBJECT"));
        assert!(!needs_moc("class A : public QObject {"));
    }

    #[test]
    fn plans_generated_files() {
        let tools = QtTools {
            version: "Qt6".into(),
            moc: "/qt/moc".into(),
            uic: "/qt/uic".into(),
            rcc: "/qt/rcc".into(),
        };
        let files = QtFiles {
            moc: vec![
                "a/window.hpp".into(),
                "b/window.hpp".into(),
                "a/main.cpp".into(),
            ],
            ui: vec!["a/window.ui".into()],
            qrc: vec![],
            kconfig: vec![],
            gresource: vec![],
        };
        let plan = plan(&tools, &files, Path::new("gen"), &["-DX".into()]);
        assert_eq!(plan.ui[0].output, PathBuf::from("gen/ui_window.h"));
        let outputs: Vec<_> = plan.generate.iter().map(|s| s.output.clone()).collect();
        assert_eq!(
            outputs,
            vec![
                PathBuf::from("gen/moc_window.cpp"),
                PathBuf::from("gen/moc_window_2.cpp"),
                PathBuf::from("gen/main.moc")
            ]
        );
        assert_eq!(plan.sources.len(), 2); // main.moc is #included, not compiled
        assert_eq!(
            plan.generate[0].cmd,
            vec!["/qt/moc", "-DX", "a/window.hpp", "-o", "gen/moc_window.cpp"]
        );
    }

    #[test]
    fn converts_flags_for_moc() {
        let flags: Vec<String> = ["-isystem", "/q", "-DA=1", "-fPIC", "-Isrc/libs"]
            .map(String::from)
            .into();
        assert_eq!(moc_flags(&flags), vec!["-I/q", "-DA=1", "-Isrc/libs"]);
    }
}
