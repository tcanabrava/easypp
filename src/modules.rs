//! Dependencies between project-local C++ named module units.
use crate::cond::Conditions;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

#[derive(Default, Debug)]
struct Unit {
    provides: Option<String>,
    imports: BTreeSet<String>,
    headers: BTreeSet<HeaderUnit>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeaderUnit {
    pub name: String,
    pub quoted: bool,
}

#[derive(Default, Debug)]
pub struct Modules {
    units: BTreeMap<PathBuf, Unit>,
    providers: BTreeMap<String, (PathBuf, Option<String>)>,
    plans: BTreeMap<PathBuf, Plan>,
    standard: BTreeMap<PathBuf, Vec<String>>,
    header_resolutions: Mutex<HashMap<Vec<String>, PathBuf>>,
}

#[derive(Default, Debug)]
pub struct Plan {
    pub depth: usize,
    pub imports: BTreeSet<String>,
}

impl Modules {
    pub fn discover(
        sources: impl Iterator<Item = (PathBuf, Option<String>)>,
        opts: &crate::builder::Options,
        flags: impl FnMut(&Path, Option<&str>) -> Vec<String>,
    ) -> Result<Self, String> {
        let mut graph = Self::default();
        let mut sources: Vec<_> = sources
            .filter(|(path, _)| crate::project::Lang::of(path) == Some(crate::project::Lang::Cpp))
            .filter_map(|(path, library)| {
                let text = fs::read_to_string(&path).ok()?;
                let unit = parse(&text);
                Some((path, library, unit))
            })
            .collect();
        // Direct declarations are a cheap indication that the project uses
        // named modules. The scanner can also find imports in included headers.
        if sources
            .iter()
            .any(|(_, _, unit)| unit.provides.is_some() || !unit.imports.is_empty())
        {
            scan_named_modules(&mut sources, opts, flags);
        }
        for (path, library, unit) in sources {
            if let Some(name) = &unit.provides
                && let Some((previous, _)) = graph
                    .providers
                    .insert(name.clone(), (path.clone(), library))
            {
                return Err(format!(
                    "module '{name}' is provided by both {} and {}",
                    previous.display(),
                    path.display()
                ));
            }
            if unit.provides.is_some() || !unit.imports.is_empty() || !unit.headers.is_empty() {
                graph.units.insert(path, unit);
            }
        }
        graph.add_standard_modules()?;
        let mut active = BTreeSet::new();
        for path in graph.units.keys() {
            visit(
                path,
                &graph.units,
                &graph.providers,
                &mut graph.plans,
                &mut active,
            )?;
        }
        Ok(graph)
    }

    pub fn provider(&self, name: &str) -> &Path {
        &self.providers[name].0
    }

    pub fn provides(&self, src: &Path) -> bool {
        self.units.get(src).is_some_and(|u| u.provides.is_some())
    }

    pub fn plan(&self, src: &Path) -> Option<&Plan> {
        self.plans.get(src)
    }

    pub fn headers(&self, src: &Path) -> impl Iterator<Item = &HeaderUnit> {
        self.units
            .get(src)
            .into_iter()
            .flat_map(|unit| &unit.headers)
    }

    pub fn resolve_header(&self, mut command: Vec<String>, name: &str) -> Option<PathBuf> {
        command.extend([
            // The driver lists sanitizer ignorelists before the primary input.
            // Suppress them only in this lookup, not when compiling the PCM.
            "-fno-sanitize-ignorelist".into(),
            "-M".into(),
            "-MT".into(),
            "header".into(),
            "--".into(),
            name.into(),
        ]);
        let mut resolutions = self.header_resolutions.lock().unwrap();
        if let Some(path) = resolutions.get(&command) {
            return Some(path.clone());
        }
        let output = Command::new(&command[0])
            .args(&command[1..])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let path = crate::builder::depfile_paths(&text)
            .first()?
            .canonicalize()
            .ok()?;
        resolutions.insert(command, path.clone());
        Some(path)
    }

    pub fn has_header_units(&self) -> bool {
        self.units.values().any(|unit| !unit.headers.is_empty())
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    pub fn standard_sources(&self) -> impl Iterator<Item = (&Path, &[String])> {
        self.standard
            .iter()
            .map(|(path, flags)| (path.as_path(), flags.as_slice()))
    }

    pub fn required_standard_sources<'a, 'b>(
        &'a self,
        sources: impl Iterator<Item = &'b Path>,
    ) -> BTreeSet<&'a Path> {
        if self.standard.is_empty() {
            return BTreeSet::new();
        }
        sources
            .filter_map(|src| self.plan(src))
            .flat_map(|plan| &plan.imports)
            .map(|name| self.provider(name))
            .filter(|path| self.is_standard_source(path))
            .collect()
    }

