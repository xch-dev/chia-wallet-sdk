# Allocator

The `Allocator` is how you interact with the CLVM runtime (which lives in the [clvmr](https://docs.rs/clvmr/latest/clvmr/) crate).

Values that you allocate, which are referenced by `NodePtr`, won't be freed until you free the `Allocator` itself. You can think of it as a sort of memory arena. All CLVM values are stored contiguously on the heap, to prevent repeated memory allocations, and to improve cache locality. This makes working with the `Allocator` very efficient, which is especially desirable for Rust projects.

## Low Level

On its own, [clvmr](https://docs.rs/clvmr/latest/clvmr/) doesn't provide many utilities for working with the `Allocator`.

For example, this is how you would allocate a string value:

```rs
let mut allocator = Allocator::new();

let ptr = allocator.new_atom(b"Hello, world!").unwrap();
```

And you can read it back like so:

```rs
let atom = allocator.atom(ptr).to_vec();
let string = String::from_utf8(atom).unwrap();
```

You can also create pairs of values:

```rs
use clvmr::SExp;

let pair = allocator.new_pair(ptr, ptr).unwrap();

let SExp::Pair(first, rest) = allocator.sexp(pair) else {
    panic!("Expected a pair");
};
```

## Type Conversions

As you can see, it gets pretty tedious to work with CLVM values in Rust by hand, especially if you need to allocate and parse complex data structures like lists and curried program arguments.

This is where [clvm-traits](https://docs.rs/clvm-traits/latest/clvm_traits/) comes in. This library provides traits and macros for converting between complex CLVM values and Rust values.

The examples above can be rewritten like this:

```rs
use clvm_traits::{FromClvm, ToClvm};

let ptr = "Hello.world!".to_clvm(&mut allocator).unwrap();
let string = String::from_clvm(&allocator, ptr).unwrap();

let pair = (ptr, ptr).to_clvm(&mut allocator).unwrap();
let (first, rest) = <(String, String)>::from_clvm(&allocator, ptr).unwrap();
```

This is much more straightforward now, and you can nest types together instead of parsing individual values repeatedly.

You can read more about how to define your own types in the [clvm-traits documentation](https://docs.rs/clvm-traits/latest/clvm_traits/), but for now we'll move on.
