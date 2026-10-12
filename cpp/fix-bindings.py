"""Works around bugs in uniffi-bindgen-cpp, which are applied to the generated C++ bindings.

1. Function and method names aren't escaped if they're C++ keywords (such as `Clvm::int`),
   so an underscore is appended to them, which is how the generator escapes other names.
2. Classes are forward declared with their original name, but referred to with the name
   converted to upper camel case (such as `VDFProof` and `VdfProof`), so the latter is used.

Each fix fails if the generated code doesn't have the expected shape, so that a change in the
generator is noticed rather than silently producing broken bindings.
"""

import re
import sys

KEYWORDS = """
alignas alignof and and_eq asm auto bitand bitor bool break case catch char char8_t char16_t
char32_t class compl concept const consteval constexpr constinit const_cast continue co_await
co_return co_yield decltype default delete do double dynamic_cast else enum explicit export
extern false float for friend goto if inline int long mutable namespace new noexcept not not_eq
nullptr operator or or_eq private protected public register reinterpret_cast requires return
short signed sizeof static static_assert static_cast struct switch template this thread_local
throw true try typedef typeid typename union unsigned using virtual void volatile wchar_t while
xor xor_eq
""".split()

KEYWORD = "|".join(KEYWORDS)


def upper_camel_case(name: str) -> str:
    words = re.findall(r"[A-Z]+(?=[A-Z][a-z]|\d|$)|[A-Z]?[a-z\d]+|[A-Z]+", name)
    return "".join(word.capitalize() for word in words)


header_path, source_path = sys.argv[1], sys.argv[2]

with open(header_path, encoding="utf-8") as file:
    header = file.read()

with open(source_path, encoding="utf-8") as file:
    source = file.read()

# Declarations in the header, such as `std::shared_ptr<Program> int(const std::string &value);`
header, declarations = re.subn(
    rf"^(    [^\n(]*[\w>&*] )({KEYWORD})\(", r"\1\2_(", header, flags=re.MULTILINE
)

# Definitions in the source, such as `std::shared_ptr<Program> Clvm::int(const std::string &value) {`
source, definitions = re.subn(rf"^([^\n(]* \w+::)({KEYWORD})\(", r"\1\2_(", source, flags=re.MULTILINE)

if declarations == 0 or declarations != definitions:
    sys.exit(f"Expected matching keyword declarations and definitions, found {declarations} and {definitions}")

renamed = []

for name in re.findall(r"^struct (\w+);$", header, flags=re.MULTILINE):
    camel = upper_camel_case(name)

    if camel != name and re.search(rf"\b{camel}\b", header):
        header = re.sub(rf"\b{name}\b", camel, header)
        source = re.sub(rf"\b{name}\b", camel, source)
        renamed.append(f"{name} to {camel}")

with open(header_path, "w", encoding="utf-8") as file:
    file.write(header)

with open(source_path, "w", encoding="utf-8") as file:
    file.write(source)

print(f"Fixed the generated C++ bindings ({declarations} keyword methods, renamed {', '.join(renamed) or 'nothing'})")
