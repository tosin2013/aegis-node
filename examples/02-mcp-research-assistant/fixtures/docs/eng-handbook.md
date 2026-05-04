# Engineering Handbook

## How we work

We ship in small, well-scoped PRs. Every change is reviewed by at least one
other engineer. Tests run on every PR; main is always green.

We default to writing no comments. Code should be self-documenting; comments
are reserved for explaining *why*, not *what*.

We treat alerts as bugs — if something pages and there's no real underlying
problem, we either fix the detection or remove the alert.
