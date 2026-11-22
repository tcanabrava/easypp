//! Dependencies between project-local C++ named module units.
use crate::cond::Conditions;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
struct Unit {
    provides: Option<String>,
    imports: BTreeSet<String>,
}

#[derive(Default, Debug)]
pub struct Modules {
    units: BTreeMap<PathBuf, Unit>,
    providers: BTreeMap<String, (PathBuf, Option<String>)>,
    plans: BTreeMap<PathBuf, Plan>,
}

#[derive(Default, Debug)]
pub struct Plan {
    pub depth: usize,
    pub imports: BTreeSet<String>,
}

impl Modules {
    pub fn discover(
        sources: impl Iterator<Item = (PathBuf, Option<String>)>,
    ) -> Result<Self, String> {
        let mut graph = Self::default();
        for (path, library) in sources {
            if crate::project::Lang::of(&path) != Some(crate::project::Lang::Cpp) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let unit = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
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
            if unit.provides.is_some() || !unit.imports.is_empty() {
                graph.units.insert(path, unit);
            }
        }
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

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
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
                        None if !sources.contains(provider) => {
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

fn parse(text: &str) -> Result<Unit, String> {
    // Skip ordinary sources quickly, then mask comments and literals before
    // looking at statements. Like include discovery, this is not a preprocessor.
    if !text.contains("module") && !text.contains("import") {
        return Ok(Unit::default());
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
            if operand.starts_with(['<', '"']) {
                return Err("header-unit imports are not supported; use #include".into());
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
    Ok(unit)
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

/// Preserve newlines and replace literals with an opaque token (a quote for
/// strings, so a header-unit import remains distinguishable).
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
        output.push(if comment {
            ' '
        } else if text.starts_with('"') {
            '"'
        } else {
            '@'
        });
        output.extend(text[..end].chars().filter(|&c| c == '\n'));
        text = &text[end..];
    }
    output
}
