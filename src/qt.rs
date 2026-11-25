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
use quick_xml::escape::escape;
use quick_xml::{Reader, events::Event};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// What `includes()` reports for a file that declares a Qt meta-object.
pub const MOC_MARKER: &str = "#qt moc";
pub(crate) const QML_MANIFEST: &str = "easypp_qml.qrc";
pub(crate) const QML_TYPEINFO: &str = "easypp.qmltypes";

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
    pub qml: Option<QmlResources>,
    pub kconfig: Vec<PathBuf>,
    pub gresource: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct QmlResources {
    pub base: PathBuf,
    pub prefix: String,
    pub files: Vec<PathBuf>,
    pub module: Option<QmlModule>,
}

#[derive(Debug, Clone)]
pub struct QmlModule {
    pub uri: String,
    pub qmldir: PathBuf,
}

impl QtFiles {
    pub fn from_files(base: &Path, files: &[PathBuf], moc: Vec<PathBuf>) -> Result<Self, String> {
        let with_ext = |ext: &str| -> Vec<PathBuf> {
            files
                .iter()
                .filter(|f| f.extension().is_some_and(|e| e == ext))
                .cloned()
                .collect()
        };
        let qrc = with_ext("qrc");
        let module = qml_module(base)?;
        if module.is_some() {
            for source in moc
                .iter()
                .filter(|path| crate::project::Lang::of(path).is_some())
            {
                let text =
                    fs::read_to_string(source).map_err(|e| format!("{}: {e}", source.display()))?;
                if text.lines().map(str::trim_start).any(|line| {
                    let token = line
                        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                        .next();
                    matches!(token, Some("QML_ELEMENT" | "QML_NAMED_ELEMENT"))
                }) {
                    return Err(format!(
                        "{}: declare QML-registered C++ types in headers",
                        source.display()
                    ));
                }
            }
        }
        let qml = if module.is_some()
            || (!base.starts_with("src/thirdparty") && !with_ext("qml").is_empty())
        {
            let explicit: HashSet<_> = qrc
                .iter()
                .flat_map(|path| resource_files(path))
                .filter_map(|path| path.canonicalize().ok())
                .collect();
            let qml_base = module.as_ref().map_or(base, |m| m.qmldir.parent().unwrap());
            let files: Vec<_> = files
                .iter()
                .filter(|path| path.starts_with(qml_base))
                .filter(|path| {
                    (matches!(
                        path.extension().and_then(|e| e.to_str()),
                        Some("qml" | "js" | "mjs")
                    ) || path.file_name().is_some_and(|name| name == "qmldir"))
                        && (module.as_ref().is_some_and(|m| **path == m.qmldir)
                            || explicit.is_empty()
                            || !path
                                .canonicalize()
                                .is_ok_and(|path| explicit.contains(&path)))
                })
                .cloned()
                .collect();
            let qml_base = qml_base.to_path_buf();
            (!files.is_empty()).then(|| QmlResources {
                base: qml_base,
                prefix: if let Some(module) = &module {
                    format!("/qt/qml/{}", module.uri.replace('.', "/"))
                } else {
                    base.strip_prefix(crate::project::LIBS_DIR)
                        .map_or_else(|_| "/".into(), |lib| format!("/libs/{}", lib.display()))
                },
                files,
                module,
            })
        } else {
            None
        };
        Ok(QtFiles {
            moc,
            ui: with_ext("ui").into_iter().filter(|p| is_qt_ui(p)).collect(),
            qrc,
            qml,
            kconfig: with_ext("kcfgc"),
            gresource: files
                .iter()
                .filter(|p| p.to_string_lossy().ends_with(".gresource.xml"))
                .cloned()
                .collect(),
        })
    }

    pub fn needs_qt(&self) -> bool {
        !(self.moc.is_empty()
            && self.ui.is_empty()
            && self.qrc.is_empty()
            && self.qml.is_none()
            && self.kconfig.is_empty())
    }

    pub fn is_empty(&self) -> bool {
        !self.needs_qt() && self.gresource.is_empty()
    }

    pub fn moc_headers(&self) -> impl Iterator<Item = &Path> {
        self.moc
            .iter()
            .filter(|path| crate::project::Lang::of(path).is_none())
            .map(PathBuf::as_path)
    }

    pub fn qml_header_dirs(&self) -> std::collections::BTreeSet<&Path> {
        self.moc_headers().filter_map(Path::parent).collect()
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
        if let Some(qml) = &self.qml {
            parts.push(format!("qml: {} files", qml.files.len()));
        }
        parts.join("; ")
    }
}

