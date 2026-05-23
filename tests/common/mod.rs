// Helpers shared across integration test files. Each tests/*.rs is its own
// crate, so a method only used by some files looks "dead" to the others;
// silence that here rather than scattering allow(dead_code) per use site.
#![allow(dead_code)]

use std::path::PathBuf;

use assert_cmd::Command;
use tempfile::TempDir;

pub struct Fixture {
    pub cfg_dir: TempDir,
    pub work_dir: TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            cfg_dir: tempfile::tempdir().expect("create cfg tempdir"),
            work_dir: tempfile::tempdir().expect("create work tempdir"),
        }
    }

    pub fn cfg_path(&self) -> &std::path::Path {
        self.cfg_dir.path()
    }

    pub fn lib_path(&self, name: &str) -> PathBuf {
        self.work_dir.path().join(name)
    }

    pub fn cdx(&self) -> Command {
        let mut c = Command::cargo_bin("cdx").expect("cdx binary built by cargo");
        c.arg("--data-dir").arg(self.cfg_path());
        c
    }

    pub fn cdx_json(&self) -> Command {
        let mut c = self.cdx();
        c.arg("--json");
        c
    }

    /// Initialize a catalog named `lib` at `work_dir/lib` and set it as current.
    /// Returns the absolute path of the catalog directory.
    pub fn init_lib(&self) -> PathBuf {
        let lib = self.lib_path("lib");
        self.cdx()
            .args(["catalog", "init", "lib"])
            .arg(&lib)
            .assert()
            .success();
        lib
    }

    pub fn fixture(name: &str) -> PathBuf {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest.join("tests").join("fixtures").join(name)
    }
}