    pub fn is_standard_source(&self, path: &Path) -> bool {
        self.standard.contains_key(path)
    }

    fn add_standard_modules(&mut self) -> Result<(), String> {
        let names: BTreeSet<&str> = self
            .units
            .values()
            .flat_map(|unit| &unit.imports)
            .map(String::as_str)
            .filter(|name| {
                matches!(*name, "std" | "std.compat") && !self.providers.contains_key(*name)
            })
            .collect();
        if names.is_empty() {
            return Ok(());
        }
        let needs_compat = names.contains("std.compat");
        let manifest = standard_manifest()?;
        let text = fs::read_to_string(&manifest).map_err(|e| {
            format!(
                "cannot read C++ module manifest {}: {e}",
                manifest.display()
            )
        })?;
        let data: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("invalid C++ module manifest {}: {e}", manifest.display()))?;
        let entries = data["modules"]
            .as_array()
            .ok_or("C++ module manifest has no modules list")?;
        let base = manifest.parent().unwrap();
        for name in ["std", "std.compat"]
            .into_iter()
            .filter(|&name| name == "std" || needs_compat)
        {
            if self.providers.contains_key(name) {
                continue;
            }
            let entry = entries
                .iter()
                .find(|entry| {
                    entry["logical-name"].as_str() == Some(name)
                        && entry["is-std-library"].as_bool() == Some(true)
                })
                .ok_or_else(|| {
                    format!(
                        "{} does not provide the '{name}' module",
                        manifest.display()
                    )
                })?;
            let path = base.join(
                entry["source-path"]
                    .as_str()
                    .ok_or("missing standard module source path")?,
            );
            let path = path.canonicalize().map_err(|e| {
                format!(
                    "cannot locate '{name}' module source {}: {e}",
                    path.display()
                )
            })?;
            if crate::project::Lang::of(&path) != Some(crate::project::Lang::Cpp) {
                return Err(format!(
                    "unsupported standard module source: {}",
                    path.display()
                ));
            }
            let flags = entry["local-arguments"]["system-include-directories"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|dir| dir.as_str())
                .flat_map(|dir| ["-isystem".to_string(), base.join(dir).display().to_string()])
                .collect();
            let imports = if name == "std.compat" {
                BTreeSet::from(["std".to_string()])
            } else {
                BTreeSet::new()
            };
            self.providers
                .insert(name.to_string(), (path.clone(), None));
            self.units.insert(
                path.clone(),
                Unit {
                    provides: Some(name.to_string()),
                    imports,
                    ..Unit::default()
                },
            );
            self.standard.insert(path, flags);
        }
        Ok(())
    }

