# Spend Context

The `SpendContext` is an important utility used by the driver code to simplify coin spends. You can think of it as an [Allocator](./allocator.md) with these additional benefits:

1. A simpler interface for common operations.
2. Automatic caching for puzzle pointers, since the size can be large.
3. Simple mechanism for currying a puzzle with its arguments.
4. Keeps track of coin spends for you, so that they can be collected at the end in one place.

In fact, you can call `ctx.allocator` directly if you need to as well.

## Setup

It's just as easy to create a `SpendContext` as an `Allocator`:

```rs
use chia_sdk_driver::SpendContext;

let mut ctx = SpendContext::new();
```

You can also create one from an existing `Allocator`, although you shouldn't need to do this that often:

```rs
use clvmr::Allocator;

let mut ctx = SpendContext::from(Allocator::new());
```
