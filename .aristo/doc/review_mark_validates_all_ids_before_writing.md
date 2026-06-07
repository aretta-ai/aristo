**Aristo verified intent — `review_mark_validates_all_ids_before_writing`**

Marking validates EVERY requested id against the index before writing any review state: an unknown id (a typo, or an id that isn't an authored intent) marks nothing and errors. Partial marking on a bad batch would silently record some reviews and drop others, leaving the backlog disagreeing with what the user believes they reviewed.

<sub>Verify level: **test**</sub>

---
