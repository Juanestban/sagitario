# 0027. Classes, construction, ARC objects and `weak` references

- Status: Accepted; refined by [ADR-0030](0030-weak-references.md), [ADR-0035](0035-explicit-self-in-initializers.md)
- Date: 2026-10-23
- Refines: [ADR-0002](0002-arc-memory-model.md) and [ADR-0004](0004-class-model.md);
  resolves open question Q12

## Context

F2 starts with classes managed by ARC. ADR-0004 and docs/spec/04-classes.md fixed the class
model (`new` as initializer, `const`/`var` fields, single inheritance, interfaces), but left open:
- classes without an initializer;
- what `new` must do before `self` can be used;
- how `self` behaves in class methods;
- whether classes have destructors;
- how instances compare;
- the syntax of `weak` references (Q12), which also shapes the header of every object.

F2 also needed to be split into increments, as F1 was.

## Decision

### Increments of F2

1. Classes and ARC.
2. Inheritance: `extends`, `zod`, `override`, `abstract`, dynamic dispatch and devirtualization.
3. `weak` references.
4. Interfaces: default methods, `impl` on classes/structs/enums, interfaces as types, the
   `Error` interface.
5. Monomorphized generics with bounds.
6. Closures: arrow functions `(x) => x * 2` as in docs/spec/08-closures.md, function types,
   and the capture rules.
7. `List<T>`, `Map<K, V>` and iterators with `map`/`filter`.
8. The reference "zoo" program, with ASan.

### Classes

| # | Topic | Decision |
|---|---|---|
| 1 | Class without `new` | A class without fields gets an implicit `new()` without parameters. A class with fields must declare `new` (E0324). Fields have no initializers in the MVP (`var n: i32 = 0;` does not exist). |
| 2 | Inside `new` | `self` is implicit (superseded by ADR-0035: `new` declares `self`). Every field, `const` and `var`, is assigned before `new` returns: at its end and at every `return`. Before that, `self` is only used to assign fields and to read fields already assigned. Calling methods on `self` or passing it along is an error (E0325). A `const` field is assigned exactly once (E0308). `new` has no return type (E0324). |
| 3 | `self` in methods | Methods take `self`; class methods never take `var self` (see ADR-0028). A `var` field can be modified through any reference to the instance, including a `const` binding or a parameter. The same holds for the parts of a `var` field: `obj.pos.x = 1` and `obj.pos.move_by(1.0)` work when `pos` is `var`. A `const` field cannot be modified outside `new`, not even in its parts. `self` cannot be reassigned in a class (E0308). |
| 4 | Destructors | There are none in the MVP. When the last reference goes away, the object releases its fields. Chains of objects are destroyed in a loop, not recursively, so long linked lists do not exhaust the stack. |
| 5 | Operations | `==` and `!=` compare identity between references of the same class; from increment 2, also between classes related by inheritance. A class cannot be interpolated until a `Display` interface exists. A class may contain itself through `Option<C>`: it is a reference. |
| 6 | `weak` (Q12) | A field modifier: `pub weak var owner: Option<Person>;`, only on `var` fields of type `Option<C>`, where `C` is a class or (from increment 4) an interface. Reading the field gives a strong `Option<C>`, which is `None` once the object is destroyed; assigning works like any field. Every object carries a strong and a weak count. Its fields are released when the strong count reaches zero, and its memory is freed when both counts do. There are no `weak` locals in the MVP. Implemented in increment 3. |
| 7 | `new C(...)` | Only through `new`, never `C::new(...)` or `obj.new(...)` (E0314). A `prv fn new` can only be used inside the class (E0205). |

## Alternatives considered

- **Field initializers (`var count: i32 = 0;`)**: convenient, but they add a second way to
  initialize fields and an evaluation order to specify. They can be added later without
  breaking code.
- **`Weak<C>` with `.upgrade()` (Rust)**: more explicit, but more verbose. It also works for
  locals and collections, which the MVP does not need, and leaves the reserved keyword `weak`
  unused.
- **User destructors (`fn drop(self)`, Swift's `deinit`)**: useful for resources. Resources are
  not part of the MVP standard library, and destructors would need rules about resurrecting
  `self`. They are deferred.
- **Recursive destruction**: simpler, but a list of a million nodes overflows the C stack.

## Consequences

- New diagnostics: E0324 (missing or malformed initializer) and E0325 (field not assigned
  before `self` is used or `new` returns).
- The runtime gains `sag_object`, the header of every instance (strong count, weak count,
  drop function), with `sag_object_retain`/`sag_object_release` and an explicit stack of
  objects to destroy.
- The C backend emits one struct per class plus `sag_new_cN` and `sag_drop_cN`. Instances are
  passed as pointers.
- SIR reads fields of an instance through a `ClassField` projection. Reading a reference-counted
  value from such a field takes an owned copy, because another reference may change the field
  during the statement.
