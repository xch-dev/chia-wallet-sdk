use std::collections::HashMap;
use std::fmt::Debug;

use chia_protocol::Bytes32;
use chia_sha2::Sha256;
use clvm_traits::{FromClvm, ToClvm};

const HASH_TREE_PREFIX: &[u8] = &[2];
const HASH_LEAF_PREFIX: &[u8] = &[1];

#[derive(Debug, Clone)]
enum BinaryTree<T> {
    Leaf(T),
    Node(Box<BinaryTree<T>>, Box<BinaryTree<T>>),
}

/// A merkle tree implementation that can be used to prove the existence of a set of leaves.
/// The proof format is compatible with standard Chia puzzles, such as `p2_1_of_n.clsp`.
///
/// ## Example
///
/// ```rust
/// # use chia_protocol::Bytes32;
/// # use chia_sdk_types::MerkleTree;
/// let leaves = vec![
///     Bytes32::new([1; 32]),
///     Bytes32::new([2; 32]),
/// ];
/// let merkle_tree = MerkleTree::new(&leaves);
/// let root = merkle_tree.root();
/// let proof = merkle_tree.proof(leaves[0]);
/// ```
#[derive(Debug, Clone)]
pub struct MerkleTree {
    root: Bytes32,
    proofs: HashMap<Bytes32, MerkleProof>,
}

/// A proof for a leaf in a merkle tree. This is a CLVM type that can be
/// passed in the solution of puzzles.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MerkleProof {
    pub path: u32,
    #[clvm(rest)]
    pub proof: Vec<Bytes32>,
}

impl MerkleProof {
    pub fn new(path: u32, proof: Vec<Bytes32>) -> Self {
        Self { path, proof }
    }
}

impl MerkleTree {
    /// Create a new merkle tree from a list of leaves. The tree will be
    /// built automatically up front, so it doesn't need to be recomputed
    /// when used later.
    pub fn new(leaves: &[Bytes32]) -> Self {
        if leaves.is_empty() {
            return Self {
                root: Bytes32::default(),
                proofs: HashMap::new(),
            };
        }

        let (root, proofs) = MerkleTree::build_merkle_tree(leaves);
        Self { root, proofs }
    }

    /// Get the precomputed root hash of the merkle tree. Typically this will
    /// be stored in the curried arguments of a puzzle.
    pub fn root(&self) -> Bytes32 {
        self.root
    }

    /// Get the proof for a given leaf, if it exists in the tree.
    pub fn proof(&self, leaf: Bytes32) -> Option<MerkleProof> {
        self.proofs.get(&leaf).cloned()
    }

    fn build_merkle_tree(leaves: &[Bytes32]) -> (Bytes32, HashMap<Bytes32, MerkleProof>) {
        let binary_tree = MerkleTree::list_to_binary_tree(leaves);
        MerkleTree::build_merkle_tree_from_binary_tree(&binary_tree)
    }

    fn sha256(args: &[&[u8]]) -> Bytes32 {
        let mut hasher = Sha256::new();
        args.iter().for_each(|arg| hasher.update(arg));

        Bytes32::from(hasher.finalize())
    }

    fn list_to_binary_tree<T: Clone + Debug + Default>(objects: &[T]) -> BinaryTree<T> {
        let size = objects.len();
        if size == 0 {
            return BinaryTree::Leaf(T::default());
        }
        if size == 1 {
            return BinaryTree::Leaf(objects[0].clone());
        }
        let midpoint = (size + 1) >> 1;
        let first_half = &objects[..midpoint];
        let last_half = &objects[midpoint..];
        let left_tree = MerkleTree::list_to_binary_tree(first_half);
        let right_tree = MerkleTree::list_to_binary_tree(last_half);
        BinaryTree::Node(Box::new(left_tree), Box::new(right_tree))
    }

