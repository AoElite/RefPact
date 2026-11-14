//! The options that a load of a crate and a load of a workspace share.
//!
//! Each option is written once, here: [`option_methods`] gives its method to
//! [`Options`](crate::Options) and to [`WorkspaceOptions`](crate::WorkspaceOptions).

use crate::rust::cfg;

/// What the shared options say.
#[derive(Debug, Clone, Default)]
pub(crate) struct Common {
    /// Files and directories that no `mod` declaration reaches, and that are accepted.
    pub allowed: Vec<String>,
    /// The method calls and the field accesses are resolved, with rust-analyzer.
    pub members: bool,
    /// A call of a function of a trait is a candidate call of its implementations.
    pub implementations: bool,
    /// The expansions of macros are read.
    pub expansions: bool,
    /// The dependencies of the manifests are references.
    pub dependencies: bool,
    /// The target and the features of the build that the load checks.
    pub settings: cfg::Settings,
}

/// The public methods of the shared options, for a type with a field `common`. `$name` is
/// the type that starts the load, for the examples: `"Crate"` or `"Workspace"`. `$root` is
/// a root module in the examples.
macro_rules! option_methods {
    ($type:ident, $name:literal, $root:literal) => {
        impl $type {
            /// Accepts Rust files below the root directory of a crate that no `mod`
            /// declaration reaches. Each entry is a file or a directory, relative to the
            /// directory of the load. An entry that matches no unreached file is an error.
            pub fn allow_unreached_files<I, S>(mut self, paths: I) -> $type
            where
                I: IntoIterator<Item = S>,
                S: Into<String>,
            {
                let paths = paths.into_iter().map(Into::into);
                self.common.allowed.extend(paths);
                self
            }

            /// Resolves what only the types of the code say: the method that a call names,
            /// the field that an access names, and the item behind `Self::name`, also inside
            /// the arguments of a macro. Each becomes a reference, of the kind
            /// [`Kind::MethodCall`](crate::Kind::MethodCall) or
            /// [`Kind::Field`](crate::Kind::Field), so that `file.read_to_end(..)` is a use
            /// of `std::fs::File::read_to_end`. The names that a macro defines from its
            /// arguments, such as `Order` in `entity!(Order)`, are known paths too.
            ///
            /// Without this option, a load reads the syntax only. Method calls and field
            /// accesses are then no references: `file.read_to_end(..)` is not a use of
            /// `std::fs`, and a rule that names one of the two kinds is an error.
            ///
            /// It is not the default, because the load then runs `rust-analyzer scip` on
            /// the directory of the load, with one index for all crates of a workspace. It
            /// must be installed (`rustup component add rust-analyzer`), and the code must
            /// be code that Cargo can build. The first load takes some seconds; the index is
            /// kept in the `target` directory and is made again only when the sources or the
            /// manifests change. When rust-analyzer is missing, the load fails with
            /// [`LoadError::Index`](crate::LoadError::Index): a rule never passes on one
            /// machine and fails on another because of it.
            ///
            /// ```no_run
            #[doc = concat!("use refpact::", $name, ";")]
            #[doc = concat!("let app = ", $name, "::options(env!(\"CARGO_MANIFEST_DIR\")).resolving_members().load().unwrap();")]
            /// ```
            pub fn resolving_members(mut self) -> $type {
                self.common.members = true;
                self
            }

            /// Checks the code as a build for one target sees it:
            /// `"x86_64-unknown-linux-gnu"`, or `"host"` for the target of this machine.
            ///
            /// By default, the code under every `cfg` is checked, so one run checks all
            /// platforms, and a violation under `#[cfg(windows)]` fails the test on Linux.
            /// With a target, the load leaves out the modules, the items, the statements and
            /// the expressions whose `cfg` condition is false for that target. The options
            /// that the target decides are `unix`, `windows` and those that start with
            /// `target_`, except `target_feature`. The load asks the compiler for them
            /// (`rustc --print cfg`). A condition that the target does not decide, such as
            /// `test` or `debug_assertions`, leaves nothing out.
            ///
            /// With [`resolving_members`](Self::resolving_members), rust-analyzer resolves
            /// the method calls for the target of this machine only.
            ///
            /// ```no_run
            #[doc = concat!("use refpact::", $name, ";")]
            #[doc = concat!("let linux = ", $name, "::options(env!(\"CARGO_MANIFEST_DIR\"))")]
            ///     .target("x86_64-unknown-linux-gnu")
            ///     .load()
            ///     .unwrap();
            /// ```
            pub fn target(mut self, target: impl Into<String>) -> $type {
                let target = target.into();
                self.common.settings.target = Some((target != "host").then_some(target));
                self
            }

            /// Checks the code as a build with these Cargo features sees it, and with no
            /// other feature: code under `#[cfg(feature = "tls")]` is left out unless `tls`
            /// is in the list. The features that `Cargo.toml` turns on by default are not
            /// read; name them. In a workspace, the features are those of all members.
            ///
            /// By default, the code of every feature is checked.
            ///
            /// With [`reading_dependencies`](Self::reading_dependencies), the dependencies
            /// are those of a build with these features too: an optional dependency is there
            /// only when one of the features turns it on. Each feature must then be one
            /// that a manifest declares.
            pub fn features<I, S>(mut self, features: I) -> $type
            where
                I: IntoIterator<Item = S>,
                S: Into<String>,
            {
                let features = features.into_iter().map(Into::into);
                self.common.settings.features = Some(features.collect());
                self
            }

            /// Lets a call of a function of a trait also count as a call of each function
            /// of the loaded code that implements it.
            ///
            /// `store.save(order)`, where `store` is a `&dyn Store` or a `impl Store`, is a
            /// use of `Store::save`. With this option it is also a **candidate** use of
            /// `PgStore::save` and of every other `save` of an `impl Store for ..`, because
            /// the call can run each of them. A rule about what the code can reach, such as
            /// `must_not_reach`, then follows the call into the implementations.
            ///
            /// This turns on [`resolving_members`](Self::resolving_members). It is not the
            /// default, because it turns around what a trait is for: with it, the code that
            /// calls the trait uses the modules that implement it. A rule leaves the
            /// candidates out with `ignoring_candidates()`.
            pub fn following_implementations(mut self) -> $type {
                self.common.implementations = true;
                self
            }

            /// Reads the code that macros write: the expansion of each derive, of each
            /// attribute macro and of each macro call that is not of the standard library.
            ///
            /// The source shows what a macro is given, not what it makes: a derive that
            /// writes a call of `std::fs`, an attribute that wraps a function, the body of a
            /// `macro_rules!`. With this option, the load asks rust-analyzer for each
            /// expansion and reads it as it reads the source. What the expansion holds is at
            /// the place of the macro, with the tag [`TAG_GENERATED`](crate::TAG_GENERATED)
            /// and a note that names the macro. A rule leaves it out with
            /// [`Rule::ignoring_generated`](crate::Rule::ignoring_generated).
            ///
            /// This turns on [`resolving_members`](Self::resolving_members). It is not the
            /// default, because it costs a second run of rust-analyzer, which builds the
            /// procedural macros of the code. The expansions are kept beside the
            /// index, in the `target` directory, until the sources or the manifests change.
            ///
            /// A method call or a field access inside an expansion is not resolved: the
            /// index has no place for it. The paths of an expansion are resolved as the
            /// paths of the module of the macro are.
            pub fn expanding_macros(mut self) -> $type {
                self.common.expansions = true;
                self
            }

            /// Reads the dependencies that the manifests name, and the packages that those
            /// need.
            ///
            /// The code shows which crates a module uses. The manifest shows which packages
            /// a crate is built with, also those that no code names yet. With this option,
            /// each dependency is a reference from the root module of the crate to the crate
            /// of the package, at its line in `Cargo.toml`:
            ///
            /// - [`Kind::Dependency`](crate::Kind::Dependency),
            ///   [`Kind::DevDependency`](crate::Kind::DevDependency) and
            ///   [`Kind::BuildDependency`](crate::Kind::BuildDependency) for what the
            ///   manifest names. The path is the name that the code writes, so `rand-core`
            ///   is `rand_core`; for a renamed dependency, the crate of the package is found
            ///   too.
            /// - [`Kind::TransitiveDependency`](crate::Kind::TransitiveDependency) for each
            ///   other package that a dependency needs, with the packages on the way in the
            ///   note of the violation.
            ///
            /// Each of these references has what Cargo says of its package as properties,
            /// for a check of your own: [`PROPERTY_VERSION`](crate::PROPERTY_VERSION),
            /// [`PROPERTY_LICENSE`](crate::PROPERTY_LICENSE) and
            /// [`PROPERTY_SOURCE`](crate::PROPERTY_SOURCE). Name them in the rule, with
            /// [`Rule::reading_properties`](crate::Rule::reading_properties).
            ///
            /// A dependency for the tests has the tag of test code, so
            /// `except_in_test_code()` leaves it out. A dependency of
            /// `[target.'cfg(windows)'.dependencies]` is under the condition `windows`, as
            /// code under `#[cfg(windows)]` is, and so is each package that only it needs.
            /// The packages are those of a build with all features, or with the features of
            /// [`features`](Self::features).
            ///
            /// In a workspace, the root module of a member is the name of its crate, and a
            /// dependency of one member on another is a use of that member. So the rules
            /// about the members, such as [`layers`](crate::layers), also see a dependency
            /// that no code uses yet.
            ///
            /// ```no_run
            #[doc = concat!("use refpact::{any_of, rule, Kind, ", $name, "};")]
            ///
            #[doc = concat!("let app = ", $name, "::options(env!(\"CARGO_MANIFEST_DIR\"))")]
            ///     .reading_dependencies()
            ///     .load()
            ///     .unwrap();
            /// rule("the code is built without OpenSSL")
            #[doc = concat!("    .modules(any_of([\"", $root, "\"]))")]
            ///     .must_not_use(any_of(["openssl", "openssl_sys"]))
            ///     .only_kinds([Kind::Dependency, Kind::TransitiveDependency])
            ///     .check(&app);
            /// ```
            ///
            /// It is not the default: a rule with a list of what a root module may use would
            /// have to name every package. Cargo then also resolves the dependencies
            /// (`cargo metadata` without `--no-deps`), which takes a fraction of a second
            /// and writes `Cargo.lock` when there is none.
            pub fn reading_dependencies(mut self) -> $type {
                self.common.dependencies = true;
                self
            }
        }
    };
}
