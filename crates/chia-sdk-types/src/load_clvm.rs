use std::{collections::HashMap, fs, io, path::Path, rc::Rc};

use chialisp::{
    classic::clvm_tools::clvmc::compile_clvm_text,
    compiler::{compiler::DefaultCompilerOpts, comptypes::CompilerOpts},
};
use clvm_traits::ToClvmError;
use clvm_utils::{TreeHash, tree_hash};
use clvmr::{Allocator, error::EvalErr, serde::node_to_bytes};
use rue_diagnostic::{Source, SourceKind};
use rue_options::CompilerOptions;
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

    #[error("Main not found")]
    MainNotFound,

    #[error("Export not found: {0}")]
    ExportNotFound(String),
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
    export_name: Option<String>,
) -> Result<Compilation, LoadClvmError> {
    let mut allocator = Allocator::new();

    let text = fs::read_to_string(path)?;

    let compilation = rue_compiler::compile_file(
        &mut allocator,
        Source::new(text.into(), SourceKind::File("main.rue".to_string())),
        if debug {
            CompilerOptions::debug()
        } else {
            CompilerOptions::default()
        },
    )
    .map_err(|error| LoadClvmError::Compiler(error.to_string()))?;

    let ptr = if let Some(export_name) = export_name {
        compilation
            .exports
            .get(&export_name)
            .copied()
            .ok_or(LoadClvmError::ExportNotFound(export_name))?
    } else {
        compilation.main.ok_or(LoadClvmError::MainNotFound)?
    };

    let hash = tree_hash(&allocator, ptr);
    let reveal = node_to_bytes(&allocator, ptr)?;

    Ok(Compilation { reveal, hash })
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, sync::LazyLock};

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

        static TEST_MOD: LazyLock<Compilation> = LazyLock::new(|| {
            compile_chialisp(
                Path::new("compile_chialisp_test.clsp"),
                &[".".to_string(), "include".to_string()],
            )
            .unwrap()
        });

        impl Mod for TestArgs {
            fn mod_reveal() -> Cow<'static, [u8]> {
                Cow::Owned(TEST_MOD.reveal.clone())
            }

            fn mod_hash() -> TreeHash {
                TEST_MOD.hash
            }
        }

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

        static TEST_MOD: LazyLock<Compilation> =
            LazyLock::new(|| compile_rue(Path::new("compile_rue_test.rue"), true, None).unwrap());

        impl Mod for TestArgs {
            fn mod_reveal() -> Cow<'static, [u8]> {
                Cow::Owned(TEST_MOD.reveal.clone())
            }

            fn mod_hash() -> TreeHash {
                TEST_MOD.hash
            }
        }

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
