use std::{collections::HashMap, fs, io, path::Path, rc::Rc};

use chialisp::{
    classic::clvm_tools::clvmc::compile_clvm_text,
    compiler::{compiler::DefaultCompilerOpts, comptypes::CompilerOpts},
};
use clvm_traits::ToClvmError;
use clvm_utils::{TreeHash, tree_hash};
use clvmr::{Allocator, error::EvalErr, serde::node_to_bytes};
use rue_compiler::{Compiler, FileTree, normalize_path};
use rue_options::find_project;
use thiserror::Error;

/// An error that can occur when compiling a Chialisp or Rue program with
/// [`compile_chialisp`](fn@compile_chialisp) or [`compile_rue`](fn@compile_rue).
#[derive(Debug, Error)]
pub enum LoadClvmError {
    /// The source file or project couldn't be read.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// The compiled program couldn't be serialized.
    #[error("CLVM error: {0}")]
    Clvm(#[from] EvalErr),

    /// The path has no file name, isn't valid UTF-8, or has no parent directory.
    #[error("Invalid file name")]
    InvalidFileName,

    /// The Chialisp compiler failed, with its error message.
    #[error("Compiler error: {0}")]
    Compiler(String),

    /// A value couldn't be converted to CLVM.
    #[error("Conversion error: {0}")]
    Conversion(#[from] ToClvmError),

    /// The Rue project configuration is invalid.
    #[error("Project error: {0}")]
    Project(#[from] rue_options::Error),

    /// No Rue project or `.rue` file was found at the given path.
    #[error("Project not found")]
    ProjectNotFound,

    /// No export name was given, and the Rue project has no `main` function.
    #[error("Main not found")]
    MainNotFound,

    /// The Rue project has no export with the given name.
    #[error("Export not found: {0}")]
    ExportNotFound(String),

    /// The Rue compiler failed.
    #[error("Rue error: {0}")]
    Rue(#[from] rue_compiler::Error),
}

/// A compiled CLVM program, as returned by [`compile_chialisp`](fn@compile_chialisp) and
/// [`compile_rue`](fn@compile_rue).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Compilation {
    /// The serialized CLVM bytecode of the program.
    pub reveal: Vec<u8>,
    /// The tree hash of the program, which is its puzzle hash (or mod hash, before currying).
    pub hash: TreeHash,
}

/// Compiles a Chialisp source file into CLVM bytecode.
///
/// The `include_paths` are the directories searched for files referenced with `(include ...)`.
/// Relative paths are resolved against the current working directory.
///
/// This is useful for prototyping puzzles or for tests. Production code usually embeds the
/// compiled bytecode instead, so the compiler doesn't need to run.
///
/// ## Example
///
/// ```rust
/// # use std::path::Path;
/// # use chia_sdk_types::{compile_chialisp, run_puzzle};
/// # use clvm_traits::{FromClvm, ToClvm, clvm_curried_args};
/// # use clvm_utils::CurriedProgram;
/// # use clvmr::{Allocator, NodePtr, serde::node_from_bytes};
/// // The file contains `(mod (AAA BBB) (+ (* AAA BBB) AAA BBB))`.
/// let compilation = compile_chialisp(Path::new("compile_chialisp_test.clsp"), &[])?;
///
/// let mut allocator = Allocator::new();
/// let program = node_from_bytes(&mut allocator, &compilation.reveal)?;
/// assert_eq!(clvm_utils::tree_hash(&allocator, program), compilation.hash);
///
/// let args = clvm_curried_args!(10, 20);
/// let puzzle = CurriedProgram { program, args }.to_clvm(&mut allocator)?;
/// let output = run_puzzle(&mut allocator, puzzle, NodePtr::NIL)?;
/// assert_eq!(u64::from_clvm(&allocator, output)?, 230);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn compile_chialisp(
    path: &Path,
    include_paths: &[String],
) -> Result<Compilation, LoadClvmError> {
    let mut allocator = Allocator::new();

    let opts = Rc::new(DefaultCompilerOpts::new(
        path.file_name()
            .ok_or(LoadClvmError::InvalidFileName)?
            .to_str()
            .ok_or(LoadClvmError::InvalidFileName)?,
    ))
    .set_search_paths(include_paths);

    let text = fs::read_to_string(path)?;

    let ptr = compile_clvm_text(
        &mut allocator,
        opts,
        &mut HashMap::new(),
        &text,
        path.to_str().ok_or(LoadClvmError::InvalidFileName)?,
        false,
    )
    .map_err(|error| LoadClvmError::Compiler(format!("{error:?}")))?;

    let hash = tree_hash(&allocator, ptr);
    let reveal = node_to_bytes(&allocator, ptr)?;

    Ok(Compilation { reveal, hash })
}

/// Compiles a Rue program into CLVM bytecode.
///
/// The `path` can be a `.rue` file, or a file or directory inside a Rue project (a directory with
/// a `Rue.toml` manifest or a `main.rue` file). The nearest enclosing project is used. If
/// `debug` is `true`, the program is compiled with Rue's debug compiler options.
///
/// If `export_name` is given, that export is compiled. Otherwise, the `main` function of the
/// project's `main.rue` (or of the file itself) is compiled.
///
/// ## Example
///
/// ```rust
/// # use std::path::Path;
/// # use chia_sdk_types::{compile_rue, run_puzzle};
/// # use clvm_traits::{FromClvm, ToClvm, clvm_curried_args};
/// # use clvm_utils::CurriedProgram;
/// # use clvmr::{Allocator, NodePtr, serde::node_from_bytes};
/// // The file has `main` and `another` functions, which both take `a` and `b` and
/// // return `a * b + a + b`.
/// let main = compile_rue(Path::new("compile_rue_test.rue"), false, None)?;
/// let another = compile_rue(Path::new("compile_rue_test.rue"), false, Some("another"))?;
/// assert_eq!(main.reveal, another.reveal);
///
/// let mut allocator = Allocator::new();
/// let program = node_from_bytes(&mut allocator, &main.reveal)?;
/// let args = clvm_curried_args!(10, 20);
/// let puzzle = CurriedProgram { program, args }.to_clvm(&mut allocator)?;
/// let output = run_puzzle(&mut allocator, puzzle, NodePtr::NIL)?;
/// assert_eq!(u64::from_clvm(&allocator, output)?, 230);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn compile_rue(
    path: &Path,
    debug: bool,
    export_name: Option<&str>,
) -> Result<Compilation, LoadClvmError> {
    let mut allocator = Allocator::new();

    let path = path.canonicalize()?;
    let project = find_project(&path, debug)?;

    let Some(project) = project else {
        return Err(LoadClvmError::ProjectNotFound);
    };

    let main_kind = if project.entrypoint.join("main.rue").exists() {
        Some(normalize_path(&project.entrypoint.join("main.rue"))?)
    } else {
        None
    };

    let mut ctx = Compiler::new(project.options);

    let tree = FileTree::compile_path(&mut ctx, &project.entrypoint, &mut HashMap::new())?;
    let base_path = if project.entrypoint.is_file() {
        project
            .entrypoint
            .parent()
            .ok_or(LoadClvmError::InvalidFileName)?
            .canonicalize()?
    } else {
        project.entrypoint.canonicalize()?
    };

    let ptr = if let Some(export_name) = export_name {
        if let Some(export) = tree
            .exports(
                &mut ctx,
                &mut allocator,
                main_kind.as_ref(),
                Some(export_name),
                &base_path,
            )?
            .into_iter()
            .next()
        {
            export.ptr
        } else {
            return Err(LoadClvmError::ExportNotFound(export_name.to_string()));
        }
    } else if let Some(main_kind) = main_kind
        && let Some(main) = tree.main(&mut ctx, &mut allocator, &main_kind, base_path.clone())?
    {
        main
    } else if let Some(main) =
        tree.main(&mut ctx, &mut allocator, &normalize_path(&path)?, base_path)?
    {
        main
    } else {
        return Err(LoadClvmError::MainNotFound);
    };

    let hash = tree_hash(&allocator, ptr);
    let reveal = node_to_bytes(&allocator, ptr)?;

    Ok(Compilation { reveal, hash })
}

/// Implements [`Mod`](crate::Mod) for a puzzle arguments type by compiling a Chialisp file.
///
/// The syntax is `compile_chialisp!(ArgsType<Generics> = STATIC_NAME, "path/to/file.clsp")`. This
/// declares a static named `STATIC_NAME` that lazily compiles the file with
/// [`compile_chialisp`](fn@compile_chialisp) the first time it's used, with `.` and `include` as
/// the include paths. The path and include paths are relative to the working directory at runtime,
/// not to the source file.
///
/// The macro expands to code that refers to [`Compilation`](crate::Compilation) without a path,
/// so it must be imported where the macro is used.
///
/// # Panics
///
/// Using the [`Mod`](crate::Mod) implementation panics if the file fails to compile.
///
/// ## Example
///
/// ```rust
/// # use chia_sdk_types::{Compilation, Mod, compile_chialisp, run_puzzle};
/// # use clvm_traits::{FromClvm, ToClvm};
/// # use clvm_utils::CurriedProgram;
/// # use clvmr::{Allocator, NodePtr, serde::node_from_bytes};
/// #[derive(Debug, Clone, ToClvm, FromClvm)]
/// #[clvm(curry)]
/// struct TestArgs {
///     a: u64,
///     b: u64,
/// }
///
/// // The file contains `(mod (AAA BBB) (+ (* AAA BBB) AAA BBB))`.
/// compile_chialisp!(TestArgs = TEST_MOD, "compile_chialisp_test.clsp");
///
/// let mut allocator = Allocator::new();
/// let program = node_from_bytes(&mut allocator, &TestArgs::mod_reveal())?;
/// let args = TestArgs { a: 10, b: 20 };
///
/// let puzzle = CurriedProgram { program, args: args.clone() }.to_clvm(&mut allocator)?;
/// assert_eq!(clvm_utils::tree_hash(&allocator, puzzle), args.curry_tree_hash());
///
/// let output = run_puzzle(&mut allocator, puzzle, NodePtr::NIL)?;
/// assert_eq!(u64::from_clvm(&allocator, output)?, 230);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[macro_export]
macro_rules! compile_chialisp {
    ( $args:ident $(< $($generic:ident),+ >)? = $mod_name:ident, $path:literal ) => {
        static $mod_name: ::std::sync::LazyLock<Compilation> =
            ::std::sync::LazyLock::new(|| $crate::compile_chialisp(::std::path::Path::new($path), &[".".to_string(), "include".to_string()]).unwrap());

        impl$(<$($generic),+>)? $crate::Mod for $args$(<$($generic),+>)? {
            fn mod_reveal() -> ::std::borrow::Cow<'static, [u8]> {
                ::std::borrow::Cow::Owned($mod_name.reveal.clone())
            }

            fn mod_hash() -> $crate::__internals::TreeHash {
                $mod_name.hash
            }
        }
    };
}

