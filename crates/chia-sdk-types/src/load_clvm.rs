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

#[derive(Debug, Error)]
pub enum LoadClvmError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("CLVM error: {0}")]
    Clvm(#[from] EvalErr),

    #[error("Invalid file name")]
    InvalidFileName,

    #[error("Compiler error: {0}")]
    Compiler(String),

    #[error("Conversion error: {0}")]
    Conversion(#[from] ToClvmError),

    #[error("Project error: {0}")]
    Project(#[from] rue_options::Error),

    #[error("Project not found")]
    ProjectNotFound,

    #[error("Main not found")]
    MainNotFound,

    #[error("Export not found: {0}")]
    ExportNotFound(String),

    #[error("Rue error: {0}")]
    Rue(#[from] rue_compiler::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Compilation {
    pub reveal: Vec<u8>,
    pub hash: TreeHash,
}

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

    let ptr = if let Some(export_name) = export_name {
        if let Some(export) = tree
            .exports(
                &mut ctx,
                &mut allocator,
                main_kind.as_ref(),
                Some(export_name),
            )?
            .into_iter()
            .next()
        {
            export.ptr
        } else {
            return Err(LoadClvmError::ExportNotFound(export_name.to_string()));
        }
    } else if let Some(main_kind) = main_kind
        && let Some(main) = tree.main(&mut ctx, &mut allocator, &main_kind)?
    {
        main
    } else if let Some(main) = tree.main(&mut ctx, &mut allocator, &normalize_path(&path)?)? {
        main
    } else {
        return Err(LoadClvmError::MainNotFound);
    };

    let hash = tree_hash(&allocator, ptr);
    let reveal = node_to_bytes(&allocator, ptr)?;

    Ok(Compilation { reveal, hash })
}

#[macro_export]
macro_rules! compile_chialisp {
    ( $args:ty = $mod_name:ident, $path:literal ) => {
        compile_chialisp!(impl $args = $mod_name $path);
    };

    ( impl $args:ty = $mod_name:ident $path:literal ) => {
        static $mod_name: ::std::sync::LazyLock<Compilation> =
            ::std::sync::LazyLock::new(|| $crate::compile_chialisp(::std::path::Path::new($path), &[".".to_string(), "include".to_string()]).unwrap());

        impl $crate::Mod for $args {
            fn mod_reveal() -> ::std::borrow::Cow<'static, [u8]> {
                ::std::borrow::Cow::Owned($mod_name.reveal.clone())
            }

            fn mod_hash() -> $crate::__internals::TreeHash {
                $mod_name.hash
            }
        }
    };
}

#[macro_export]
macro_rules! compile_rue {
    ( $args:ty = $mod_name:ident, $path:literal ) => {
        compile_rue!(impl $args = $mod_name $path false None);
    };

    ( $args:ty = $mod_name:ident, $path:literal, $export_name:literal ) => {
        compile_rue!(impl $args = $mod_name $path false Some($export_name));
    };

    ( debug $args:ty = $mod_name:ident, $path:literal ) => {
        compile_rue!(impl $args = $mod_name $path true None);
    };

    ( debug $args:ty = $mod_name:ident, $path:literal, $export_name:literal ) => {
        compile_rue!(impl $args = $mod_name $path true Some($export_name));
    };

    ( impl $args:ty = $mod_name:ident $path:literal $debug:literal $export_name:expr ) => {
        static $mod_name: ::std::sync::LazyLock<Compilation> =
            ::std::sync::LazyLock::new(|| $crate::compile_rue(::std::path::Path::new($path), $debug, $export_name).unwrap());

        impl $crate::Mod for $args {
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
        struct TestArgs {
            a: u64,
            b: u64,
        }

        compile_chialisp!(TestArgs = TEST_MOD, "compile_chialisp_test.clsp");

        let args = TestArgs { a: 10, b: 20 };

        let mut allocator = Allocator::new();

        let mod_ptr = node_from_bytes(&mut allocator, TestArgs::mod_reveal().as_ref())?;

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
        struct TestArgs {
            a: u64,
            b: u64,
        }

        compile_rue!(debug TestArgs = TEST_MOD, "compile_rue_test.rue");

        let args = TestArgs { a: 10, b: 20 };

        let mut allocator = Allocator::new();

        let mod_ptr = node_from_bytes(&mut allocator, TestArgs::mod_reveal().as_ref())?;

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
        struct TestArgs {
            a: u64,
            b: u64,
        }

        compile_rue!(debug TestArgs = TEST_MOD, "compile_rue_test.rue", "another");

        let args = TestArgs { a: 10, b: 20 };

        let mut allocator = Allocator::new();

        let mod_ptr = node_from_bytes(&mut allocator, TestArgs::mod_reveal().as_ref())?;

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
