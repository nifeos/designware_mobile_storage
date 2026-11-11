# Stranger run, 2026-11-11

An agent with no earlier context in this repository cloned it fresh at `c1b58f1` and followed the
README alone to a build, the tests and the proofs. Everything below is what the README did not
say, with the evidence for each.

## The configuration

- An x86_64 Linux host (Ubuntu 26.04), 80 hardware threads and 53 gigabytes of memory.
- Rust stable 1.99.1, with the three bare-metal targets already installed.
- Kani 0.68.0 and its C Bounded Model Checker (CBMC) 6.11.0 already installed, so the README's install steps were not
  measured. `cargo kani --version` matched the version the README names.

## What it took

| step | command | result |
|---|---|---|
| build | `cargo build` | clean, no warnings |
| tests | `cargo test` | 118 unit tests and 4 doctests pass, 1 doctest ignored; `tests/simulator.rs` does not run |
| tests with the feature | `cargo test --features simulator` | 118 unit tests, 7 in `tests/simulator.rs` and 5 doctests pass |
| a bare-metal build | `cargo build --target riscv64imac-unknown-none-elf` | clean |
| proofs | `cargo kani` | 26 of 26 harnesses verified in 40 minutes 11 seconds of wall clock |
| one falsification | `patch -p1 < falsifications/proofs.the_clock_divider_is_total.patch`, then `cargo kani --harness the_clock_divider_is_total` | fails at `(divider == 0) == (base <= target)`, as the README promises |

## Findings

1. **`cargo test` skips the test the README says runs.** The README says that
   `tests/simulator.rs` runs its `first_sector` example word for word. `Cargo.toml` gives that
   target `required-features = ["simulator"]`, so a plain `cargo test` never builds it and prints
   nothing about the skip. The README never says to pass `--features simulator`. A stranger who
   follows it sees green and believes that test passed when it did not run.
2. **No command for the build or the tests.** The README never writes `cargo build` or
   `cargo test`. A Rust reader guesses them correctly, but finding 1 shows the guess is not
   enough.
3. **The proofs' cost is not stated.** `cargo kani` verifies the harnesses one at a time. It took
   40 minutes 11 seconds here; the slowest harness took 342 seconds and the next two 312 and
   296. The README says only that one combined harness once ran past five minutes. It does not
   give the total, the option that runs harnesses in parallel (`--jobs`, which needs
   `--output-format=terse`), or how much memory each harness needs.
4. **No command for one harness.** The README says to apply a falsification patch and that "its
   harness fails". It does not say how to run just that harness. Without
   `cargo kani --harness <name>`, checking one patch costs the whole 40-minute run. The harness
   name is the last dotted part of the patch's file name, which the README also leaves the reader
   to infer.
5. **The falsification patch has drifted.** `patch -p1` applied it with
   `Hunk #1 succeeded at 198 (offset 50 lines)`. It still works, but nothing keeps the context
   lines current.
6. **Nothing explains Kani's warnings.** `cargo kani` prints 14 "trailing semicolon in macro used
   in expression position" future-incompatibility warnings (the first at `src/command.rs:252`, on
   a `const` `assert!`), a warning that Kani does not support `caller_location` or a foreign function
   and that verification "will fail" if either is reachable, and a note on an unstable feature. The
   stable build shows none of them, and the README does not say to expect them. All 26
   harnesses verified, so they are noise here, but a stranger cannot tell that before the run
   ends.
7. **No bare-metal build command.** The README says the crate is `no_std` and targets the JH7110,
   but it does not show a build for that target. `cargo build --target
   riscv64imac-unknown-none-elf` works once `rustup target add` has added the target.

## What the README got right

The dependency lines, the example code, the proof list, the Kani version and its install steps,
and the falsification promise all held. Every harness verified, and the one patch tried broke its
own harness at its own assertion. Nothing in the README was wrong; every finding is an omission.

## What closed the findings

The README now has a "Building and testing" section that covers findings 1 to 4, 6 and 7. The
patch in finding 5 applies without an offset.
