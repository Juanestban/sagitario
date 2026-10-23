# 0030. Weak references: initialization, scope and runtime

- Status: Accepted
- Date: 2026-10-23
- Refines: [ADR-0027](0027-classes-and-arc.md) (decision 6)

## Context

ADR-0027 chose the syntax of `weak` references (`pub weak var owner: Option<Person>;`) and the
two counts of every object. F2 increment 3 implements them. Still open (questions Q37–Q41):
- whether weak fields need an explicit initial value;
- where `weak` is allowed;
- what reading and writing a weak field do;
- whether reference cycles get warnings;
- how the leak check counts destroyed objects that are still weakly referenced.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q37 | Initialization | A `weak` field is assigned in `new` like every other field (`self.owner = None;`); there are no default values. |
| Q38 | Scope | Only class fields, `var`, of type `Option<C>` where `C` is a class (and, from F2 increment 4, an interface). Anything else is E0328: struct fields, `const` fields, other types. There are no weak locals or enum fields. |
| Q39 | Access | Assigning `Some(x)` makes the field refer weakly to `x` (its weak count goes up, not its strong count) and drops the previous weak reference. Reading the field gives a new strong `Option`: `Some` while the instance is alive, `None` once its strong count reached zero. |
| Q40 | Cycle warnings | None for class fields in the MVP: a type-level check would flag legitimate lists and trees. The only cycle warning is the one in docs/spec/08-closures.md, for closures stored in fields that capture `self` (increment 6). |
| Q41 | Leak check | An object counts as alive until its memory is freed. When its strong count reaches zero its fields are released, and the memory is freed once no weak reference remains. |

## Alternatives considered

- **Weak fields start as `None` (Swift)**: convenient, but the only field with a default value.
- **`weak` in structs**: copying a struct would copy weak references, adding cost and rules for
  little use. It can be added later.

## Consequences

- New diagnostic E0328 (invalid `weak`).
- A weak field is stored as a bare object pointer (NULL for `None`). SIR reads it with
  `WeakLoad` and writes it with `WeakStore`; the runtime adds `sag_object_upgrade`,
  `sag_object_weak_set` and `sag_object_weak_release`.
- While an object's fields are released, the runtime holds a weak reference to it. This keeps
  an object that refers weakly to itself from being freed during its own destruction.
