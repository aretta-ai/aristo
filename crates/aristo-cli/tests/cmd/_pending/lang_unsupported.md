# `aristo lang` — unsupported language error

Source: `../aretta-sdk/docs/mockups/12-phase-1-architecture/cli-sessions.md` § "K5 → Unsupported language".

When neither manifest detection nor the optional `aristo.toml [language]` hint resolves to a registered `LanguageSyntax` impl, the CLI exits with a clear list of supported and planned languages. Skills (per K5) are instructed to surface this error rather than guess syntax.

```console
$ aristo lang
? 2
error: Cannot detect a supported language in this repository.
       Aristo supports: Rust.
       Planned: Python, Go, TypeScript.
       For unsupported languages, see https://aristo.ai/docs/languages
```
