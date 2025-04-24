#![allow(clippy::disallowed_methods)]

use camino::{Utf8Path, Utf8PathBuf};
use std::env;
use std::fs;
use std::process::Command;

const DYLIB_EXTENSION: &str = if cfg!(target_os = "macos") {
    "dylib"
} else if cfg!(target_os = "linux") {
    "so"
} else if cfg!(target_os = "windows") {
    "dll"
} else {
    panic!("Unsupported operating system")
};

#[derive(Clone, Debug)]
struct BuildInfo {
    /// Path to the cargo executable, obtained from the CARGO environment variable.
    cargo: Utf8PathBuf,

    /// Path to the rustc executable, obtained from the RUSTC environment variable.
    rustc: Utf8PathBuf,

    /// The build profile (e.g., "debug" or "release"), obtained from the PROFILE environment variable.
    /// Determines which build configuration to use.
    profile: Profile,

    /// The base directory of the project, typically two levels up from the current directory.
    /// Used as a reference point for locating other project-related paths.
    /// e.g. `/Users/amos/bearcove/home`
    #[allow(dead_code)]
    workspace_dir: Utf8PathBuf,

    /// Where the workspace artifacts are written
    /// e.g. `/Users/amos/bearcove/home/target`
    /// e.g. `/tmp/beardist-build-cache/foo/bar/baz/target`
    workspace_target_dir: Utf8PathBuf,

    /// Path to the rubicon-exports crate directory, located in crates-outside-workspace/rubicon-exports.
    /// Contains the source code for rubicon-exports that needs to be built.
    /// e.g. `/Users/amos/bearcove/home/crates/rubicon-exports`
    rubicon_exports_dir: Utf8PathBuf,

    /// OUT_DIR environment variable
    out_dir: Utf8PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Profile {
    Debug,
    Release,
}

impl Profile {
    fn artifact_dir(self, target_dir: &Utf8Path) -> Utf8PathBuf {
        target_dir.join(match self {
            Self::Debug => "debug",
            Self::Release => "release",
        })
    }
}

impl BuildInfo {
    fn new() -> Self {
        let rustc = Utf8PathBuf::from(env::var("RUSTC").unwrap());
        let cargo = Utf8PathBuf::from(env::var("CARGO").unwrap());
        let profile = match env::var("PROFILE").unwrap().as_str() {
            "debug" => Profile::Debug,
            "release" => Profile::Release,
            _ => panic!("Unsupported profile"),
        };
        let out_dir = Utf8PathBuf::from(env::var("OUT_DIR").unwrap());

        let workspace_dir = Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap())
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let workspace_target_dir = if let Ok(target) = env::var("CARGO_TARGET_DIR") {
            Utf8PathBuf::from(target)
        } else {
            workspace_dir.join("target")
        };

        let rubicon_exports_dir = workspace_dir
            .join("crates-outside-workspace")
            .join("rubicon-exports");

        BuildInfo {
            cargo,
            profile,
            workspace_dir,
            out_dir,
            rustc,
            workspace_target_dir,
            rubicon_exports_dir,
        }
    }

    fn workspace_artifact_dir(&self) -> Utf8PathBuf {
        self.profile.artifact_dir(&self.workspace_target_dir)
    }

    /// Copies `libstd-HASH.{dylib,so,etc.}` into the workspace's artifact directory ($TARGET/$PROFILE).
    /// This will allow running home without `cargo run` — `.cargo/config.toml` sets the RPATH on macOS & Linux
    /// to look for libs there (including libstd).
    fn copy_libstd(&self) {
        let rustc_libdir = Command::new(&self.rustc)
            .arg("--print")
            .arg("target-libdir")
            .output()
            .expect("Failed to execute rustc")
            .stdout;
        let libdir = Utf8PathBuf::from(
            std::str::from_utf8(&rustc_libdir)
                .unwrap()
                .trim()
                .to_string(),
        );

        let suffix = format!(".{}", DYLIB_EXTENSION);
        let libstd_path = libdir
            .read_dir_utf8()
            .unwrap()
            .find_map(|entry| {
                let entry = entry.unwrap();
                let name = entry.file_name();
                println!("examining {name}");
                if name.starts_with("libstd-") && name.ends_with(&suffix) {
                    return Some(entry.into_path());
                }
                None
            })
            .unwrap();

        let libstd_name = libstd_path.file_name().unwrap().to_string();
        let workspace_libstd_path = self.workspace_artifact_dir().join(&libstd_name);
        copy_file(&libstd_path, &workspace_libstd_path);
    }
}

fn main() {
    let build_info = BuildInfo::new();
    let build_info = Box::leak(Box::new(build_info));
    println!("{:?}", build_info);

    let copy_libstd_handle = std::thread::spawn(|| {
        build_info.copy_libstd();
    });

    copy_libstd_handle
        .join()
        .expect("Libstd symlink thread panicked");
}

fn copy_file(source: &Utf8Path, destination: &Utf8Path) {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("Failed to create parent directory {}: {e}", parent);
        });
    }

    if let Ok(_meta) = destination.symlink_metadata() {
        eprintln!("Destination file exists (or is a symlink) removing it: {destination}");
        fs::remove_file(destination).unwrap_or_else(|e| {
            eprintln!("Failed to remove existing file: {e}");
        });
    }

    fs::copy(source, destination).unwrap_or_else(|e| {
        panic!(
            "Failed to copy file from {:?} to {:?}: {}",
            source, destination, e
        );
    });
}
