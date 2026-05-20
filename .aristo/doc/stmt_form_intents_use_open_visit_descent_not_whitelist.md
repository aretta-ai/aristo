**Aristo verified intent — `stmt_form_intents_use_open_visit_descent_not_whitelist`**

stmt-form intents are discovered via syn::Visit's full descent (visit_block + default traversal of every Expr variant), NOT a hand-rolled whitelist of expression kinds. A whitelist silently drops macros nested inside any unenumerated context — match arms, closures, unsafe blocks, async blocks, try blocks, let initializers — and the failure mode is invisible (the intent doesn't appear in `aristo list`, can't be cited as a ground in a proof, and skips the freshness check). The Visit-based descent is open by default; new syn::Expr variants get visited automatically.

<sub>Verify level: **test**</sub>

---
