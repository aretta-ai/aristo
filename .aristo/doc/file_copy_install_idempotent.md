**Aristo verified intent — `file_copy_install_idempotent`**

A second invocation with identical content leaves the target byte-identical and returns `Unchanged`. Created (file did not exist) and Updated (content differed) are distinct outcomes; idempotence is the Unchanged case specifically.

<sub>Verify level: **test**</sub>

---
