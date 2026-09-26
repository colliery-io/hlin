//! A widget's two Trunk builds, and where each is read from.
//!
//! A converted widget has two `dist`s: its own UI (`ui/dist`), served at the
//! root, and its Hlin module (`module/dist`), served under Hlin's base. Each is
//! read, in order of preference, from:
//!
//! 1. the directory a flag names (`--ui-dir`, `--module-dir`);
//! 2. the binary itself, when the widget is built with its `embed` feature,
//!    which is how a container ships one file ([[HLIN-I-0013]]);
//! 3. the build's directory in this repository, which is development: a
//!    checkout builds and tests without Trunk, and a widget started before its
//!    UI was built says so and serves the rest.
//!
//! [`dist!`](crate::dist!) makes a [`Dist`] in the widget's own crate, because
//! both what is embedded and whether to embed it are the widget crate's: the
//! folder is relative to its manifest, and `embed` is its feature.

use std::path::{Path, PathBuf};

use crate::files::ModuleFiles;

/// The files of a build compiled into the binary, by name.
pub type Embedded = fn() -> Vec<(String, Vec<u8>)>;

/// One Trunk build.
#[derive(Debug, Clone, Copy)]
pub struct Dist {
    /// Where Trunk writes it, in this repository.
    pub dir: &'static str,
    /// The same files compiled into the binary, when they are.
    pub embedded: Option<Embedded>,
}

/// A widget's two builds.
#[derive(Debug, Clone, Copy)]
pub struct Builds {
    /// Its own UI, served at the root.
    pub ui: Dist,
    /// Its Hlin module, served under Hlin's base.
    pub module: Dist,
}

impl Dist {
    /// A build on disk only, at `dir`.
    pub const fn at(dir: &'static str) -> Self {
        Self {
            dir,
            embedded: None,
        }
    }

    /// The build's files, from `flag` if given, else embedded, else `dir`;
    /// and where they came from, for the log. `None` where there is no build
    /// with an `index.html`.
    pub fn files(&self, flag: Option<&Path>) -> Option<(ModuleFiles, String)> {
        let read = |dir: &Path| {
            ModuleFiles::read(dir)
                .ok()
                .filter(ModuleFiles::has_entry)
                .map(|files| (files, dir.display().to_string()))
        };
        if let Some(dir) = flag {
            return read(dir);
        }
        if let Some(embedded) = self.embedded {
            let files = ModuleFiles::from_files(embedded());
            return files.has_entry().then(|| (files, "the binary".to_string()));
        }
        read(&PathBuf::from(self.dir))
    }
}

/// The folder's files as a [`Dist`]: embedded when the calling crate is built
/// with its `embed` feature, read from disk otherwise.
///
/// ```text
/// hlin_widget_support::dist!("ui/dist")
/// ```
///
/// The folder is relative to the calling crate's `Cargo.toml`, which must
/// declare `embed = ["hlin-widget-support/embed"]`. With `embed` on, the
/// folder must be built first, since its files are compiled in.
#[macro_export]
macro_rules! dist {
    ($folder:tt) => {{
        #[cfg(feature = "embed")]
        let dist = {
            #[derive($crate::rust_embed::Embed)]
            #[crate_path = "hlin_widget_support::rust_embed"]
            #[folder = $folder]
            struct Files;

            fn files() -> ::std::vec::Vec<(::std::string::String, ::std::vec::Vec<u8>)> {
                Files::iter()
                    .filter_map(|name| {
                        Files::get(&name).map(|file| (name.to_string(), file.data.into_owned()))
                    })
                    .collect()
            }

            $crate::Dist {
                dir: concat!(env!("CARGO_MANIFEST_DIR"), "/", $folder),
                embedded: Some(files),
            }
        };
        #[cfg(not(feature = "embed"))]
        let dist = $crate::Dist::at(concat!(env!("CARGO_MANIFEST_DIR"), "/", $folder));
        dist
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("widget-dist-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for file in files {
            std::fs::write(dir.join(file), "x").unwrap();
        }
        dir
    }

    #[test]
    fn a_directory_named_by_a_flag_wins_over_the_embedded_build() {
        let dir = built("flag", &["index.html"]);
        let dist = Dist {
            dir: "/nowhere",
            embedded: Some(|| vec![("index.html".to_string(), b"embedded".to_vec())]),
        };
        let (_, from) = dist.files(Some(&dir)).unwrap();
        assert_eq!(from, dir.display().to_string());
        let (_, from) = dist.files(None).unwrap();
        assert_eq!(from, "the binary");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_build_without_an_entry_is_no_build() {
        let dir = built("entryless", &["boot.js"]);
        assert!(Dist::at("/nowhere").files(Some(&dir)).is_none());
        assert!(Dist::at("/nowhere").files(None).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
