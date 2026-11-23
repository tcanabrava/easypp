//! Per-project cache in `build/easypp.db`, a Turso (SQLite-compatible) database.
//!
//! What is stored, and when it is thrown away:
//!
//! - the system snapshot (clang's search folders, the parsed pkg-config
//!   index): redone when its fingerprint changes, i.e. when clang, clang++
//!   or pkg-config is replaced, `PKG_CONFIG_PATH` changes, or any include,
//!   library or pkg-config folder is modified;
//! - which system library each header maps to, and each package's flags:
//!   dropped together with the snapshot;
//! - the `#include` list of every project file: reused while the file's
//!   modification time and size are unchanged.
//!
//! The first build of a project pays for detection and for writing the
//! database; later builds read it back in one pass. The cache is an
//! optimisation for discovery. The database also holds the CUDA architecture
//! chosen on the first build. `easypp clean` removes the database.

use crate::system::{Flags, Package, Snapshot, SysDep, System};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::task::{Context, Poll, Waker};
use std::time::UNIX_EPOCH;
use turso::{Connection, Database, Value};

/// Bump when the tables below change; older caches are recreated.
const SCHEMA_VERSION: &str = "5"; // 2: omp; 3: Q_OBJECT; 4: #if, macro includes; 5: main()

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS toolchain (
    fingerprint TEXT NOT NULL, include_dirs TEXT NOT NULL, builtin_dirs TEXT NOT NULL,
    lib_dirs TEXT NOT NULL, default_lib_dirs TEXT NOT NULL, pc_dirs TEXT NOT NULL,
    pkg_config INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS packages (
    ord INTEGER PRIMARY KEY, name TEXT NOT NULL, include_dirs TEXT NOT NULL,
    cflags TEXT NOT NULL, libs TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS headers (
    include_name TEXT PRIMARY KEY, kind TEXT, name TEXT, dir TEXT);
CREATE TABLE IF NOT EXISTS pkg_flags (name TEXT PRIMARY KEY, cflags TEXT NOT NULL, libs TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sources (
    path TEXT PRIMARY KEY, mtime INTEGER NOT NULL, size INTEGER NOT NULL, includes TEXT NOT NULL);
";

const TABLES: &[&str] = &["toolchain", "packages", "headers", "pkg_flags", "sources"];

/// Runs a future to completion on this thread. Turso's local I/O is
/// synchronous: a pending poll has already done its I/O step, so polling
/// again simply continues.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
    }
}

fn db_err(e: turso::Error) -> String {
    e.to_string()
}

/// `#include` lists of project files, keyed by canonical path.
#[derive(Default)]
pub struct IncludeIndex {
    entries: HashMap<String, (i64, i64, Vec<String>)>,
    changed: HashSet<String>,
    seen: HashSet<String>,
}

impl IncludeIndex {
    /// The includes of `path`, from the index if the file is unchanged.
    pub fn includes(
        &mut self,
        path: &Path,
        parse: impl FnOnce(&str) -> Vec<String>,
    ) -> Vec<String> {
        let key = path.to_string_lossy().into_owned();
        let Ok(meta) = fs::metadata(path) else {
            return Vec::new();
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos() as i64);
        let size = meta.len() as i64;
        self.seen.insert(key.clone());

        if let Some((m, s, includes)) = self.entries.get(&key)
            && *m == mtime
            && *s == size
        {
            return includes.clone();
        }
        let Ok(bytes) = fs::read(path) else {
            return Vec::new();
        };
        let includes = parse(&String::from_utf8_lossy(&bytes));
        self.entries
            .insert(key.clone(), (mtime, size, includes.clone()));
        self.changed.insert(key);
        includes
    }
}

pub struct Cache {
    conn: Connection,
    _db: Database,
    _lock: File,
    /// The stored snapshot and its fingerprint, as loaded.
    stored: Option<(String, Snapshot)>,
    headers: HashMap<String, Option<SysDep>>,
    flags: HashMap<String, Flags>,
    index: IncludeIndex,
    /// Set when the system had to be detected again this run.
    redetected: bool,
}

impl Cache {
    /// Opens (or creates) `build_dir/easypp.db`.
    pub fn open(build_dir: &Path) -> Result<Cache, String> {
        fs::create_dir_all(build_dir).map_err(|e| e.to_string())?;
        // One easypp at a time per project; a second one just runs uncached.
        let lock = File::create(build_dir.join("easypp.lock")).map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|_| "another easypp is using this project's cache".to_string())?;

        let path = build_dir.join("easypp.db");
        match Cache::open_db(&path, lock) {
            Ok(cache) => Ok(cache),
            Err((e, lock)) => {
                // Unreadable or from an incompatible version: start over once.
                remove_db_files(&path);
                Cache::open_db(&path, lock).map_err(|(again, _)| format!("{e}; then: {again}"))
            }
        }
    }

    fn open_db(path: &Path, lock: File) -> Result<Cache, (String, File)> {
        let fail = |e: String, lock: File| Err((e, lock));
        let db = match block_on(turso::Builder::new_local(&path.to_string_lossy()).build()) {
            Ok(db) => db,
            Err(e) => return fail(db_err(e), lock),
        };
        let conn = match db.connect() {
            Ok(c) => c,
            Err(e) => return fail(db_err(e), lock),
        };
        let mut cache = Cache {
            conn,
            _db: db,
            _lock: lock,
            stored: None,
            headers: HashMap::new(),
            flags: HashMap::new(),
            index: IncludeIndex::default(),
            redetected: false,
        };
        match cache.load() {
            Ok(()) => Ok(cache),
            Err(e) => fail(e, cache._lock),
        }
    }

    fn exec(&self, sql: &str) -> Result<(), String> {
        block_on(self.conn.execute_batch(sql)).map_err(db_err)
    }

    fn rows(&self, sql: &str) -> Result<Vec<Vec<Value>>, String> {
        block_on(async {
            let mut rows = self.conn.query(sql, ()).await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                let values = (0..row.column_count())
                    .map(|i| row.get_value(i))
                    .collect::<Result<Vec<_>, _>>()?;
                out.push(values);
            }
            Ok::<_, turso::Error>(out)
        })
        .map_err(db_err)
    }

    fn load(&mut self) -> Result<(), String> {
        self.exec(SCHEMA)?;
        let version = self.rows("SELECT value FROM meta WHERE key = 'schema'")?;
        if version.first().map(|r| text(&r[0])) != Some(SCHEMA_VERSION.to_string()) {
            let drops: String = TABLES
                .iter()
                .map(|t| format!("DROP TABLE IF EXISTS {t};"))
                .collect();
            self.exec(&drops)?;
            self.exec(SCHEMA)?;
            self.exec(&format!(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema', '{SCHEMA_VERSION}')"
            ))?;
            return Ok(());
        }

        if let Some(row) = self.rows(
            "SELECT fingerprint, include_dirs, builtin_dirs, lib_dirs, default_lib_dirs, pc_dirs, pkg_config FROM toolchain",
        )?
        .first()
        {
            let packages = self
                .rows("SELECT name, include_dirs, cflags, libs FROM packages ORDER BY ord")?
                .iter()
                .map(|r| Package {
                    name: text(&r[0]),
                    include_dirs: paths(&r[1]),
                    cflags: list(&r[2]),
                    libs: list(&r[3]),
                })
                .collect();
            let snapshot = Snapshot {
                include_dirs: paths(&row[1]),
                builtin_dirs: paths(&row[2]),
                lib_dirs: paths(&row[3]),
                default_lib_dirs: paths(&row[4]),
                pc_dirs: paths(&row[5]),
                packages,
                pkg_config: matches!(row[6], Value::Integer(1)),
            };
            self.stored = Some((text(&row[0]), snapshot));
        }

        for r in self.rows("SELECT include_name, kind, name, dir FROM headers")? {
            let dep = match (text(&r[1]).as_str(), &r[3]) {
                ("pkg", _) => Some(SysDep::Pkg(text(&r[2]))),
                ("openmp", _) => Some(SysDep::OpenMp),
                ("cudart", _) => Some(SysDep::CudaRuntime),
                ("lib", Value::Null) => Some(SysDep::Lib {
                    name: text(&r[2]),
                    dir: None,
                }),
                ("lib", dir) => Some(SysDep::Lib {
                    name: text(&r[2]),
                    dir: Some(PathBuf::from(text(dir))),
                }),
                _ => None,
            };
            self.headers.insert(text(&r[0]), dep);
        }
        for r in self.rows("SELECT name, cflags, libs FROM pkg_flags")? {
            self.flags.insert(text(&r[0]), (list(&r[1]), list(&r[2])));
        }
        for r in self.rows("SELECT path, mtime, size, includes FROM sources")? {
            self.index
                .entries
                .insert(text(&r[0]), (int(&r[1]), int(&r[2]), list(&r[3])));
        }
        Ok(())
    }

    pub fn cuda_architecture(&self) -> Result<Option<String>, String> {
        Ok(self
            .rows("SELECT value FROM meta WHERE key = 'cuda_architecture'")?
            .first()
            .map(|row| text(&row[0])))
    }

    pub fn save_cuda_architecture(&self, architecture: &str) -> Result<(), String> {
        self.exec(&format!(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('cuda_architecture', {})",
            quote(architecture)
        ))
    }

    /// True if this run had to ask the compiler and pkg-config again.
    pub fn redetected(&self) -> bool {
        self.redetected
    }

    /// The system description: from the cache when its fingerprint still
    /// matches, otherwise detected again.
    pub fn system(&mut self, cross: Option<&crate::cross::Target>) -> System {
        if let Some((fingerprint, snapshot)) = self.stored.take()
            && snapshot.fingerprint() == fingerprint
        {
            let headers = std::mem::take(&mut self.headers);
            let flags = std::mem::take(&mut self.flags);
            // Keep copies to know what changed when saving.
            self.headers = headers.clone();
            self.flags = flags.clone();
            self.stored = Some((fingerprint, snapshot.clone()));
            return System::new(snapshot, headers, flags);
        }
        self.redetected = true;
        self.stored = None;
        self.headers.clear();
        self.flags.clear();
        System::detect(cross)
    }

    pub fn take_index(&mut self) -> IncludeIndex {
        std::mem::take(&mut self.index)
    }

    /// Writes back what this run learned, in one transaction. Does nothing
    /// (and touches no file) when nothing changed.
    pub fn save(&mut self, system: &System, index: IncludeIndex) -> Result<(), String> {
        let (headers, flags) = system.answers();
        let mut sql = String::new();

        if self.redetected {
            let snap = system.snapshot();
            sql.push_str("DELETE FROM toolchain; DELETE FROM packages; DELETE FROM headers; DELETE FROM pkg_flags;");
            sql.push_str(&format!(
                "INSERT INTO toolchain VALUES ({}, {}, {}, {}, {}, {}, {});",
                quote(&snap.fingerprint()),
                quote(&join_paths(&snap.include_dirs)),
                quote(&join_paths(&snap.builtin_dirs)),
                quote(&join_paths(&snap.lib_dirs)),
                quote(&join_paths(&snap.default_lib_dirs)),
                quote(&join_paths(&snap.pc_dirs)),
                i32::from(snap.pkg_config),
            ));
            for chunk in snap.packages.chunks(200).enumerate() {
                let (n, packages) = chunk;
                let values: Vec<String> = packages
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        format!(
                            "({}, {}, {}, {}, {})",
                            n * 200 + i,
                            quote(&p.name),
                            quote(&join_paths(&p.include_dirs)),
                            quote(&p.cflags.join("\n")),
                            quote(&p.libs.join("\n"))
                        )
                    })
                    .collect();
                sql.push_str(&format!(
                    "INSERT INTO packages VALUES {};",
                    values.join(", ")
                ));
            }
        }

        for (include, dep) in &headers {
            if self.headers.get(include) == Some(dep) {
                continue;
            }
            let (kind, name, dir) = match dep {
                None => ("NULL".to_string(), "NULL".to_string(), "NULL".to_string()),
                Some(SysDep::Pkg(name)) => (quote("pkg"), quote(name), "NULL".into()),
                Some(SysDep::OpenMp) => (quote("openmp"), "NULL".into(), "NULL".into()),
                Some(SysDep::CudaRuntime) => (quote("cudart"), "NULL".into(), "NULL".into()),
                Some(SysDep::Lib { name, dir }) => (
                    quote("lib"),
                    quote(name),
                    dir.as_ref()
                        .map_or("NULL".into(), |d| quote(&d.to_string_lossy())),
                ),
            };
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO headers VALUES ({}, {kind}, {name}, {dir});",
                quote(include)
            ));
        }
        for (name, (cflags, libs)) in &flags {
            if self.flags.get(name) == Some(&(cflags.clone(), libs.clone())) {
                continue;
            }
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO pkg_flags VALUES ({}, {}, {});",
                quote(name),
                quote(&cflags.join("\n")),
                quote(&libs.join("\n"))
            ));
        }

        for path in &index.changed {
            let (mtime, size, includes) = &index.entries[path];
            sql.push_str(&format!(
                "INSERT OR REPLACE INTO sources VALUES ({}, {mtime}, {size}, {});",
                quote(path),
                quote(&includes.join("\n"))
            ));
        }
        for path in index.entries.keys().filter(|p| !index.seen.contains(*p)) {
            sql.push_str(&format!(
                "DELETE FROM sources WHERE path = {};",
                quote(path)
            ));
        }

        if sql.is_empty() {
            return Ok(());
        }
        self.exec(&format!("BEGIN; {sql} COMMIT;"))?;
        self.headers = headers;
        self.flags = flags;
        Ok(())
    }
}

