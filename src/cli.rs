use crate::builder;
use clap::{Args, Parser, Subcommand, value_parser};
use std::ffi::OsString;
use std::num::NonZeroUsize;

#[derive(Parser)]
#[command(
    version,
    about = "Build, run, and test C and C++ projects from their folder layout",
    after_help = "Start here:\n  easypp new hello\n  cd hello\n  easypp run\n\nFind more help with `easypp help <command>` or `easypp help deps add`."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Actions>,
    #[command(flatten)]
    pub build: BuildArgs,
    #[arg(long, hide = true)]
    pub watch: bool,
}

#[derive(Args)]
pub struct BuildArgs {
    /// Select the optimized Release profile. Debug is the default.
    #[arg(short, long, global = true)]
    release: bool,
    /// Set the number of parallel build and test jobs (default: available CPUs).
    #[arg(short, long, global = true, value_name = "N")]
    jobs: Option<NonZeroUsize>,
    /// Print compiler and linker commands.
    #[arg(short, long, global = true)]
    verbose: bool,
    /// Ignore the discovery cache for this invocation.
    #[arg(long, global = true)]
    no_cache: bool,
    /// Disable AddressSanitizer and UBSan in Debug builds.
    #[arg(long, global = true)]
    no_sanitize: bool,
}

impl From<BuildArgs> for builder::Options {
    fn from(args: BuildArgs) -> Self {
        Self {
            release: args.release,
            jobs: args
                .jobs
                .or_else(|| std::thread::available_parallelism().ok())
                .map_or(1, NonZeroUsize::get),
            verbose: args.verbose,
            cache: !args.no_cache,
            sanitize: !args.no_sanitize,
        }
    }
}

#[derive(Subcommand)]
pub enum Actions {
    /// Create a new project with an example program and test.
    #[command(after_help = "Example: easypp new hello")]
    New {
        /// Directory to create.
        name: String,
    },
    /// Create the easypp folder layout in the current directory.
    Init,
    /// Compile the project's libraries and executables.
    #[command(
        after_help = "Example: easypp build --release -j4\n\nIf conanfile.txt exists, missing or stale Conan dependencies are installed first."
    )]
    Build,
    /// Build and run an executable from src/bin.
    #[command(after_help = "Example: easypp run hello -- --greeting Alice")]
    Run(ProgramArgs),
    /// Build and debug a program or test with LLDB or GDB.
    #[command(after_help = "Example: easypp debug hello")]
    Debug(ProgramArgs),
    /// Build and run tests from src/tests.
    #[command(after_help = "Example: easypp test zipjson -- --gtest_filter=ZipJson.*")]
    Test(TestArgs),
    /// Manage Conan dependencies recorded in conanfile.txt.
    #[command(
        after_help = "Find more help with `easypp help deps add` or `easypp help deps install`."
    )]
    Deps {
        #[command(subcommand)]
        action: DependencyAction,
    },
    /// Show discovered targets, dependencies, and code generators.
    Info,
    /// Format the project's code with clang-format.
    Fmt {
        /// Report formatting differences without editing files.
        #[arg(long)]
        check: bool,
    },
    /// Check the project's code with clang-tidy.
    Lint,
    /// Generate a CMakeLists.txt for the discovered project.
    ExportCmake {
        /// Replace an existing CMakeLists.txt.
        #[arg(long)]
        force: bool,
    },
    /// Update compile_commands.json and generated sources as files change.
    Watch,
    /// Remove build artifacts and generated dependency metadata.
    Clean,
}

#[derive(Args)]
pub struct ProgramArgs {
    /// Target name. Optional if the project has a single executable.
    pub name: Option<String>,
    /// Arguments passed to the program after --.
    #[arg(last = true, value_name = "ARGS")]
    pub args: Vec<OsString>,
}

#[derive(Args)]
pub struct TestArgs {
    /// Run only test executables whose names contain this text.
    pub filter: Option<String>,
    /// Time limit per test, in seconds.
    #[arg(long, default_value_t = 60, value_name = "SECS", value_parser = value_parser!(u64).range(1..))]
    pub timeout: u64,
    /// Arguments passed to each test executable after --.
    #[arg(last = true, value_name = "ARGS")]
    pub args: Vec<String>,
}

#[derive(Subcommand)]
pub enum DependencyAction {
    /// Add or update package references and install them.
    #[command(after_help = "Example: easypp deps add zlib/1.3.1 --release")]
    Add {
        /// Conan package references, such as zlib/1.3.1.
        #[arg(required = true, value_name = "NAME/VERSION")]
        references: Vec<String>,
        #[command(flatten)]
        install: ConanArgs,
    },
    /// Install dependencies already recorded in conanfile.txt.
    #[command(after_help = "Example: easypp deps install --release --build-missing")]
    Install(ConanArgs),
}

#[derive(Args)]
pub struct ConanArgs {
    /// Ask Conan to build package binaries that are unavailable.
    #[arg(long)]
    build_missing: bool,
    #[arg(last = true, hide = true, value_parser = ["--build=missing"])]
    args: Vec<String>,
}

impl ConanArgs {
    pub fn into_flags(mut self) -> Vec<String> {
        if self.build_missing && self.args.is_empty() {
            self.args.push("--build=missing".into());
        }
        self.args
    }
}