fn qml_module(base: &Path) -> Result<Option<QmlModule>, String> {
    if !base.starts_with(crate::project::BIN_DIR) && !base.starts_with(crate::project::TESTS_DIR) {
        return Ok(None);
    }
    let qmldir = base.join("qml/qmldir");
    if !qmldir.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&qmldir).map_err(|e| format!("{}: {e}", qmldir.display()))?;
    let mut directives = contents.lines().filter_map(|line| {
        let mut words = line.split('#').next()?.split_whitespace();
        (words.next()? == "module").then_some(words)
    });
    let Some(mut words) = directives.next() else {
        return Ok(None);
    };
    let uri = words.next().unwrap_or_default();
    let valid = uri.split('.').all(|part| {
        let mut bytes = part.bytes();
        bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
    });
    if !valid || words.next().is_some() || directives.next().is_some() {
        return Err(format!(
            "{}: expected one valid QML module URI",
            qmldir.display()
        ));
    }
    Ok(Some(QmlModule {
        uri: uri.into(),
        qmldir,
    }))
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
    pub qmltyperegistrar: Option<PathBuf>,
    pub qmllint: Option<PathBuf>,
}

/// Only Qt 6 is supported.
pub fn is_qt(dep: &SysDep) -> bool {
    matches!(dep, SysDep::Pkg(name) if name.starts_with("Qt6") || crate::kde::is_native(name))
}

/// Qt 5 packages are ignored entirely when resolving headers.
pub fn is_unsupported_package(name: &str) -> bool {
    name.starts_with("Qt5")
}

/// Forward Qt diagnostics as they arrive and explain unresolved runtime imports
/// if the application exits unsuccessfully.
pub fn run_qml(command: &mut Command) -> io::Result<ExitStatus> {
    if std::env::var_os("QT_FORCE_STDERR_LOGGING").is_none() {
        command.env("QT_FORCE_STDERR_LOGGING", "1");
    }
    let mut child = command.stderr(Stdio::piped()).spawn()?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut missing = std::collections::BTreeSet::new();
        let mut line = Vec::new();
        let mut buf = [0; 4096];
        loop {
            let n = match stderr.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            let _ = io::stderr().write_all(&buf[..n]);
            for segment in buf[..n].split_inclusive(|byte| *byte == b'\n') {
                if line.len() < 8192 {
                    line.extend_from_slice(segment);
                }
                if segment.ends_with(b"\n") {
                    if let Some(uri) = missing_import(&String::from_utf8_lossy(&line)) {
                        missing.insert(uri.to_owned());
                    }
                    line.clear();
                }
            }
        }
        if let Some(uri) = missing_import(&String::from_utf8_lossy(&line)) {
            missing.insert(uri.to_owned());
        }
        let _ = send.send(missing);
    });
    let status = child.wait()?;
    // A subprocess can inherit stderr and keep the pipe open after Qt exits.
    if let Ok(missing) = receive.recv_timeout(Duration::from_millis(100))
        && !status.success()
    {
        for uri in missing {
            crate::ui::warn(&format!(
                "QML module '{uri}' was unavailable at runtime. Install the Qt 6 QML package that provides it, or add its import root to QML_IMPORT_PATH."
            ));
        }
    }
    Ok(status)
}

fn missing_import(line: &str) -> Option<&str> {
    let (_, tail) = line.split_once("module \"")?;
    let (uri, tail) = tail.split_once('"')?;
    let tail = match tail.strip_prefix(" version ") {
        Some(version) => version.trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == '.'),
        None => tail,
    };
    (!uri.is_empty()
        && tail.starts_with(" is not installed")
        && uri
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_')))
    .then_some(uri)
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
        qmltyperegistrar: find("qmltyperegistrar"),
        qmllint: find("qmllint"),
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
    pub qml_manifest: Option<(PathBuf, String)>,
    pub qml_registration: Option<Step>,
    pub qml_resource: Option<Step>,
}