fn remove_db_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        let _ = fs::remove_file(PathBuf::from(p));
    }
}

/// An SQL string literal.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn text(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        _ => String::new(),
    }
}

fn int(v: &Value) -> i64 {
    match v {
        Value::Integer(i) => *i,
        _ => 0,
    }
}

fn list(v: &Value) -> Vec<String> {
    text(v)
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn paths(v: &Value) -> Vec<PathBuf> {
    list(v).into_iter().map(PathBuf::from).collect()
}

fn join_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_snapshot_and_answers() {
        let dir = std::env::temp_dir().join(format!("easypp-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let snapshot = Snapshot {
            include_dirs: vec!["/nonexistent/include".into()],
            builtin_dirs: vec![],
            lib_dirs: vec!["/nonexistent/lib".into()],
            default_lib_dirs: vec!["/nonexistent/lib".into()],
            pc_dirs: vec!["/nonexistent/pc".into()],
            packages: vec![Package {
                name: "it's".into(),
                include_dirs: vec!["/x/y".into()],
                cflags: vec!["-I/x/y".into(), "-DA=1".into()],
                libs: vec!["-lfoo".into()],
            }],
            pkg_config: true,
        };
        let mut headers = HashMap::new();
        headers.insert(
            "foo.h".to_string(),
            Some(SysDep::Lib {
                name: "foo".into(),
                dir: Some("/opt/lib".into()),
            }),
        );
        headers.insert("stdio.h".to_string(), None);
        let system = System::new(snapshot.clone(), headers.clone(), HashMap::new());

        {
            let mut cache = Cache::open(&dir).unwrap();
            cache.redetected = true;
            cache.save(&system, IncludeIndex::default()).unwrap();
        }
        let mut cache = Cache::open(&dir).unwrap();
        let (fingerprint, stored) = cache.stored.clone().unwrap();
        assert_eq!(stored, snapshot);
        assert_eq!(fingerprint, snapshot.fingerprint());
        let loaded = cache.system(None);
        assert!(!cache.redetected());
        assert_eq!(loaded.answers().0, headers);
        fs::remove_dir_all(dir).unwrap();
    }
}
