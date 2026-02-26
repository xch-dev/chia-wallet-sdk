use std::{collections::HashMap, env, fs, path::PathBuf};

use anyhow::{Context, Result};
use clvm_utils::tree_hash;
use clvmr::{Allocator, serde::node_to_bytes};
use rue_compiler::{Compiler, FileTree, normalize_path};
use rue_options::find_project;

fn compile_rue_main(path: &PathBuf) -> Result<(Vec<u8>, [u8; 32])> {
    let mut allocator = Allocator::new();

    let path = path.canonicalize()?;
    let project = find_project(&path, false)?;
    let project = project.context("Rue project not found for puzzle source")?;

    let main_kind = if project.entrypoint.join("main.rue").exists() {
        Some(normalize_path(&project.entrypoint.join("main.rue"))?)
    } else {
        None
    };

    let mut ctx = Compiler::new(project.options);
    let tree = FileTree::compile_path(&mut ctx, &project.entrypoint, &mut HashMap::new())?;

    let ptr = if let Some(main_kind) = main_kind
        && let Some(main) = tree.main(&mut ctx, &mut allocator, &main_kind)?
    {
        main
    } else if let Some(main) = tree.main(&mut ctx, &mut allocator, &normalize_path(&path)?)? {
        main
    } else {
        anyhow::bail!("No `main` found in Rue file: {}", path.display());
    };

    let hash = tree_hash(&allocator, ptr).to_string();
    let hash_bytes: Vec<u8> = hex::decode(hash).context("invalid tree hash encoding")?;
    let hash_bytes: [u8; 32] = hash_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("unexpected tree hash length"))?;

    let reveal = node_to_bytes(&allocator, ptr)?;

    Ok((reveal, hash_bytes))
}

fn main() -> Result<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    let fee_layer_src = manifest_dir.join("puzzles").join("fee_layer_v1.rue");
    println!("cargo:rerun-if-changed={}", fee_layer_src.display());

    let (fee_layer_reveal, fee_layer_hash) = compile_rue_main(&fee_layer_src)?;

    let fee_layer_reveal_path = out_dir.join("fee_layer_v1.rue.bin");
    fs::write(&fee_layer_reveal_path, &fee_layer_reveal)?;

    let fee_layer_hash_list = fee_layer_hash
        .iter()
        .map(|b| format!("0x{b:02x}"))
        .collect::<Vec<_>>()
        .join(", ");

    let programs_rs = format!(
        r#"
/// Compiled from `puzzles/fee_layer_v1.rue`.
pub const FEE_LAYER_V1: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/fee_layer_v1.rue.bin"));

/// Tree hash of `FEE_LAYER_V1`.
pub const FEE_LAYER_V1_HASH: [u8; 32] = [{fee_layer_hash_list}];
"#
    );

    fs::write(out_dir.join("programs.rs"), programs_rs)?;

    Ok(())
}