    fn build_merkle_tree_from_binary_tree(
        tuples: &BinaryTree<Bytes32>,
    ) -> (Bytes32, HashMap<Bytes32, MerkleProof>) {
        match tuples {
            BinaryTree::Leaf(t) => {
                let hash = MerkleTree::sha256(&[HASH_LEAF_PREFIX, t]);
                let mut proof = HashMap::new();
                proof.insert(*t, MerkleProof::new(0, vec![]));
                (hash, proof)
            }
            BinaryTree::Node(left, right) => {
                let (left_root, left_proofs) = MerkleTree::build_merkle_tree_from_binary_tree(left);
                let (right_root, right_proofs) =
                    MerkleTree::build_merkle_tree_from_binary_tree(right);

                let new_root = MerkleTree::sha256(&[HASH_TREE_PREFIX, &left_root, &right_root]);
                let mut new_proofs = HashMap::new();

                for (name, MerkleProof { path, mut proof }) in left_proofs {
                    proof.push(right_root);
                    new_proofs.insert(name, MerkleProof::new(path, proof));
                }

                for (name, MerkleProof { path, mut proof }) in right_proofs {
                    let path = path | (1 << proof.len());
                    proof.push(left_root);
                    new_proofs.insert(name, MerkleProof::new(path, proof));
                }

                (new_root, new_proofs)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;
    use rstest::rstest;

    #[rstest]
    #[case::no_leaves(&[],
           Bytes32::default(),
           vec![]
    )]
    #[case::one_leaf(&[Bytes32::from([1; 32])],
           Bytes32::from(hex!("ce041765675ad4d93378e20bd3a7d0d97ddcf3385fb6341581b21d4bc9e3e69e")),
           vec![(Bytes32::from([1; 32]), 0, vec![])]
    )]
    #[case::two_leaves(&[Bytes32::from([1; 32]), Bytes32::from([2; 32])],
           Bytes32::from(hex!("00f2e7e0bc3ee77f0b5aa330406f69bfbd5c2e3b8a4338dba49f64bb3f0247c4")),
           vec![
               (Bytes32::from([1; 32]), 0, vec![hex!("f1386fff8b06ac98d347997ff5d0abad3b977514b1b7cfe0689f45f3f1393497").into()]),
               (Bytes32::from([2; 32]), 1, vec![hex!("ce041765675ad4d93378e20bd3a7d0d97ddcf3385fb6341581b21d4bc9e3e69e").into()])
           ]
    )]
    #[case::three_leaves(&[Bytes32::from([1; 32]), Bytes32::from([2; 32]), Bytes32::from([3; 32])],
           Bytes32::from(hex!("adb439d3868b9273de8753e20a62a8e6d9ff6cfb43b189337a23df0690c7f55b")), 
           vec![
               (Bytes32::from([1; 32]), 0, vec![hex!("f1386fff8b06ac98d347997ff5d0abad3b977514b1b7cfe0689f45f3f1393497").into(), hex!("131c41585fc6b26c2cf8ea6fc61be03c3c4e3facb3f7e70ec69ea094b17dc3e1").into()]),
               (Bytes32::from([2; 32]), 1, vec![hex!("ce041765675ad4d93378e20bd3a7d0d97ddcf3385fb6341581b21d4bc9e3e69e").into(), hex!("131c41585fc6b26c2cf8ea6fc61be03c3c4e3facb3f7e70ec69ea094b17dc3e1").into()]),
               (Bytes32::from([3; 32]), 1, vec![hex!("00f2e7e0bc3ee77f0b5aa330406f69bfbd5c2e3b8a4338dba49f64bb3f0247c4").into()])
           ]
    )]
    #[case::seven_leaves(&[Bytes32::from([1; 32]), Bytes32::from([2; 32]), Bytes32::from([3; 32]), Bytes32::from([4; 32]), Bytes32::from([5; 32]), Bytes32::from([6; 32]), Bytes32::from([7; 32])],
           Bytes32::from(hex!("1c4b11429685dd0a516282981bb3e12c13596e846f67af1da080b9134cdea4c6")),
           vec![
               (Bytes32::from([1; 32]), 0, vec![hex!("f1386fff8b06ac98d347997ff5d0abad3b977514b1b7cfe0689f45f3f1393497").into(), hex!("1d85c3d5d2a5f093b49c79b2686ff698fb58d3ef4959b939ed6925dc65325499").into(), hex!("c80c9f4f69abfa70474c4d27d076ab32e23ff9bd1215fe87c6a0e6899a126d10").into()]),
               (Bytes32::from([2; 32]), 1, vec![hex!("ce041765675ad4d93378e20bd3a7d0d97ddcf3385fb6341581b21d4bc9e3e69e").into(), hex!("1d85c3d5d2a5f093b49c79b2686ff698fb58d3ef4959b939ed6925dc65325499").into(), hex!("c80c9f4f69abfa70474c4d27d076ab32e23ff9bd1215fe87c6a0e6899a126d10").into()]),
               (Bytes32::from([3; 32]), 2, vec![hex!("db1a2656e1809de78fb29dddf24a1c75fbf7c6dc1f1341f485457c713ce49fa0").into(), hex!("00f2e7e0bc3ee77f0b5aa330406f69bfbd5c2e3b8a4338dba49f64bb3f0247c4").into(), hex!("c80c9f4f69abfa70474c4d27d076ab32e23ff9bd1215fe87c6a0e6899a126d10").into()]),
               (Bytes32::from([4; 32]), 3, vec![hex!("131c41585fc6b26c2cf8ea6fc61be03c3c4e3facb3f7e70ec69ea094b17dc3e1").into(), hex!("00f2e7e0bc3ee77f0b5aa330406f69bfbd5c2e3b8a4338dba49f64bb3f0247c4").into(), hex!("c80c9f4f69abfa70474c4d27d076ab32e23ff9bd1215fe87c6a0e6899a126d10").into()]),
               (Bytes32::from([5; 32]), 4, vec![hex!("0684e189ecc12eb7472925a5b16ec60d10a476a59545452f58fcca994433a4f7").into(), hex!("d3907c0247e7e98b72338a00d87244248df71eb313589da290d45adfba44e6d2").into(), hex!("7eb919730e38f305365791a43adddeea0fc275371aac8c7b08983937beeb956f").into()]),
               (Bytes32::from([6; 32]), 5, vec![hex!("90cbc3c7c7634183ae482172520c1b8d85ee10f1ca0b4744fdbe7da2245141bb").into(), hex!("d3907c0247e7e98b72338a00d87244248df71eb313589da290d45adfba44e6d2").into(), hex!("7eb919730e38f305365791a43adddeea0fc275371aac8c7b08983937beeb956f").into()]),
               (Bytes32::from([7; 32]), 3, vec![hex!("3831644ba5da8ec5f16d32ef7c0a318cfec302245fac118321a5da9f43efbf94").into(), hex!("7eb919730e38f305365791a43adddeea0fc275371aac8c7b08983937beeb956f").into()])
           ]
    )]
    #[case::eight_leaves(&[Bytes32::from([1; 32]), Bytes32::from([2; 32]), Bytes32::from([3; 32]), Bytes32::from([4; 32]), Bytes32::from([5; 32]), Bytes32::from([6; 32]), Bytes32::from([7; 32]), Bytes32::from([8; 32])],
           Bytes32::from(hex!("3023a77c57dd4c0f84fe2d9b42252e483a9974482b6d4d5fbf0e3d405a46f436")),
           vec![
               (Bytes32::from([1; 32]), 0, vec![hex!("f1386fff8b06ac98d347997ff5d0abad3b977514b1b7cfe0689f45f3f1393497").into(), hex!("1d85c3d5d2a5f093b49c79b2686ff698fb58d3ef4959b939ed6925dc65325499").into(), hex!("eb06e593af742e80db1c2bef77f23c85ad87a8048bb1228037cd18d6b50f9042").into()]),
               (Bytes32::from([2; 32]), 1, vec![hex!("ce041765675ad4d93378e20bd3a7d0d97ddcf3385fb6341581b21d4bc9e3e69e").into(), hex!("1d85c3d5d2a5f093b49c79b2686ff698fb58d3ef4959b939ed6925dc65325499").into(), hex!("eb06e593af742e80db1c2bef77f23c85ad87a8048bb1228037cd18d6b50f9042").into()]),
               (Bytes32::from([3; 32]), 2, vec![hex!("db1a2656e1809de78fb29dddf24a1c75fbf7c6dc1f1341f485457c713ce49fa0").into(), hex!("00f2e7e0bc3ee77f0b5aa330406f69bfbd5c2e3b8a4338dba49f64bb3f0247c4").into(), hex!("eb06e593af742e80db1c2bef77f23c85ad87a8048bb1228037cd18d6b50f9042").into()]),
               (Bytes32::from([4; 32]), 3, vec![hex!("131c41585fc6b26c2cf8ea6fc61be03c3c4e3facb3f7e70ec69ea094b17dc3e1").into(), hex!("00f2e7e0bc3ee77f0b5aa330406f69bfbd5c2e3b8a4338dba49f64bb3f0247c4").into(), hex!("eb06e593af742e80db1c2bef77f23c85ad87a8048bb1228037cd18d6b50f9042").into()]),
               (Bytes32::from([5; 32]), 4, vec![hex!("0684e189ecc12eb7472925a5b16ec60d10a476a59545452f58fcca994433a4f7").into(), hex!("f76c002f93a1ba959ebe50568ba888a5d1871e2f804977e996bb6932f7eadf06").into(), hex!("7eb919730e38f305365791a43adddeea0fc275371aac8c7b08983937beeb956f").into()]),
               (Bytes32::from([6; 32]), 5, vec![hex!("90cbc3c7c7634183ae482172520c1b8d85ee10f1ca0b4744fdbe7da2245141bb").into(), hex!("f76c002f93a1ba959ebe50568ba888a5d1871e2f804977e996bb6932f7eadf06").into(), hex!("7eb919730e38f305365791a43adddeea0fc275371aac8c7b08983937beeb956f").into()]),
               (Bytes32::from([7; 32]), 6, vec![hex!("467d8acd80729c1fe2c497db207e7861b0fd9aab3552da7a2abb828a45f288cc").into(), hex!("3831644ba5da8ec5f16d32ef7c0a318cfec302245fac118321a5da9f43efbf94").into(), hex!("7eb919730e38f305365791a43adddeea0fc275371aac8c7b08983937beeb956f").into()]),
               (Bytes32::from([8; 32]), 7, vec![hex!("d3907c0247e7e98b72338a00d87244248df71eb313589da290d45adfba44e6d2").into(), hex!("3831644ba5da8ec5f16d32ef7c0a318cfec302245fac118321a5da9f43efbf94").into(), hex!("7eb919730e38f305365791a43adddeea0fc275371aac8c7b08983937beeb956f").into()])
           ]
    )]
    fn test_merkle_tree(
        #[case] leaves: &[Bytes32],
        #[case] expected_root: Bytes32,
        #[case] expected_proofs: Vec<(Bytes32, u32, Vec<Bytes32>)>,
    ) {
        let merkle_tree = MerkleTree::new(leaves);

        assert_eq!(merkle_tree.root(), expected_root);

        for (leaf, path, proof) in expected_proofs {
            assert_eq!(merkle_tree.proof(leaf), Some(MerkleProof::new(path, proof)));
        }
    }
}
