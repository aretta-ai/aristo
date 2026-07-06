**Aristo verified intent — `instrument_write_is_idempotent`**

Re-emitting identical content leaves the file byte-identical and returns Unchanged; Created (file absent) and Updated (content differed) are the other two outcomes. Idempotence is the Unchanged case specifically — a re-run on up-to-date output must not rewrite the file, which would churn its mtime and dirty a clean tree. Shared by vendor-c and gen-c.

<sub>Verify level: **test**</sub>

---
