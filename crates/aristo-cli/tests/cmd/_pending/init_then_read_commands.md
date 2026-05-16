# Read commands work on a freshly-initialized project

Source: `../aretta-sdk/docs/diagrams/02-state-map.mmd` § `idx -. preflight .-> r_list, r_status`.

The block of `init_creates_index_file.md` that exercises `aristo list` and `aristo status` on a freshly-initialized project — split out because those two commands don't land until slices 18 and 19. Promote into `active/` (and delete this file) when slice 19 closes.

```console
$ aristo init
[..]

$ aristo list
0 annotations.

$ aristo status
Aristo SDK v[..]
  Tier:              [..]
  Annotations:       0
  Verified:          0 (n/a)
  Index:             .aristo/index.toml ([..])
[..]

```
