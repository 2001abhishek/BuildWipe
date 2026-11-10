//! What counts as a removable build artifact. These rules are the same on
//! every OS. Where the scanner must not go is OS-specific and lives in
//! `platform`.
//!
//! A folder name alone is never enough for ambiguous names: `target`, `build`,
//! `bin`, `Library` and friends only match when the files beside them prove
//! which toolchain produced them.

use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Node,
    Rust,
    Python,
    Jvm,
    Dotnet,
    Mobile,
    Vendor,
    GameEngine,
    Terraform,
    OsJunk,
}

/// Lower-cased names of everything in one directory.
pub struct Listing {
    names: HashSet<String>,
}

impl Listing {
    pub fn new(names: impl IntoIterator<Item = String>) -> Self {
        Self {
            names: names.into_iter().map(|n| n.to_lowercase()).collect(),
        }
    }

    pub fn read(dir: &Path) -> std::io::Result<Self> {
        let names = fs::read_dir(dir)?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned());
        Ok(Self::new(names))
    }

    pub fn has(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    fn has_extension(&self, extension: &str) -> bool {
        self.names.iter().any(|n| n.ends_with(extension))
    }
}

enum Marker {
    /// The name is distinctive enough to match on its own.
    Always,
    /// At least one of these must sit beside the folder.
    AnyOf(&'static [&'static str]),
    /// All of these must sit beside the folder.
    AllOf(&'static [&'static str]),
    /// A file ending in one of these suffixes must sit beside the folder.
    Extension(&'static [&'static str]),
}

impl Marker {
    fn matches(&self, listing: &Listing) -> bool {
        match self {
            Marker::Always => true,
            Marker::AnyOf(names) => names.iter().any(|n| listing.has(n)),
            Marker::AllOf(names) => names.iter().all(|n| listing.has(n)),
            Marker::Extension(suffixes) => suffixes.iter().any(|s| listing.has_extension(s)),
        }
    }
}

/// Why a matched folder deserves a second look before it is removed.
const NOTE_VENDOR: &str =
    "Vendored dependencies are sometimes committed on purpose. Check before removing.";
const NOTE_DIST: &str = "Usually build output, but some projects keep hand-written files here.";

struct DirRule {
    name: &'static str,
    /// Set when the artifact is a subfolder of `name`, such as `var/cache`.
    nested: Option<&'static str>,
    category: Category,
    marker: Marker,
    note: Option<&'static str>,
}

const fn rule(name: &'static str, category: Category, marker: Marker) -> DirRule {
    DirRule {
        name,
        nested: None,
        category,
        marker,
        note: None,
    }
}

const NODE: Marker = Marker::AnyOf(&["package.json"]);
const GRADLE: Marker = Marker::AnyOf(&[
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
]);
const FLUTTER: Marker = Marker::AnyOf(&["pubspec.yaml"]);
const DOTNET: Marker = Marker::Extension(&[".csproj", ".fsproj", ".vbproj"]);
const UNITY: Marker = Marker::AllOf(&["assets", "projectsettings"]);
const UNREAL: Marker = Marker::Extension(&[".uproject"]);

