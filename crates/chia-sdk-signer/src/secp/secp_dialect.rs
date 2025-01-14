use std::cell::RefCell;

use chia_protocol::Bytes32;
use chia_secp::{K1PublicKey, R1PublicKey};
use clvm_traits::FromClvm;
use clvmr::{
    cost::Cost,
    dialect::{Dialect, OperatorSet},
    op_utils::get_args,
    reduction::{Reduction, Response},
    Allocator, NodePtr,
};

use super::{RequiredSecpSignature, SecpPublicKey};

const SECP256R1_VERIFY_COST: Cost = 1_850_000;
const SECP256K1_VERIFY_COST: Cost = 1_300_000;

#[derive(Debug, Default, Clone)]
pub struct SecpDialect<T> {
    dialect: T,
    collected_ops: RefCell<Vec<RequiredSecpSignature>>,
}

impl<T> SecpDialect<T> {
    pub fn new(dialect: T) -> Self {
        Self {
            dialect,
            collected_ops: RefCell::new(Vec::new()),
        }
    }

    pub fn collect(self) -> Vec<RequiredSecpSignature> {
        self.collected_ops.into_inner()
    }
}

impl<T> Dialect for SecpDialect<T>
where
    T: Dialect,
{
    fn apply_kw(&self) -> u32 {
        self.dialect.apply_kw()
    }

    fn quote_kw(&self) -> u32 {
        self.dialect.quote_kw()
    }

    fn softfork_kw(&self) -> u32 {
        self.dialect.softfork_kw()
    }

    fn allow_unknown_ops(&self) -> bool {
        self.dialect.allow_unknown_ops()
    }

    fn softfork_extension(&self, ext: u32) -> OperatorSet {
        self.dialect.softfork_extension(ext)
    }

    fn op(
        &self,
        allocator: &mut Allocator,
        op: NodePtr,
        args: NodePtr,
        max_cost: Cost,
        extensions: OperatorSet,
    ) -> Response {
        let response = self.dialect.op(allocator, op, args, max_cost, extensions);

        let op_len = allocator.atom_len(op);
        if op_len != 4 {
            return response;
        }

        let atom = allocator.atom(op);
        let opcode = u32::from_be_bytes(atom.as_ref().try_into().unwrap());

        let (r1, name, cost) = match opcode {
            // We special case these opcodes and allow the response to pass through otherwise.
            // If new operators are added to the main dialect, they likely shouldn't be included here.
            // We're using the same cost to ensure that softfork conditions behave the same.
            0x13d6_1f00 => (false, "secp256k1_verify", SECP256K1_VERIFY_COST),
            0x1c3a_8f00 => (true, "secp256r1_verify", SECP256R1_VERIFY_COST),
            _ => return response,
        };

        let [pubkey, msg, sig] = get_args::<3>(allocator, args, name)?;

        let Ok(public_key) = (if r1 {
            R1PublicKey::from_clvm(allocator, pubkey).map(SecpPublicKey::R1)
        } else {
            K1PublicKey::from_clvm(allocator, pubkey).map(SecpPublicKey::K1)
        }) else {
            return response;
        };

        let Ok(message_hash) = Bytes32::from_clvm(allocator, msg) else {
            return response;
        };

        self.collected_ops.borrow_mut().push(RequiredSecpSignature {
            public_key,
            message_hash: message_hash.to_bytes(),
            placeholder_ptr: sig,
        });

        Ok(Reduction(cost, NodePtr::NIL))
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::Bytes;
    use chia_secp::K1SecretKey;
    use clvm_traits::{clvm_list, clvm_quote, ToClvm};
    use clvmr::{run_program, ChiaDialect};
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    use super::*;

    #[test]
    fn test_signature_collection() -> anyhow::Result<()> {
        let mut a = Allocator::new();
        let mut rng = ChaCha8Rng::seed_from_u64(1337);

        let op = Bytes::from(vec![0x13, 0xd6, 0x1f, 0x00]);
        let public_key = K1SecretKey::from_bytes(&rng.gen())?.public_key();
        let fake_sig = a.new_atom(&[1, 2, 3])?;
        let message = a.new_atom(&[42; 32])?;
        let program = clvm_list!(
            op,
            clvm_quote!(public_key),
            clvm_quote!(message),
            clvm_quote!(fake_sig)
        )
        .to_clvm(&mut a)?;

        let dialect = SecpDialect::new(ChiaDialect::new(0));

        let reduction = run_program(&mut a, &dialect, program, NodePtr::NIL, u64::MAX).unwrap();
        let mut collected = dialect.collect();

        assert!(a.atom(reduction.1).is_empty());
        assert_eq!(collected.len(), 1);

        let item = collected.remove(0);
        assert_eq!(item.public_key, SecpPublicKey::K1(public_key));
        assert_eq!(item.placeholder_ptr, fake_sig);

        Ok(())
    }
}
