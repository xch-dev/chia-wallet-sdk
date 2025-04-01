## Puzzles

Chia coins have a puzzle, which controls how it can be spent.
The solution is used as the arguments to the puzzle, and the
output is a list of conditions.

A puzzle consists of multiple layers composed together.

## Layers

A layer is a subset of the logic that makes up a smart coin in Chia.
They are also referred to as "inner puzzles", and the solution can be broken
up into "inner solutions" as well.

Generally, you can parse and construct the individual layers separately.
This allows them to be composed together freely. However, there are sometimes
additional restraints which limit the ways they can be mixed. For example,
the [`CatLayer`] cannot have another [`CatLayer`] as its inner puzzle, due to the
way it's written. This would create an error when validating the announcements.

### P2 Puzzles

A p2 puzzle (meaning "pay to") controls the ownership of the coin.
The simplest example of this is [`p2_conditions.clsp`], which requires a signature
from a single public key and outputs a list of conditions from the solution.

The "standard transaction" (which is [`p2_delegated_puzzle_or_hidden_puzzle.clsp`])
is a kind of p2 puzzle that adds additional flexibility. Specifically, support
for an inner puzzle, and usage of a delegated puzzle instead of directly conditions.

Generally, the p2 puzzle is the base layer in a coin's puzzle, and everything
else builds on top of it to restrict the way it can be spent or attach state.

## Primitives

A `Primitive` uses one or more `Layer` to parse info from a parent's coin spend.
Generally, `Layer` has the ability to parse and construct individual puzzles and solutions,
and the composed `Primitive` struct can parse all of the information required to spend a coin.
The `Primitive` should also provide a way to spend the coin, and other utilities necessary.

[`p2_conditions.clsp`]: https://github.com/Chia-Network/chia-blockchain/blob/bd022b0c9b0d3e0bc13a0efebba9f22417ca64b5/chia/wallet/puzzles/p2_conditions.clsp
[`p2_delegated_puzzle_or_hidden_puzzle.clsp`]: https://github.com/Chia-Network/chia-blockchain/blob/bd022b0c9b0d3e0bc13a0efebba9f22417ca64b5/chia/wallet/puzzles/p2_delegated_puzzle_or_hidden_puzzle.clsp