static DIR_RULES: &[DirRule] = &[
    // Node.js and its frameworks
    rule("node_modules", Category::Node, NODE),
    rule(".next", Category::Node, NODE),
    rule(".nuxt", Category::Node, NODE),
    rule(
        ".output",
        Category::Node,
        Marker::AnyOf(&["nuxt.config.ts", "nuxt.config.js", "nuxt.config.mjs"]),
    ),
    rule(".angular", Category::Node, Marker::AnyOf(&["angular.json"])),
    rule(".svelte-kit", Category::Node, NODE),
    rule(".turbo", Category::Node, NODE),
    rule(".parcel-cache", Category::Node, NODE),
    DirRule {
        note: Some(NOTE_DIST),
        ..rule("dist", Category::Node, NODE)
    },
    // Rust
    rule("target", Category::Rust, Marker::AnyOf(&["cargo.toml"])),
    // Python (virtualenvs are detected through pyvenv.cfg, see `is_virtualenv`)
    rule("__pycache__", Category::Python, Marker::Always),
    rule(".pytest_cache", Category::Python, Marker::Always),
    rule(".mypy_cache", Category::Python, Marker::Always),
    rule(".ruff_cache", Category::Python, Marker::Always),
    rule(".tox", Category::Python, Marker::AnyOf(&["tox.ini", "pyproject.toml", "setup.py"])),
    // Java and Kotlin
    rule("target", Category::Jvm, Marker::AnyOf(&["pom.xml"])),
    rule("build", Category::Jvm, GRADLE),
    rule(".gradle", Category::Jvm, GRADLE),
    // .NET
    rule("bin", Category::Dotnet, DOTNET),
    rule("obj", Category::Dotnet, DOTNET),
    // Flutter and iOS
    rule("build", Category::Mobile, FLUTTER),
    rule(".dart_tool", Category::Mobile, FLUTTER),
    rule("Pods", Category::Mobile, Marker::AnyOf(&["podfile"])),
    // Go, PHP and Ruby
    DirRule {
        note: Some(NOTE_VENDOR),
        ..rule(
            "vendor",
            Category::Vendor,
            Marker::AnyOf(&["go.mod", "composer.json", "gemfile"]),
        )
    },
    DirRule {
        nested: Some("cache"),
        ..rule("var", Category::Vendor, Marker::AnyOf(&["composer.json"]))
    },
    // Unity and Unreal
    rule("Library", Category::GameEngine, UNITY),
    rule("Temp", Category::GameEngine, UNITY),
    rule("Intermediate", Category::GameEngine, UNREAL),
    rule("DerivedDataCache", Category::GameEngine, UNREAL),
    // Terraform
    rule(".terraform", Category::Terraform, Marker::Extension(&[".tf"])),
];

/// Files that mark a folder as the root of a project, used only for grouping.
const PROJECT_FILES: &[&str] = &[
    "package.json",
    "cargo.toml",
    "pyproject.toml",
    "requirements.txt",
    "setup.py",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "pubspec.yaml",
    "podfile",
    "go.mod",
    "composer.json",
    "gemfile",
];
const PROJECT_EXTENSIONS: &[&str] = &[".csproj", ".fsproj", ".vbproj", ".sln", ".uproject", ".tf"];

