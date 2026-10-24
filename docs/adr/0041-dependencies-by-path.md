# 0041. Dependencies by path

- Status: Accepted
- Date: 2026-10-24
- Builds on: [ADR-0040](0040-nova-package-manager.md); refines [ADR-0010](0010-modules-and-packages.md)
  (dependency names as path roots, no cycles between packages)

## Context

F3 increment 5 lets packages use libraries. ADR-0010 planned dependency names as path roots and
forbade cycles between packages. The language has no module visibility, no re-exports and no
separate compilation yet. Increment 5 settles Q126–Q139.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q126 | Sources | Only by path: `util = { path = "../util" }`, relative to the manifest's directory or absolute. The TOML subset gains one-line inline tables. Git and registries come later. |
| Q127 | Names | The key must be the dependency's `name`; renaming is deferred. |
| Q128 | Kind | A dependency must be a library. |
| Q129 | In code | A dependency's name is a path root, like `std`. A module-level item or import with the same name hides it. |
| Q130 | Visibility | Only `pub` items are usable from other packages. All modules of a dependency are reachable by path; re-exports stay deferred. |
| Q131 | Transitive dependencies | Allowed. Each package sees only its direct dependencies by name. One directory reached by several paths (compared canonically) is one package, compiled once. |
| Q132 | Cycles | An error showing the chain: `app -> util -> app`. |
| Q133 | `Nova.lock` | Deferred to dependencies from git or a registry; with paths it would only repeat the manifests. |
| Q134 | Compilation | The whole graph is compiled together into one C program. Each package has its own module tree, and `pkg` names its own root. Separate compilation is left for later. |
| Q135 | Diagnostics | Errors and warnings in a dependency are shown with its paths. |
| Q136 | `sag` | `CompileOptions` takes the dependencies (name, root module, their dependencies); only Nova fills them. |
| Q137 | Deferred | Development dependencies, features, optional dependencies, `nova add`, `nova tree` and renaming. |
| Q138 | Nova errors | `N0010` no package at the path, `N0011` another name, `N0012` not a library, `N0013` a cycle, each pointing into the manifest that causes it. Dependencies by `version` or `git` are `N0008`. |
| Q139 | Tests and documentation | Nova integration tests with several packages (direct, transitive, shared, private items, hiding, each error), this ADR, and updates to the specification, `docs/nova.md` and the roadmap. |

## Alternatives considered

- **An error when a local name equals a dependency's:** safer, but a new dependency could break
  code that never uses it.
- **Separate compilation of each package:** needs a stable interface format and generic code in
  it. Compiling the graph together is what the compiler already does for modules.
- **A lockfile now:** nothing to lock with paths.

## Consequences

- The AST and the resolver know packages (`Package::packages`, `Module::package`,
  `Resolution::package_roots`); `ModuleInfo::display_path` shows a dependency's modules with its
  name (`util::shapes`).
- Privacy between packages reuses the module rule: a dependency's module is not within the
  dependent's tree, so its private items are reported as private to their module.
- Messages show dependency paths normalized lexically (`../geo/../math` becomes `../math`).
