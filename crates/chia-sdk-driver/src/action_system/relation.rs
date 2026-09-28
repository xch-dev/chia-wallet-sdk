/// How the conditions spends in a transaction are tied together, so that the transaction can't be
/// split apart and only partially included in a block. Settlement spends are never included in the
/// relation, since they can't output additional conditions. If there's only a single conditions
/// spend, no relation conditions are added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Relation {
    /// The spends aren't tied together. This is only safe if something else ties them together,
    /// such as signatures that can't be separated, or messages between the spends.
    None,

    /// Each spend asserts that the previous spend (wrapping around to the last) is included in the
    /// same block, with `ASSERT_CONCURRENT_SPEND`.
    AssertConcurrent,

    /// Each spend creates a coin announcement with an empty message, and asserts the announcement
    /// of the next spend (wrapping around to the first). Removing any spend breaks the ring.
    CoinAnnouncementRing,

    /// The first spend creates a coin announcement with an empty message, and every other spend
    /// asserts it. This ties every spend to the first spend (and therefore to each other), but the
    /// first spend could be included on its own, without the others.
    CoinAnnouncementHub,
}