pub fn is_project_root(listing: &Listing) -> bool {
    PROJECT_FILES.iter().any(|f| listing.has(f))
        || PROJECT_EXTENSIONS.iter().any(|e| listing.has_extension(e))
        || UNITY.matches(listing)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirMatch {
    pub category: Category,
    pub nested: Option<&'static str>,
    pub note: Option<&'static str>,
}

/// Decides whether a child folder called `name` is an artifact, given what sits beside it.
pub fn match_dir(name: &str, parent: &Listing) -> Option<DirMatch> {
    DIR_RULES
        .iter()
        .find(|r| r.name == name && r.marker.matches(parent))
        .map(|r| DirMatch {
            category: r.category,
            nested: r.nested,
            note: r.note,
        })
}

pub fn is_junk_file(name: &str) -> bool {
    name.eq_ignore_ascii_case(".DS_Store") || name.eq_ignore_ascii_case("Thumbs.db")
}

pub fn is_virtualenv(listing: &Listing) -> bool {
    listing.has("pyvenv.cfg")
}

#[derive(Debug, PartialEq, Eq)]
pub struct Classified {
    pub category: Category,
    pub note: Option<&'static str>,
}

/// Re-derives from disk what `path` is. The wipe calls this immediately before
/// deleting, so a folder that changed since the scan is refused.
pub fn classify_path(path: &Path) -> Option<Classified> {
    let name = path.file_name()?.to_str()?;
    let parent = path.parent()?;
    let meta = fs::symlink_metadata(path).ok()?;

    if meta.file_type().is_symlink() {
        return None;
    }
    if meta.is_file() {
        return is_junk_file(name).then_some(Classified {
            category: Category::OsJunk,
            note: None,
        });
    }

    if let Ok(listing) = Listing::read(path) {
        if is_virtualenv(&listing) {
            return Some(Classified {
                category: Category::Python,
                note: None,
            });
        }
    }

    if name == "cache" && parent.file_name().is_some_and(|p| p == "var") {
        let listing = Listing::read(parent.parent()?).ok()?;
        let found = match_dir("var", &listing)?;
        return found.nested.map(|_| Classified {
            category: found.category,
            note: found.note,
        });
    }

    let listing = Listing::read(parent).ok()?;
    let found = match_dir(name, &listing).filter(|m| m.nested.is_none())?;
    Some(Classified {
        category: found.category,
        note: found.note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beside(names: &[&str]) -> Listing {
        Listing::new(names.iter().map(|n| n.to_string()))
    }

    fn category(name: &str, names: &[&str]) -> Option<Category> {
        match_dir(name, &beside(names)).map(|m| m.category)
    }

    #[test]
    fn ambiguous_names_need_their_toolchain_beside_them() {
        for name in ["target", "build", "bin", "obj", "vendor", "dist", "Library", "Temp"] {
            assert_eq!(category(name, &["readme.md"]), None, "{name} matched alone");
        }
    }

    #[test]
    fn the_same_name_resolves_to_the_right_toolchain() {
        assert_eq!(category("target", &["Cargo.toml"]), Some(Category::Rust));
        assert_eq!(category("target", &["pom.xml"]), Some(Category::Jvm));
        assert_eq!(category("build", &["build.gradle.kts"]), Some(Category::Jvm));
        assert_eq!(category("build", &["pubspec.yaml"]), Some(Category::Mobile));
        assert_eq!(category("bin", &["App.csproj"]), Some(Category::Dotnet));
        assert_eq!(category("vendor", &["go.mod"]), Some(Category::Vendor));
        assert_eq!(
            category("Library", &["Assets", "ProjectSettings"]),
            Some(Category::GameEngine)
        );
        assert_eq!(category("Intermediate", &["Game.uproject"]), Some(Category::GameEngine));
        assert_eq!(category(".terraform", &["main.tf"]), Some(Category::Terraform));
    }

    #[test]
    fn unity_requires_both_marker_folders() {
        assert_eq!(category("Library", &["Assets"]), None);
    }

    #[test]
    fn node_modules_without_a_package_json_is_left_alone() {
        // Global installs like nvm's node_modules have no package.json beside them.
        assert_eq!(category("node_modules", &["node.exe"]), None);
        assert_eq!(category("node_modules", &["package.json"]), Some(Category::Node));
    }

    #[test]
    fn the_home_gradle_cache_is_not_a_project_artifact() {
        assert_eq!(category(".gradle", &[".bashrc", "documents"]), None);
    }

    #[test]
    fn folder_names_are_case_sensitive() {
        assert_eq!(category("Target", &["Cargo.toml"]), None);
    }

    #[test]
    fn items_that_may_hold_real_work_carry_a_note() {
        let vendor = match_dir("vendor", &beside(&["go.mod"])).unwrap();
        assert!(vendor.note.is_some());
        let dist = match_dir("dist", &beside(&["package.json"])).unwrap();
        assert!(dist.note.is_some());
        let modules = match_dir("node_modules", &beside(&["package.json"])).unwrap();
        assert!(modules.note.is_none());
    }

    #[test]
    fn junk_files_match_in_any_case() {
        assert!(is_junk_file(".DS_Store"));
        assert!(is_junk_file("thumbs.db"));
        assert!(!is_junk_file("desktop.ini"));
    }
}
