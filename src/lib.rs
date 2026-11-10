#![cfg_attr(not(any(test, feature = "simulator")), no_std)]
#![doc = include_str!("../README.md")]

// Adapted from OpenBSD's `sys/dev/fdt/dwmmc.c` revision 1.33 (register map, reset, clock, bus
// width, command flags and the polled first input, first output (FIFO) transfer), under the
// Internet Systems Consortium (ISC) license, whose notice must be kept: it follows, and
// `LICENSE-ISC` at the repository root holds the same text. Card identification (`sd`) follows the
// SD specification's sequence and takes only the order of its addressing commands from OpenBSD's
// `sys/dev/sdmmc/sdmmc_mem.c` (`openbsd/src@21ea132db7ea`, read 2026-11-04 UTC); none of that
// file's code or text is adapted, so its notice is not carried. GPL sources (Linux, U-Boot, the
// device trees) were read only for hardware facts; no GPL code was copied.
//
// SPDX-License-Identifier: ISC
//
// Copyright (c) 2017 Mark Kettenis
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

pub mod card;
pub mod command;
pub mod host_controller;
#[doc(alias = "idmac")]
pub mod internal_direct_memory_access;
pub mod jh7110;
pub mod partition;
pub mod registers;
pub mod sd;

// The simulation and every knob the crate's own tests turn; `exported_simulator` wraps the part a
// caller's tests may use, so the knobs the feature leaves private are unused outside the tests.
#[cfg(any(test, feature = "simulator"))]
#[cfg_attr(not(test), allow(dead_code))]
mod simulator;

#[cfg(feature = "simulator")]
mod exported_simulator;
#[cfg(feature = "simulator")]
pub use exported_simulator::{SimulatedDirectMemoryAccessRegion, SimulatedHostController};

#[cfg(test)]
mod tests;

#[cfg(kani)]
mod proofs;