/// Implements [`Mod`](crate::Mod) for a puzzle arguments type by compiling a Rue program.
///
/// The syntax is `compile_rue!(ArgsType<Generics> = STATIC_NAME, "path")`, optionally followed by
/// an export name (`, "export_name"`), and optionally prefixed with `debug` to compile with debug
/// options. This declares a static named `STATIC_NAME` that lazily compiles the program with
/// [`compile_rue`](fn@compile_rue) the first time it's used. The path is relative to the working
/// directory at runtime, not to the source file.
///
/// The macro expands to code that refers to [`Compilation`](crate::Compilation) without a path,
/// so it must be imported where the macro is used.
///
/// # Panics
///
/// Using the [`Mod`](crate::Mod) implementation panics if the program fails to compile.
///
/// ## Example
///
/// ```rust
/// # use chia_sdk_types::{Compilation, Mod, compile_rue, run_puzzle};
/// # use clvm_traits::{FromClvm, ToClvm};
/// # use clvm_utils::CurriedProgram;
/// # use clvmr::{Allocator, NodePtr, serde::node_from_bytes};
/// #[derive(Debug, Clone, ToClvm, FromClvm)]
/// #[clvm(curry)]
/// struct TestArgs {
///     a: u64,
///     b: u64,
/// }
///
/// // The `another` export takes `a` and `b` and returns `a * b + a + b`.
/// compile_rue!(TestArgs = TEST_MOD, "compile_rue_test.rue", "another");
///
/// let mut allocator = Allocator::new();
/// let program = node_from_bytes(&mut allocator, &TestArgs::mod_reveal())?;
/// let args = TestArgs { a: 10, b: 20 };
/// let puzzle = CurriedProgram { program, args }.to_clvm(&mut allocator)?;
///
/// let output = run_puzzle(&mut allocator, puzzle, NodePtr::NIL)?;
/// assert_eq!(u64::from_clvm(&allocator, output)?, 230);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[macro_export]
macro_rules! compile_rue {
    ( $args:ident $(< $($generic:ident),+ >)? = $mod_name:ident, $path:literal ) => {
        $crate::compile_rue!(@impl $args $(<$($generic),+>)? = $mod_name $path false None);
    };

    ( $args:ident $(< $($generic:ident),+ >)? = $mod_name:ident, $path:literal, $export_name:literal ) => {
        $crate::compile_rue!(@impl $args $(<$($generic),+>)? = $mod_name $path false Some($export_name));
    };

    ( debug $args:ident $(< $($generic:ident),+ >)? = $mod_name:ident, $path:literal ) => {
        $crate::compile_rue!(@impl $args $(<$($generic),+>)? = $mod_name $path true None);
    };

    ( debug $args:ident $(< $($generic:ident),+ >)? = $mod_name:ident, $path:literal, $export_name:literal ) => {
        $crate::compile_rue!(@impl $args $(<$($generic),+>)? = $mod_name $path true Some($export_name));
    };

    ( @impl $args:ident $(< $($generic:ident),+ >)? = $mod_name:ident $path:literal $debug:literal $export_name:expr ) => {
        static $mod_name: ::std::sync::LazyLock<Compilation> =
            ::std::sync::LazyLock::new(|| $crate::compile_rue(::std::path::Path::new($path), $debug, $export_name).unwrap());

        impl$(<$($generic),+>)? $crate::Mod for $args$(<$($generic),+>)? {
            fn mod_reveal() -> ::std::borrow::Cow<'static, [u8]> {
                ::std::borrow::Cow::Owned($mod_name.reveal.clone())
            }

            fn mod_hash() -> $crate::__internals::TreeHash {
                $mod_name.hash
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use clvm_traits::{FromClvm, ToClvm};
    use clvm_utils::CurriedProgram;
    use clvmr::{NodePtr, serde::node_from_bytes};

    use crate::{Mod, run_puzzle};

    use super::*;

    #[test]
    fn test_compile_chialisp() -> anyhow::Result<()> {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, ToClvm, FromClvm)]
        #[clvm(curry)]
        struct TestArgs<T, U> {
            a: T,
            b: U,
        }

        compile_chialisp!(TestArgs<T, U> = TEST_MOD, "compile_chialisp_test.clsp");

        let args = TestArgs { a: 10, b: 20 };

        let mut allocator = Allocator::new();

        let mod_ptr = node_from_bytes(&mut allocator, TestArgs::<u64, u64>::mod_reveal().as_ref())?;

        let ptr = CurriedProgram {
            program: mod_ptr,
            args,
        }
        .to_clvm(&mut allocator)?;

        let output = run_puzzle(&mut allocator, ptr, NodePtr::NIL)?;

        assert_eq!(hex::encode(node_to_bytes(&allocator, output)?), "8200e6");

        Ok(())
    }

    #[test]
    fn test_compile_rue() -> anyhow::Result<()> {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, ToClvm, FromClvm)]
        #[clvm(curry)]
        struct TestArgs<T> {
            a: u64,
            b: T,
        }

        compile_rue!(debug TestArgs<T> = TEST_MOD, "compile_rue_test.rue");

        let args = TestArgs { a: 10, b: 20 };

        let mut allocator = Allocator::new();

        let mod_ptr = node_from_bytes(&mut allocator, TestArgs::<u64>::mod_reveal().as_ref())?;

        let ptr = CurriedProgram {
            program: mod_ptr,
            args,
        }
        .to_clvm(&mut allocator)?;

        let output = run_puzzle(&mut allocator, ptr, NodePtr::NIL)?;

        assert_eq!(hex::encode(node_to_bytes(&allocator, output)?), "8200e6");

        Ok(())
    }

    #[test]
    fn test_compile_rue_export() -> anyhow::Result<()> {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, ToClvm, FromClvm)]
        #[clvm(curry)]
        struct TestArgs<T> {
            a: u64,
            b: T,
        }

        compile_rue!(debug TestArgs<T> = TEST_MOD, "compile_rue_test.rue", "another");

        let args = TestArgs { a: 10, b: 20 };

        let mut allocator = Allocator::new();

        let mod_ptr = node_from_bytes(&mut allocator, TestArgs::<u64>::mod_reveal().as_ref())?;

        let ptr = CurriedProgram {
            program: mod_ptr,
            args,
        }
        .to_clvm(&mut allocator)?;

        let output = run_puzzle(&mut allocator, ptr, NodePtr::NIL)?;

        assert_eq!(hex::encode(node_to_bytes(&allocator, output)?), "8200e6");

        Ok(())
    }
}
