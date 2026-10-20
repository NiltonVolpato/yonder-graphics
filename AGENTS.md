# Agent Guidelines & Project Rules

## API Surface & Exports
* **Do not make everything public!**
  * Default all types, functions, methods, and modules to private (`pub(crate)` or item-private).
  * Only expose what is explicitly required for the public API surface.
* **Do not export symbols under more than one name!**
  * Avoid duplicate aliases, re-exports under multiple paths or conflicting facade names.
  * Maintain a single canonical export location for every public type and function.

## Code Organization & Structure
* **Use directories to group logically related modules in a crate.**
  * Do not keep everything in a flat directory structure.
  * Group related domain functionality (e.g. math, primitives, raster, damage, targets) into appropriate subdirectories with `mod.rs` or standard Rust module hierarchy.

## Core Engineering Principles
* **DRY & SSOT**: Eliminate duplication; maintain a single source of truth.
* **Stop adding superfluous or redundant checks!**
  * Do not add redundant defensive checks across layers (e.g. checking `col < cols && row < rows` and then checking `idx < active_hashes.len()` when `active_hashes.len() >= cols * rows` is an established struct invariant).
  * Rely on structural invariants established at construction (verify via `debug_assert!` if needed).
  * Keep hot paths and inner loops clean and branch-efficient; do not clutter them with paranoid dead-code checks.
* **Architecture Before Implementation**: Draft plans, brainstorm questions, and save detailed plans to artifacts before executing code changes.
* **Two-Pass Method**: First make it work and test it; then refactor architecture without breaking tests.
* **Error Handling**: Never swallow or ignore errors; propagate or handle them explicitly.
* **No Abbreviations**: Use descriptive, readable identifiers (except standard mathematical conventions).