    /// Turn named imports into edges in the existing static-library graph.
    pub fn libraries(
        &self,
        sources: &[PathBuf],
        own: Option<&str>,
    ) -> Result<BTreeSet<String>, String> {
        let mut libraries = BTreeSet::new();
        for src in sources {
            if let Some(unit) = self.units.get(src) {
                for name in &unit.imports {
                    let (provider, library) = &self.providers[name];
                    match library {
                        Some(name) if Some(name.as_str()) != own => {
                            libraries.insert(name.clone());
                        }
                        None if !self.is_standard_source(provider)
                            && !sources.contains(provider) =>
                        {
                            return Err(format!(
                                "{} imports '{name}' from another executable or test ({}); move the module to src/libs/",
                                src.display(),
                                provider.display()
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(libraries)
    }
}

fn scan_named_modules(
    sources: &mut [(PathBuf, Option<String>, Unit)],
    opts: &crate::builder::Options,
    mut flags: impl FnMut(&Path, Option<&str>) -> Vec<String>,
) {
    if !crate::system::in_path("clang-scan-deps") {
        return;
    }
    let Ok(directory) = std::env::current_dir() else {
        return;
    };
    let entries: Vec<_> = sources.iter().enumerate()
        // P1689 can't scan a header-unit import before its PCM exists.
        .filter(|(_, (_, _, unit))| unit.headers.is_empty())
        .map(|(index, (src, library, unit))| {
            let output = format!("easypp-scan-{index}.o");
            let mut arguments = flags(src, library.as_deref());
            if unit.provides.is_some() || matches!(src.extension().and_then(|s| s.to_str()), Some("cppm" | "ixx" | "ccm" | "cxxm")) {
                arguments.extend(["-x".into(), "c++".into()]);
            }
            arguments.extend(["-c".into(), src.display().to_string(), "-o".into(), output.clone()]);
            serde_json::json!({ "directory": directory, "file": src, "output": output, "arguments": arguments })
        }).collect();
    if entries.is_empty() {
        return;
    }

    // The database is private to this invocation, even during concurrent scans.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let db = std::env::temp_dir().join(format!("easypp-scan-{}-{nonce}.json", std::process::id()));
    let Ok(file) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&db)
    else {
        return;
    };
    let result = serde_json::to_writer(file, &entries).ok().and_then(|()| {
        Command::new("clang-scan-deps")
            .arg("-format=p1689")
            .arg(format!("-compilation-database={}", db.display()))
            .arg("-j")
            .arg(opts.jobs.to_string())
            .output()
            .ok()
    });
    let _ = fs::remove_file(&db);
    let Some(output) = result else { return };
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return;
    };
    let Some(rules) = document["rules"].as_array() else {
        return;
    };
    for rule in rules {
        let Some(index) = rule["primary-output"]
            .as_str()
            .and_then(|output| {
                Path::new(output)
                    .file_name()?
                    .to_str()?
                    .strip_prefix("easypp-scan-")
            })
            .and_then(|output| output.strip_suffix(".o")?.parse::<usize>().ok())
        else {
            continue;
        };
        let Some((_, _, unit)) = sources.get_mut(index) else {
            continue;
        };
        // Failed units (for example those needing generated Qt headers) can be
        // absent from otherwise valid output. Leave their source-text plan intact.
        unit.provides = rule["provides"]
            .as_array()
            .and_then(|entries| entries.first())
            .and_then(|entry| entry["logical-name"].as_str())
            .map(str::to_owned);
        unit.imports = rule["requires"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry["logical-name"].as_str().map(str::to_owned))
            .collect();
    }
}

fn standard_manifest() -> Result<PathBuf, String> {
    let query = |arg: &str| -> Option<PathBuf> {
        let output = Command::new("clang++").arg(arg).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        if !path.is_absolute() {
            return None;
        }
        path.canonicalize().ok().filter(|p| p.is_file())
    };
    if let Some(path) = query("-print-library-module-manifest-path") {
        return Ok(path);
    }
    // Older drivers can locate the manifest as a library file but do not
    // expose it through the dedicated driver query.
    let macros = Command::new("clang++")
        .args(["-dM", "-E", "-x", "c++", "-include", "version", "/dev/null"])
        .output()
        .map_err(|e| format!("cannot query Clang's C++ standard library: {e}"))?;
    let macros = String::from_utf8_lossy(&macros.stdout);
    let fallback = if macros.contains("#define _LIBCPP_VERSION ") {
        "-print-file-name=libc++.modules.json"
    } else {
        "-print-file-name=libstdc++.modules.json"
    };
    query(fallback).ok_or_else(|| "import std requires a Clang-compatible standard library module manifest. Install your C++ standard library's module sources.".into())
}

type Providers = BTreeMap<String, (PathBuf, Option<String>)>;

fn visit(
    path: &Path,
    units: &BTreeMap<PathBuf, Unit>,
    providers: &Providers,
    plans: &mut BTreeMap<PathBuf, Plan>,
    active: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    if plans.contains_key(path) {
        return Ok(());
    }
    if !active.insert(path.to_path_buf()) {
        return Err(format!("C++ module import cycle at {}", path.display()));
    }
    let mut plan = Plan::default();
    for name in &units[path].imports {
        let (provider, _) = providers.get(name).ok_or_else(|| {
            format!(
                "{} imports '{name}', but no project source provides that module",
                path.display()
            )
        })?;
        visit(provider, units, providers, plans, active)?;
        let dependency = &plans[provider];
        plan.depth = plan.depth.max(dependency.depth + 1);
        plan.imports.extend(dependency.imports.iter().cloned());
        plan.imports.insert(name.clone());
    }
    active.remove(path);
    plans.insert(path.to_path_buf(), plan);
    Ok(())
}

fn parse(text: &str) -> Unit {
    // Skip ordinary sources quickly, then mask comments and literals before
    // looking at statements. Like include discovery, this is not a preprocessor.
    if !text.contains("module") && !text.contains("import") {
        return Unit::default();
    }
    let spliced = text.replace("\\\r\n", "").replace("\\\n", "");
    let text = mask_comments_and_literals(&spliced);
    let mut conditions = Conditions::default();
    let mut code = String::new();
    for line in text.lines() {
        if let Some(directive) = line.trim_start().strip_prefix('#') {
            conditions.directive(directive.trim_start());
        } else if conditions.active() {
            code.push_str(line);
            code.push(' ');
        }
    }

    let mut unit = Unit::default();
    let mut primary = None;
    let mut at_preamble = true;
    let mut depth = 0usize;
    for statement in code.split_inclusive([';', '{', '}']) {
        let statement = statement.trim();
        if statement.ends_with('{') {
            depth += 1;
            at_preamble = false;
            continue;
        }
        if statement.ends_with('}') {
            depth = depth.saturating_sub(1);
            continue;
        }
        let Some(statement) = statement.strip_suffix(';') else {
            continue;
        };
        if depth != 0 || statement.trim() == "module" {
            continue;
        }
        let can_declare_module = std::mem::replace(&mut at_preamble, false);
        let (exported, statement) =
            keyword(statement, "export").map_or((false, statement), |s| (true, s));
        if can_declare_module && let Some(name) = keyword(statement, "module").and_then(module_name)
        {
            primary = Some(name.split(':').next().unwrap().to_string());
            if exported || name.contains(':') {
                unit.provides = Some(name);
            } else {
                // A non-partition implementation unit implicitly imports its interface.
                unit.imports.insert(name);
            }
        } else if let Some(operand) = keyword(statement, "import") {
            if let Some((name, quoted)) = operand
                .strip_prefix('<')
                .and_then(|text| text.split_once('>'))
                .map(|(name, _)| (name, false))
                .or_else(|| {
                    operand
                        .strip_prefix('"')
                        .and_then(|text| text.split_once('"'))
                        .map(|(name, _)| (name, true))
                })
            {
                unit.headers.insert(HeaderUnit {
                    name: name.to_owned(),
                    quoted,
                });
                continue;
            }
            let name = if let Some(partition) = operand.strip_prefix(':') {
                primary
                    .as_ref()
                    .zip(module_name(partition))
                    .map(|(m, p)| format!("{m}:{p}"))
            } else {
                module_name(operand)
            };
            unit.imports.extend(name);
        }
    }
    unit
}

fn keyword<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    (!rest.starts_with(|c: char| c.is_alphanumeric() || c == '_')).then(|| rest.trim_start())
}

fn module_name(text: &str) -> Option<String> {
    let text = text.split_once("[[").map_or(text, |(name, _)| name);
    let mut name = String::new();
    // Dots and colons are separate tokens and may be surrounded by spaces.
    for part in text.split_inclusive(['.', ':']) {
        let delimiter = part.chars().last().filter(|c| matches!(c, '.' | ':'));
        let ident = delimiter
            .map_or(part, |d| &part[..part.len() - d.len_utf8()])
            .trim();
        let mut chars = ident.chars();
        if !chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
            || !chars.all(|c| c.is_alphanumeric() || c == '_')
        {
            return None;
        }
        name.push_str(ident);
        name.extend(delimiter);
    }
    (!name.is_empty() && !name.ends_with(['.', ':'])).then_some(name)
}

fn in_number(prefix: &str) -> bool {
    prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '\''))
        .last()
        .is_some_and(|c| c.is_ascii_digit())
}

/// Preserve newlines and quoted import operands while masking other literals.
fn mask_comments_and_literals(mut text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    while !text.is_empty() {
        let comment = text.starts_with("//") || text.starts_with("/*");
        let end = if text.starts_with("//") {
            text.find('\n').unwrap_or(text.len())
        } else if let Some(rest) = text.strip_prefix("/*") {
            rest.find("*/").map_or(text.len(), |n| n + 4)
        } else if let Some(rest) = text.strip_prefix("R\"")
            && let Some((delimiter, body)) = rest.split_once('(')
            && delimiter.len() <= 16
            && !delimiter.contains(char::is_whitespace)
        {
            let closing = format!("){delimiter}\"");
            body.find(&closing)
                .map_or(text.len(), |i| 3 + delimiter.len() + i + closing.len())
        } else if text.starts_with('"') || (text.starts_with('\'') && !in_number(&output)) {
            let quote = text.as_bytes()[0];
            let mut escaped = false;
            let mut bytes = text.bytes().enumerate().skip(1);
            bytes
                .find_map(|(i, byte)| {
                    let closes = !escaped && byte == quote;
                    escaped = !escaped && byte == b'\\';
                    closes.then_some(i + 1)
                })
                .unwrap_or(text.len())
        } else {
            let ch = text.chars().next().unwrap();
            output.push(ch);
            text = &text[ch.len_utf8()..];
            continue;
        };
        let follows_import = output
            .trim_end()
            .strip_suffix("import")
            .is_some_and(|before| !before.ends_with(|c: char| c.is_alphanumeric() || c == '_'));
        if text.starts_with('"') && follows_import {
            output.push_str(&text[..end]);
        } else {
            output.push(if comment { ' ' } else { '@' });
            output.extend(text[..end].chars().filter(|&c| c == '\n'));
        }
        text = &text[end..];
    }
    output
}
