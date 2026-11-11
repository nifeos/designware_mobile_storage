# designware_mobile_storage

[![check](https://github.com/nifeos/designware_mobile_storage/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/nifeos/designware_mobile_storage/actions/workflows/ci.yml?query=branch%3Amain)
[![proofs](https://github.com/nifeos/designware_mobile_storage/actions/workflows/proofs.yml/badge.svg?branch=main)](https://github.com/nifeos/designware_mobile_storage/actions/workflows/proofs.yml?query=branch%3Amain)
[![license](https://img.shields.io/badge/license-%28MIT%20OR%20Apache--2.0%29%20AND%20ISC-blue.svg)](https://github.com/nifeos/designware_mobile_storage#license)
[![minimum Rust version](https://img.shields.io/badge/rust-1.88%2B-blue.svg)](https://github.com/nifeos/designware_mobile_storage/blob/main/Cargo.toml)

A Secure Digital (SD) card and eMMC (embedded multimedia card) driver for the Synopsys `DesignWare`
Mobile Storage Host Controller (`snps,dw-mshc`, Linux's `dw_mmc`), as pure logic. It holds the
register map, the command encoding and the polled controller sequences. It also holds the internal
direct memory access controller (IDMAC), card identification, block reads and writes, and the
decoding of the card registers. A master boot record (MBR) rule says where a write test may land.

The crate never touches memory itself. You implement one small trait,
`host_controller::RegisterWindowAndClock`, over your own mapped window. It reads a register, writes
one and reads a microsecond clock. The crate is `no_std`, has no allocator, and targets the `StarFive`
JH7110 (`VisionFive` 2).

## EXAMPLES

The crate is not on crates.io. Take it from this repository, pinned at a commit you have read:

```toml
[dependencies]
designware_mobile_storage = { git = "https://github.com/nifeos/designware_mobile_storage", rev = "<commit>" }
```

Hand `HostController::new` your register window, the controller's input clock and the first input,
first output (FIFO) depth from the device tree. Then identify the card and move whole 512-byte
blocks:

```rust
use designware_mobile_storage::host_controller::{HostController, RegisterWindowAndClock};
use designware_mobile_storage::sd;

fn first_sector<R: RegisterWindowAndClock>(registers: R) -> Option<[u8; 512]> {
    let (mut host, _id) = HostController::new(registers, 50_000_000, Some(32)).ok()?;
    let card = sd::identify(&mut host, 4).ok()?; // at most a 4-bit bus
    let mut buffer = [0u8; 512];
    sd::read_blocks(&mut host, &card, 0, &mut buffer).ok()?;
    Some(buffer)
}
```

An eMMC comes up on a 1-bit bus at 25 megahertz. A caller may then ask for a wider bus and high
speed. Neither step has run on a real eMMC yet (see BUGS), and `identify` never takes either:

```rust
use designware_mobile_storage::host_controller::{HostController, RegisterWindowAndClock};
use designware_mobile_storage::sd;

fn faster_embedded_card<R: RegisterWindowAndClock>(host: &mut HostController<R>) -> Option<sd::Card> {
    let mut card = sd::identify(host, 8).ok()?; // the eMMC slot's bus-width
    if sd::switch_multimedia_card_bus_width(host, &mut card, 8).is_err()
        || sd::switch_multimedia_card_to_high_speed(host, &mut card).is_err()
    {
        // A failed switch leaves the card's width and timing unknown: start over at 1 bit.
        card = sd::identify(host, 8).ok()?;
    }
    Some(card)
}
```

`partition::MasterBootRecord::parse` reads that sector. `MasterBootRecord::scratch_sector_range`
names the sectors before the first partition that a write test may use.

`HostController::with_internal_direct_memory_access` moves later transfers through a direct memory
access (DMA) region you supply. The region implements
`internal_direct_memory_access::DirectMemoryAccessRegion`. The `jh7110` module holds that
system-on-chip's constants and `Slot::from_node`. That function decodes one slot from the properties
of a device-tree node you found. The crate reads no device tree itself.

The `simulator` feature lets your own tests run against the simulated controller and card that
this crate's tests use, the way `embedded-hal-mock` stands in for `embedded-hal`. It is off by
default and needs `std`, so turn it on for your tests only. `SimulatedHostController` implements
`RegisterWindowAndClock`, so it goes wherever your code takes a register window. Choose the card's
kind and size, fill or check its blocks, fail the next block read, or leave the slot empty.
`attach_direct_memory_access_region` hands back the memory its IDMAC reaches. A test of the
`first_sector` function above:

```toml
[dev-dependencies]
designware_mobile_storage = { git = "https://github.com/nifeos/designware_mobile_storage", rev = "<commit>", features = ["simulator"] }
```

```rust,ignore
use designware_mobile_storage::SimulatedHostController;
use designware_mobile_storage::sd::CardKind;

#[test]
fn first_sector_reads_what_the_card_holds() {
    let mut simulator = SimulatedHostController::new(CardKind::SdHighCapacity, 8192);
    simulator.card_blocks_mut()[0][510..].copy_from_slice(&[0x55, 0xaa]);
    let sector = first_sector(simulator).expect("the simulated card reads");
    assert_eq!(sector[510..], [0x55, 0xaa]);
}
```

The crate's own `tests/simulator.rs` runs this test word for word.

Every error enum implements `Display` and `core::error::Error`. These are `host_controller::Error`,
`sd::InitializationError`, `sd::BlockTransferError` and `internal_direct_memory_access::Refusal`.
They and `sd::CardKind` are `#[non_exhaustive]`. A `match` on one needs a wildcard arm, and a later
release can add a variant without breaking you.

## What is proven

Each property below has a Kani harness in `src/proofs.rs`, twenty-six in all.

- No partition table makes the write test's scratch range touch a partition or sector 0.
- A FIFO step never moves more than the transfer still owes. A write step never moves more than the
  FIFO has room for, whatever `STATUS_REGISTER` says.
- A block range the driver admits lies inside the card. It also lies inside what the command's
  32-bit argument can name.
- An IDMAC descriptor chain describes exactly the transfer's bytes. It keeps each buffer inside the
  region's data pages and links only to descriptors in its own chain page.
- The MBR parse accepts exactly the sectors that end in the signature. It reads each entry from its
  own bytes and never panics on any sector 0.
- A field read from a 128-bit register never exceeds its width.
- Decoding any card-specific data (CSD) register, as SD or as eMMC, sizes the card without
  overflow. It sizes nothing for an unknown structure or a block length under 512 bytes.
- The `EXT_CSD` sector count comes from bytes 212 to 215 of any buffer. A buffer too short for it
  gives zero.

The harnesses below assume nothing of the controller. Every register read and every descriptor word
in DMA memory is whatever the solver picks. The solver also picks the clock's readings, and the
harnesses assume only that time passes. From some reading on, each reading is more than the
deadline past every earlier one. Before that reading the clock may wrap past 2^64, stand still or
jump either way.

They prove that no path panics, overflows or indexes out of bounds, and that each wait ends. They do
not prove that the driver does the right thing with a faithful controller. The host tests against
the simulated controller cover that. Only a board shows it for silicon.

- A wait ends at the first clock reading past its deadline and reads the clock no more.
- The IDMAC wait ends once the clock has passed its deadline, however the controller rewrites the
  descriptors. The deadline restarts only when one more descriptor has closed, so at most once per
  descriptor in the chain (bounded here at two).
- The card clock divider is total for any base and target: no zero divisor, no overflow, no panic.
  It bypasses the divider exactly when the base is at or below the target.
- A command word carries the start bit and the command's own index. It never carries the
  clock-update-only bit. It has a data phase exactly when the command has one, and the write bit
  only on a write.
- Sending a command with no data phase ends and never panics, whatever the controller answers. Every
  error names the command sent, and a bad response carries the status bit that made it bad. A short
  response leaves words 1 to 3 zero.
- A reset and a card clock change end and never panic. From a 50 megahertz base the card then runs
  at or below any target asked for. When the target is below the divider's range, it runs at the
  slowest rate the divider has.
- A FIFO data phase stays inside its buffer and ends, whatever fill and status the controller
  reports. The proof covers data phases of one or two blocks of 4 or 8 bytes through a FIFO one or
  two words deep. The loop's arithmetic does not depend on the length, and the FIFO step proof above
  holds for any length.
- A two-block data phase through the FIFO that fails ends with `CMD12` sent, unless the controller
  reported its own automatic stop. A card left sending or receiving answers nothing else.
- Reading one block through the IDMAC never sends a write command. The driver touches the DMA region
  only inside it.
- Writing one block through the IDMAC sends at most one write command, of 512 bytes, addressed to
  the block asked for. It sends that command only when the card has that block.
- An eMMC's bus widens on the controller only after the card took the switch: a `CMD6` writing
  `BUS_WIDTH` with a clean R1, then a `CMD13` with no error, no switch error and the transfer
  state. The width it returns is the `Card`'s and no more than the slot allows.
- The card clock rises above 25 megahertz only after the same holds for `HS_TIMING` and an
  `EXT_CSD` read after the switch shows `HS_TIMING` at 1. Both switch harnesses end, never panic
  and never send a write command.
- The steps of identification end, never panic and never send a write command. They are reset,
  `CMD0`, `CMD8`, power-up and addressing on the polled host, then configuration (an eMMC's
  `EXT_CSD` read through the IDMAC). A configured SD card has a capacity and runs no faster than 25
  megahertz. `identify` itself only builds a `Card` from these steps' results. As one harness it ran
  past five minutes.

The harnesses of whole sequences use a clock that passes every deadline at each reading. They bound
their loops by what the controller reports rather than by time. The two wait harnesses above cover a
clock that takes longer, and every wait in the driver is one of those two loops.

The bring-up harnesses below judge `identify` against the card state machine of the SD
specification and the eMMC standard: idle, ready, identification, standby, transfer. The card starts
in any state, with any address and bus width. The controller starts with any bus width and clock, as
an earlier bring-up that failed anywhere might leave them.

For each command the solver picks whether the card answered, stayed silent or had its answer
garbled, and it picks the response words. The controller reports that choice and otherwise carries
out each command at once. A controller that fails some other way reaches the driver as the same
error from the same call, and the harnesses above cover it. The harnesses assume one thing of a
card: an SD card publishes a relative address other than zero.

- Every command `identify` sends is legal in the state the card is in. No step runs before the one
  it needs, and an application command always follows `CMD55`. `CMD9` and `CMD7` go to the card's
  own address, and no command writes.
- A card comes back from `identify` only after it went from `CMD0` through every state to transfer.
  The `Card` names the address, the operation conditions register (OCR) and the kind of the card.
  An SD card has its 512-byte block length and the bus width the slot allows. An eMMC has had its
  `EXT_CSD` read on a 1-bit bus, and the controller runs the transfer clock.
- Whatever a failed bring-up left, the next one resets the controller and powers the slot. It drops
  the bus to 1 bit at the identification clock and sends `CMD0` before anything else. A card it
  hands back runs one bus width on the card, the controller and the `Card`.
- Against a card that reports busy to every `ACMD41` or `CMD1`, power-up ends with
  `InitializationError::PowerUp` and the last OCR. It sends at most one more power-up command once
  the clock has passed its one-second deadline. The clock runs forward at any pace, from any start
  below 2^62.

Install Kani and run them as "Building and testing" below describes.

## Building and testing

```sh
cargo build
cargo test --features simulator
rustup target add riscv64imac-unknown-none-elf
cargo build --lib --target riscv64imac-unknown-none-elf   # the JH7110's target; CI also builds aarch64-unknown-none-softfloat and x86_64-unknown-none
```

Pass `--features simulator` to `cargo test`. Without it, Cargo skips `tests/simulator.rs` (that
target needs the feature) and says nothing about the skip, so a plain `cargo test` looks green
without running the seven simulator tests or the `first_sector` example above. With the feature,
the run is 118 unit tests, 7 tests in `tests/simulator.rs` and 5 documentation tests. Run
`rustup target add` once, before the first bare-metal build. Without it, that build stops with
`can't find crate for core`.

### Running the proofs

The proofs need Kani, which `rustup` does not install. Run `cargo install --locked kani-verifier
--version 0.68.0`, then `cargo kani setup`. The setup step downloads a model checker, so it needs
network access. CI uses version 0.68.0, and another version may verify differently.

Then `cargo kani`, from the crate's directory, runs all twenty-six harnesses one at a time. That
took 40 minutes on an `x86_64` Linux machine with 80 threads. The slowest harness took about 340
seconds and the next two about 310 and 300. The heaviest peaks near 6 gigabytes of memory. CI runs
two at a time with `cargo kani --jobs 2 --output-format=terse`; Kani verifies in parallel only
with terse output, and two heavy harnesses fit a 16 gigabyte machine. To run one harness, give its
full path and `--exact`. Most take seconds:

```sh
cargo kani --harness proofs::the_clock_divider_is_total --exact
```

A passing run prints several things that look alarming. All of them are expected:

- Fourteen `warning: trailing semicolon in macro used in expression position` future-compatibility
  warnings, the first at `src/command.rs:252`. They come from Kani's toolchain, and a stable
  build shows none.
- `warning: use of an unstable feature`, which Kani sets for itself.
- `warning: Found the following unsupported constructs: caller_location (1), foreign function (1)`,
  followed by "Verification will fail if one or more of these constructs is reachable". A harness
  that ends in `VERIFICATION:- SUCCESSFUL` never reached them.
- Many `aborting path on assume(false)` lines. They mark paths that a harness's assumptions rule
  out.
- A check described as unreachable code. It reports code that cannot run, and it is no failure.
- Source paths under `/home/runner/.rustup`, a directory that does not exist on your machine. They
  come from the toolchain Kani ships.

Trust the last line of the run: `Complete - 26 successfully verified harnesses, 0 failures, 26 total.`

### Running one falsification patch

```sh
patch -p1 --no-backup-if-mismatch < falsifications/proofs.the_clock_divider_is_total.patch
cargo kani --harness proofs::the_clock_divider_is_total --exact   # ends VERIFICATION:- FAILED
git checkout -- src
```

The harness name is the part of the patch's file name after `proofs.`. The run ends with
`Complete - 0 successfully verified harnesses, 1 failures, 1 total.`, and the failed check names
the assertion that the patch breaks, here `(divider == 0) == (base <= target)`.
`git checkout -- src` reverts the patch.

## Implementing the traits

`host_controller::RegisterWindowAndClock` is the register window and a clock, and each trait's
documentation states its contract in a `# Contract` section. In short:

- Each `read` or `write` is one 32-bit access at the window's base plus the offset, made when the
  call is made. Nothing may merge, split, repeat, drop or reorder it against another access to the
  window: a read of the data FIFO's offset takes a word out of the FIFO, and several writes start
  the controller. Use volatile accesses to a device mapping. Every offset the driver passes is a
  multiple of 4 below `registers::USED_REGISTER_WINDOW_BYTES`.
- `now_microseconds` counts microseconds from any fixed point and may wrap. It must keep
  advancing, because every wait in the driver ends only when the clock passes its deadline.

`internal_direct_memory_access::DirectMemoryAccessRegion` is the memory the IDMAC reads and writes,
used only after `HostController::with_internal_direct_memory_access`. The region starts on a page
(`internal_direct_memory_access::PAGE_BYTES`). It holds one page of descriptors, then 1 to
`MAXIMUM_DESCRIPTORS` data pages, all below 4 GiB as the controller addresses them. The driver
calls `barrier` twice in each transfer. The first call comes after it writes the descriptors and
the data, before the register writes that start the controller. The second comes after the
controller has finished, before the driver reads the data back. While the transfer runs, the
driver polls each descriptor's own bit with `read_u32`, with no barrier in between.

- Where the DMA is coherent with the processor's caches, the accessors are plain loads and stores
  to the region. `barrier` is a fence that orders memory accesses against device accesses, such
  as `fence iorw, iorw` on RISC-V or `dsb sy` on Arm.
- Where it is not coherent, map the region uncached and keep `barrier` that fence. A cached
  mapping works only if `barrier` writes the region's dirty cache lines back to memory and
  invalidates them, and `read_u32` and `read_bytes` invalidate the lines they read first. The
  driver does no cache maintenance of its own.

## BUGS

- `identify` runs default speed only: 25 megahertz, 4-bit SD, 1-bit eMMC. No tuning, ultra high
  speed (UHS), double data rate (DDR) or the 200 megahertz eMMC timing.
- An eMMC's 4- and 8-bit bus (`sd::switch_multimedia_card_bus_width`) and high-speed timing at up
  to 52 megahertz (`sd::switch_multimedia_card_to_high_speed`) are implemented but unconfirmed on
  hardware. They have run only against the simulated card, and only a caller that calls them gets
  them. Until a board with an eMMC module passes `docs/bench-emmc-bus-modes.md`, every eMMC is
  known to work on a 1-bit bus at 25 megahertz only.
- Polled, with interrupts masked. `HostController::is_card_present` reads the card-detect line when
  asked, and nothing watches it. Hot removal is untested on a board, and a card pulled during a
  transfer surfaces as `NoResponse`. No write-protect handling.
- After a failed multi-block transfer the driver sends `CMD12` itself. That sequence has run only
  against the simulated card, until a board confirms it on a read interrupted by an error.
- The default deadlines come from the SD specification, not from busy times measured on a board.
  `HostController::set_deadlines` lengthens them.
- The driver refuses a 16- or 64-bit FIFO. The IDMAC builds only 32-bit descriptors, so its region
  must sit below 4 GiB.
- The polled path has read a real card. The IDMAC path has so far run only against the simulated
  controller in the tests.
- The IDMAC moves at most its region's data pages per command and copies through them once each way.
- The proof that the card clock stays at or below the target covers a 50 megahertz base only. For a
  base the solver picks, the comparison builds two symbolic 32-bit divisions. That ran past five
  minutes with every solver tried and with the base bounded to 2^28.
- A clock that never passes a deadline hangs the wait on it. The trait promises microseconds, and
  the driver has no other way to tell a slow controller from a dead one.
- The block harnesses move one block, and through the IDMAC only. The proof of the polled path's FIFO
  loop covers data phases of 16 bytes or less, because a 512-byte block took too many FIFO steps for
  the solver. The host tests move whole blocks and multi-block transfers when reading and when writing. They use a 32-
  and a 48-word FIFO and a confused fill count.
- The proofs show that identification walks the card state machine in order. Nothing proves that it
  reads the right capacity or tells SD from SD High Capacity (SDHC). The host tests check that
  against simulated SD, SDHC and eMMC cards.
- The MBR parse does not know the card's size. The layer that mounts a partition checks whether it
  fits the card.
- The driver takes the address an SD card publishes with `CMD3` as it comes. A card that published
  zero would get `CMD7` at zero, which deselects every card. The bring-up proofs assume a card
  publishes a nonzero address.

## Minimum Rust version

Rust 1.88. To check it, install that toolchain and a bare-metal target, then test and build with it:

```sh
rustup toolchain install 1.88 --profile minimal --target riscv64imac-unknown-none-elf
cargo +1.88 test --features simulator
cargo +1.88 build --lib --target riscv64imac-unknown-none-elf
```

## License

Licensed under either of the MIT license (`LICENSE-MIT`) or the Apache License, Version 2.0
(`LICENSE-APACHE`), at your option. Parts adapt OpenBSD's `sys/dev/fdt/dwmmc.c` (revision 1.33)
under the Internet Systems Consortium (ISC) license: the register map, the reset, clock and bus
width sequences, the command flags and the polled FIFO transfer. `LICENSE-ISC` holds that notice,
and `src/lib.rs` carries it too. The manifest's SPDX expression is `(MIT OR Apache-2.0) AND ISC`.
Card identification follows the SD specification. It takes only the order of its addressing
commands from OpenBSD's `sdmmc_mem.c`, whose code the crate does not adapt. GPL sources (Linux,
U-Boot, the device trees) served only as references for hardware facts, and the crate copies no
GPL code.
