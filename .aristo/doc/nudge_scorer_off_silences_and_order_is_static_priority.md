**Aristo verified intent — `nudge_scorer_off_silences_and_order_is_static_priority`**

Two invariants the scorer must preserve. First, `aggressiveness = off` is an absolute silence: its factor is 0.0 and the fire test is `pressure * factor >= 1`, so no signal fires at any pressure — even an infinite one. Second, the surfaced order is the static SIGNALS priority order, NOT the pressures: a count-pressure and a fraction-pressure are incommensurable, so sorting by pressure would let a noisy low-priority signal jump the queue ahead of a review the user actually needs to see first.

<sub>Verify level: **test**</sub>

---
