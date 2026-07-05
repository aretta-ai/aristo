# `aristo lang` — auto-detect C from a build manifest

Source: C annotate design (`docs/multilang-c-go-design-study.md`), slice C-3.

With no arguments, `aristo lang` detects the repo's language from its manifest — a C build file (`Makefile`, `CMakeLists.txt`, `compile_commands.json`, ...), or, failing that, a `.c`/`.h` source in the directory — and emits the C annotation cheat sheet the C directive extractor accepts. Skills run `aristo lang` first so they author C directives with the canonical syntax rather than guessing.

```console
$ aristo lang
Detected language: C (from Makefile)

# Aristo annotation syntax — C

## Directive form (a `//` line comment directly above the item)
  // @aristo intent("text here", verify = "test", id = "snake_case_id", parent = "other_id")
  int the_thing(void) { ... }
...
```
