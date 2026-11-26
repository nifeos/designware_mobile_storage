# Changelog

Dates are UTC. One line per release, what changed and why, so a reader of the diff has the
sentence beside it.

## 0.1.0-pre.3

- Breaking: the `jh7110` module and its `slot_from_node` fuzz target are gone. The crate names no
  system-on-chip; a caller decodes its own device tree and passes the input clock and queue depth
  to `HostController::new` as before.
- Breaking: the `partition` module (`MasterBootRecord`, `PartitionEntry`) is gone, with its two
  harnesses and its fuzz target. Parsing a partition table is not a controller's job, and other
  block drivers need it without this crate; it moves to a partition-table crate of its own.
- Breaking: the eMMC switch path is gone: `sd::switch_multimedia_card_bus_width`,
  `sd::switch_multimedia_card_to_high_speed`, `sd::MultimediaCardSwitchError`, `command::SWITCH`,
  `card::r1_state` and `card::R1_STATE_TRANSFER`, with their two harnesses and the simulated
  card's `CMD6`. It had never run on an eMMC and nothing called it; a public item is easier to add
  once a board has run it than to take back. An eMMC runs on a 1-bit bus at 25 megahertz, as `identify`
  leaves it.

## 0.1.0-pre.2

- Breaking rename: `queue_threshold_for_programmed_input_output` is `queue_threshold_for_programmed_io`,
  and the test that names it follows. The old spelling has no alias.
- The driver clears the 64-bit addressing status register where that mode puts it, and `Slot::from_node`
  has a fuzz target.
- The simulator is under mutation testing, and its own contract has tests.

## 0.1.0-pre.1

- First version.
- A `simulator` feature, off by default, exports the simulated controller and card so a caller's
  own tests can run against them.
- The simulated controller panics after about four million register writes or commands, so a
  driver stuck in a loop fails its test at once instead of filling the host's memory.

## BUGS

- The template's lag check guards the CI pin, not this file; a change that matters to a user of
  the crate belongs in a release line here.
