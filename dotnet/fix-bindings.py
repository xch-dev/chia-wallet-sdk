"""Works around bugs in uniffi-bindgen-cs, which are applied to the generated C# bindings.

1. Every object has a constructor that takes its handle as a `ulong`, which conflicts with
   constructors exported from Rust that take a single `u64` argument (such as `ReserveFee`).
   See https://github.com/NordSecurity/uniffi-bindgen-cs/issues/179
2. The generated code refers to `String` and `Action` from the `System` namespace without
   qualifying them, so they are shadowed by the `Clvm.String` method and `Action` class.

Each fix fails if the generated code doesn't have the expected shape, so that a change in
the generator is noticed rather than silently producing broken bindings.
"""

import re
import sys


def replace(source: str, pattern: str, replacement: str, expected: int | None = 1) -> str:
    result, count = re.subn(pattern, replacement, source, flags=re.DOTALL)
    if count == 0 or (expected is not None and count != expected):
        sys.exit(f"Expected {expected or 'any'} matches for {pattern!r}, found {count}")
    return result


path = sys.argv[1]

with open(path, encoding="utf-8") as file:
    source = file.read()

names = re.findall(r"^    public (\w+)\(ulong @\w+\) :$", source, flags=re.MULTILINE)

if not names:
    sys.exit("Expected at least one constructor with a single ulong argument")

for name in names:
    source = replace(
        source,
        rf"public {name}\(ulong pointer\) \{{",
        f"internal {name}(ulong pointer, bool _) {{",
    )
    source = replace(
        source,
        rf"return new {name}\(value\);",
        f"return new {name}(value, true);",
    )
    source = replace(
        source,
        rf"(public {name}\(ulong @\w+\) :\n        this\(\n.*?\n\))\) \{{\}}",
        r"\1, true) {}",
    )

source = replace(source, r"(?<![\w.])String\.Format\(", "System.String.Format(", None)
source = replace(source, r"\(Action invoke\)", "(System.Action invoke)")

with open(path, "w", encoding="utf-8") as file:
    file.write(source)

print(f"Fixed the generated C# bindings (handle constructors of {', '.join(names)})")