impl Plan {
    /// Headers before moc, then type registration before bundling QML resources.
    pub const PHASES: [fn(&Self) -> &[Step]; 4] = [
        |plan| &plan.ui,
        |plan| &plan.generate,
        |plan| plan.qml_registration.as_slice(),
        |plan| plan.qml_resource.as_slice(),
    ];
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
    let moc_step = |input: &Path, output: PathBuf| {
        let mut cmd = vec![s(&tools.moc)];
        cmd.extend(moc_flags.iter().cloned());
        cmd.extend([s(input), "-o".into(), s(&output)]);
        Step {
            tool: "moc",
            input: input.to_path_buf(),
            inputs: vec![input.to_path_buf()],
            output,
            cmd,
            additional_outputs: vec![],
        }
    };
    let rcc_step = |tool, name, input: &Path, output: PathBuf, dependencies: Vec<PathBuf>| {
        let cmd = vec![
            s(&tools.rcc),
            "--name".into(),
            name,
            s(input),
            "-o".into(),
            s(&output),
        ];
        Step {
            tool,
            input: input.to_path_buf(),
            inputs: std::iter::once(input.to_path_buf())
                .chain(dependencies)
                .collect(),
            output,
            cmd,
            additional_outputs: vec![],
        }
    };

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
        plan.generate.push(moc_step(&header, output));
    }

    let module = files.qml.as_ref().and_then(|qml| qml.module.as_ref());
    let mut metatypes = Vec::new();
    for input in &files.moc {
        let is_source = crate::project::Lang::of(input).is_some();
        let output = if is_source {
            gen_dir.join(format!("{}.moc", stem(input)))
        } else {
            unique(format!("moc_{}", stem(input)), ".cpp")
        };
        if !is_source {
            plan.sources.push(output.clone());
        }
        let mut step = moc_step(input, output);
        if module.is_some() && !is_source {
            step.cmd.push("--output-json".into());
            let json = step.output.with_extension("cpp.json");
            metatypes.push(json.clone());
            step.additional_outputs.push(json);
        }
        plan.generate.push(step);
    }

    for qrc in &files.qrc {
        let output = unique(format!("qrc_{}", stem(qrc)), ".cpp");
        plan.resources.push(output.clone());
        plan.generate
            .push(rcc_step("rcc", stem(qrc), qrc, output, resource_files(qrc)));
    }
    if let Some(qml) = &files.qml {
        let manifest = gen_dir.join(QML_MANIFEST);
        let output = unique("qrc_easypp_qml".into(), ".cpp");
        let name = format!(
            "easypp_qml_{}",
            gen_dir
                .to_string_lossy()
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let mut contents = format!("<RCC><qresource prefix=\"{}\">\n", escape(&qml.prefix));
        let back_to_root = "../".repeat(gen_dir.components().count());
        let mut inputs = qml.files.clone();
        if let Some(module) = module {
            let registration = gen_dir.join("easypp_qml_registration.cpp");
            let typeinfo = gen_dir.join(QML_TYPEINFO);
            let registrar = tools
                .qmltyperegistrar
                .as_ref()
                .expect("validated QML registrar");
            let mut cmd = vec![
                s(registrar),
                "--import-name".into(),
                module.uri.clone(),
                "--major-version".into(),
                "1".into(),
                "--minor-version".into(),
                "0".into(),
                "--generate-qmltypes".into(),
                s(&typeinfo),
                "-o".into(),
                s(&registration),
            ];
            cmd.extend(metatypes.iter().map(|path| s(path)));
            plan.qml_registration = Some(Step {
                tool: "qmltyperegistrar",
                input: module.qmldir.clone(),
                inputs: metatypes,
                output: registration.clone(),
                additional_outputs: vec![typeinfo.clone()],
                cmd,
            });
            plan.sources.push(registration);
            inputs.push(typeinfo);
            let _ = writeln!(contents, "  <file>{QML_TYPEINFO}</file>");
        }
        for file in &qml.files {
            let alias = s(file
                .strip_prefix(&qml.base)
                .expect("QML file under component"));
            let source = format!("{back_to_root}{}", s(file));
            let _ = writeln!(
                contents,
                "  <file alias=\"{}\">{}</file>",
                escape(&alias),
                escape(&source)
            );
        }
        contents.push_str("</qresource></RCC>\n");
        plan.qml_manifest = Some((manifest.clone(), contents));
        plan.resources.push(output.clone());
        plan.qml_resource = Some(rcc_step("rcc (QML)", name, &manifest, output, inputs));
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
        if open_end < close
            && let Ok(path) = quick_xml::escape::unescape(rest[open_end + 1..close].trim())
        {
            out.push(base.join(path.as_ref()));
        }
        rest = &rest[close + "</file>".len()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_missing_qml_import_diagnostics() {
        assert_eq!(
            missing_import("qrc:/Main.qml:2:1: module \"QtQuick.Controls\" is not installed"),
            Some("QtQuick.Controls")
        );
        assert_eq!(
            missing_import("module \"QtQuick\" version 999.0 is not installed"),
            Some("QtQuick")
        );
        assert_eq!(
            missing_import("module \"QtQuick\" has no type called Window"),
            None
        );
    }

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
            qmltyperegistrar: None,
            qmllint: None,
        };
        let files = QtFiles {
            moc: vec![
                "a/window.hpp".into(),
                "b/window.hpp".into(),
                "a/main.cpp".into(),
            ],
            ui: vec!["a/window.ui".into()],
            qrc: vec![],
            qml: None,
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
