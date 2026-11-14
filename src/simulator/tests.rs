//! The simulator's own contract, driven through its registers alone. The driver's tests use only
//! the paths a working driver takes, so they never see the simulator refuse a command in the wrong
//! card state, hold the FIFO to its depth, or stop the IDMAC where a test asked it to. A caller's
//! test relies on each of those to catch a driver bug, so each is checked here.

use std::vec::Vec;

use super::*;
use crate::internal_direct_memory_access::DESCRIPTOR_FIRST;
use crate::registers::*;

/// The bus address of the memory the IDMAC tests attach. Descriptors sit at its start, buffers
/// from its second page.
const BASE: u32 = 0x8000_0000;
/// The `CMD7` and `CMD9` argument for an SD card here: the relative address it chooses, in bits 31:16.
const SD_SELECT_ARGUMENT: u32 = 0xb368 << 16;
/// The same for an eMMC: the relative address these tests give it.
const MULTIMEDIA_CARD_SELECT_ARGUMENT: u32 = 1 << 16;
/// R1's current-state field (bits 12:9) for the transfer state, with `READY_FOR_DATA` (bit 8).
const TRANSFER_READY: u32 = 4 << 9 | 1 << 8;

/// A controller with its card clock on and divided, holding `kind` of card, which powers up on
/// its first poll. Block `n` holds bytes counting up from `n`.
fn simulator(kind: CardKind, depth: usize) -> SimulatedHostController {
    let mut s = SimulatedHostController::new(kind, 1024, depth);
    s.busy_polls = 0;
    // Byte `i` of block `n` is `(i + n) as u8`, so every block is a window of one repeating run.
    // Copying windows keeps this cheap under Miri, where a per-byte fill of every block took
    // minutes per test.
    let run: [u8; 512 + 256] = core::array::from_fn(|i| i as u8);
    for (n, block) in s.storage.iter_mut().enumerate() {
        let start = n % 256;
        block.copy_from_slice(&run[start..start + 512]);
    }
    s.write(CLOCK_DIVIDER_REGISTER, 1);
    s.write(CLOCK_ENABLE_REGISTER, CLOCK_ENABLE_CARD_CLOCK);
    s
}

/// Send command `index` with `argument`, expecting a response, plus `flags`: the raw interrupt
/// status after one reading, and the response's first word unless the command timed out.
fn command(
    s: &mut SimulatedHostController,
    index: u32,
    argument: u32,
    flags: u32,
) -> (u32, Option<u32>) {
    s.write(RAW_INTERRUPT_STATUS_REGISTER, RAW_INTERRUPT_ALL_MASK);
    s.write(COMMAND_ARGUMENT_REGISTER, argument);
    s.write(
        COMMAND_REGISTER,
        COMMAND_START | COMMAND_RESPONSE_EXPECT | flags | index,
    );
    let status = s.read(RAW_INTERRUPT_STATUS_REGISTER);
    assert_ne!(
        status & RAW_INTERRUPT_COMMAND_DONE,
        0,
        "CMD{index} never finished"
    );
    let answer =
        (status & RAW_INTERRUPT_RESPONSE_TIMEOUT == 0).then(|| s.read(RESPONSE_0_REGISTER));
    (status, answer)
}

/// The response's first word to command `index`, or `None` when the card does not answer.
fn send(s: &mut SimulatedHostController, index: u32, argument: u32) -> Option<u32> {
    command(s, index, argument, 0).1
}

/// Reset the card and power it up: the card is then in the ready state.
fn ready(s: &mut SimulatedHostController) {
    send(s, 0, 0);
    if s.kind == CardKind::MultimediaCard {
        assert!(send(s, 1, 0x40ff_8000).is_some());
    } else {
        send(s, 8, 0x1aa);
        assert!(send(s, 55, 0).is_some());
        assert!(send(s, 41, 1 << 30).is_some());
    }
}

/// Take the card to the standby state, and return its relative address as `CMD7` takes it.
fn standby(s: &mut SimulatedHostController) -> u32 {
    ready(s);
    assert!(command(s, 2, 0, COMMAND_LONG_RESPONSE).1.is_some());
    if s.kind == CardKind::MultimediaCard {
        assert!(send(s, 3, MULTIMEDIA_CARD_SELECT_ARGUMENT).is_some());
        MULTIMEDIA_CARD_SELECT_ARGUMENT
    } else {
        assert_eq!(send(s, 3, 0), Some(SD_SELECT_ARGUMENT | 0x0500));
        SD_SELECT_ARGUMENT
    }
}

/// Take the card to the transfer state.
fn selected(s: &mut SimulatedHostController) {
    let select_argument = standby(s);
    assert!(send(s, 7, select_argument).is_some());
}

/// R1's current-state field, from the status `CMD13` answers.
fn card_state(s: &mut SimulatedHostController) -> Option<u32> {
    send(s, 13, 0).map(|r1| r1 >> 9 & 0xf)
}

/// Start a read of `bytes` from block 0 (`CMD17`, or `CMD18` with more than a block).
fn start_read(s: &mut SimulatedHostController, bytes: u32, flags: u32) -> u32 {
    s.write(BLOCK_SIZE_BYTES_REGISTER, 512);
    s.write(BYTE_COUNT_REGISTER, bytes);
    let index = if bytes > 512 { 18 } else { 17 };
    command(s, index, 0, COMMAND_DATA_EXPECTED | flags).0
}

/// Read `words` words out of the FIFO, as fast as the card fills it.
fn drain(s: &mut SimulatedHostController, words: usize) -> Vec<u32> {
    let queue = queue_offset(SIMULATED_VERSION_IDENTIFICATION);
    let mut out = Vec::new();
    for _ in 0..words {
        let filled = queue_count(s.read(STATUS_REGISTER)) as usize;
        for _ in 0..filled.min(words - out.len()) {
            out.push(s.read(queue));
        }
        if out.len() == words {
            break;
        }
    }
    out
}

/// Block `n` as the card holds it, in FIFO words.
fn block_words(s: &SimulatedHostController, n: usize) -> Vec<u32> {
    let (words, _) = s.storage[n].as_chunks::<4>();
    words.iter().map(|w| u32::from_le_bytes(*w)).collect()
}

/// Write `data` to block `block` with `CMD24` through the FIFO.
fn write_block(s: &mut SimulatedHostController, block: u32, data: &[u8; 512]) {
    s.write(BLOCK_SIZE_BYTES_REGISTER, 512);
    s.write(BYTE_COUNT_REGISTER, 512);
    command(s, 24, block, COMMAND_DATA_EXPECTED | COMMAND_WRITE);
    let queue = queue_offset(SIMULATED_VERSION_IDENTIFICATION);
    for chunk in data.as_chunks::<4>().0.chunks(s.queue_depth_words()) {
        for word in chunk {
            s.write(queue, u32::from_le_bytes(*word));
        }
        s.read(STATUS_REGISTER);
    }
    assert_ne!(
        s.read(RAW_INTERRUPT_STATUS_REGISTER) & RAW_INTERRUPT_DATA_TRANSFER_OVER,
        0
    );
}

#[test]
fn peek_and_poke_skip_the_side_effects_and_the_depth_is_the_one_built() {
    let mut s = SimulatedHostController::new(CardKind::SdHighCapacity, 8, 24);
    assert_eq!(s.queue_depth_words(), 24);
    s.poke(CONTROL_REGISTER, CONTROL_CONTROLLER_RESET);
    assert_eq!(s.peek(CONTROL_REGISTER), CONTROL_CONTROLLER_RESET);
    // A poked reset is not a requested one, so it never clears.
    assert_eq!(s.read(CONTROL_REGISTER), CONTROL_CONTROLLER_RESET);
    assert_eq!(s.read(CONTROL_REGISTER), CONTROL_CONTROLLER_RESET);
}

#[test]
fn version_and_hardware_configuration_ignore_writes() {
    let mut s = SimulatedHostController::new(CardKind::SdHighCapacity, 8, 32);
    s.write(VERSION_IDENTIFICATION_REGISTER, 0);
    s.write(HARDWARE_CONFIGURATION_REGISTER, 0);
    assert_eq!(
        s.read(VERSION_IDENTIFICATION_REGISTER),
        SIMULATED_VERSION_IDENTIFICATION
    );
    assert_eq!(
        s.read(HARDWARE_CONFIGURATION_REGISTER),
        SIMULATED_HARDWARE_CONFIGURATION
    );
}

#[test]
fn a_command_register_write_without_the_start_bit_sends_nothing() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    s.write(COMMAND_REGISTER, COMMAND_RESPONSE_EXPECT | 13);
    assert!(s.commands.is_empty());
    s.write(
        COMMAND_REGISTER,
        COMMAND_START | COMMAND_RESPONSE_EXPECT | 13,
    );
    assert_eq!(s.commands, [(13, 0)]);
}

#[test]
fn a_command_sent_with_the_card_clock_off_is_flagged() {
    let mut s = SimulatedHostController::new(CardKind::SdHighCapacity, 8, 32);
    send(&mut s, 0, 0);
    assert!(s.clockless_command);
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    send(&mut s, 0, 0);
    assert!(!s.clockless_command);
}

/// An SD card answers each command only in the state the specification gives it, so a driver
/// that skips or reorders a step of identification gets silence, as from a real card.
#[test]
fn an_sd_card_answers_each_command_only_in_its_own_state() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    // Idle: no identification or relative address before power-up.
    assert_eq!(send(&mut s, 2, 0), None);
    assert_eq!(send(&mut s, 3, 0), None);
    ready(&mut s);
    // Ready: the interface condition is asked in idle only, and the address follows the CID.
    assert_eq!(send(&mut s, 8, 0x1aa), None);
    assert_eq!(send(&mut s, 3, 0), None);
    assert!(command(&mut s, 2, 0, COMMAND_LONG_RESPONSE).1.is_some());
    assert_eq!(send(&mut s, 2, 0), None);
    assert_eq!(send(&mut s, 3, 0), Some(SD_SELECT_ARGUMENT | 0x0500));
    // Standby: the CSD and selection answer the card's own address only, and nothing of the
    // transfer state answers yet.
    assert_eq!(send(&mut s, 9, SD_SELECT_ARGUMENT + (1 << 16)), None);
    assert_eq!(send(&mut s, 7, SD_SELECT_ARGUMENT + (1 << 16)), None);
    for index in [12, 13, 16] {
        assert_eq!(send(&mut s, index, 0), None, "CMD{index} in standby");
    }
    assert!(send(&mut s, 55, 0).is_some());
    assert_eq!(send(&mut s, 6, 2), None, "ACMD6 in standby");
    assert!(send(&mut s, 9, SD_SELECT_ARGUMENT).is_some());
    assert!(send(&mut s, 7, SD_SELECT_ARGUMENT).is_some());
    // Transfer: the status is exact, an application command says so, and the identification
    // commands and a stop with nothing to stop get no answer.
    assert_eq!(send(&mut s, 13, 0), Some(TRANSFER_READY));
    assert_eq!(send(&mut s, 55, 0), Some(TRANSFER_READY | 1 << 5));
    assert_eq!(send(&mut s, 6, 2), Some(TRANSFER_READY));
    assert_eq!(send(&mut s, 9, SD_SELECT_ARGUMENT), None);
    assert_eq!(send(&mut s, 8, 0x1aa), None);
    assert_eq!(send(&mut s, 12, 0), None);
}

#[test]
fn an_embedded_multimedia_card_answers_each_command_only_in_its_own_state() {
    let mut s = simulator(CardKind::MultimediaCard, 32);
    ready(&mut s);
    assert_eq!(send(&mut s, 3, MULTIMEDIA_CARD_SELECT_ARGUMENT), None);
    let mut s = simulator(CardKind::MultimediaCard, 32);
    standby(&mut s);
    assert_eq!(send(&mut s, 6, 0x03b7_0100), None);
    assert_eq!(send(&mut s, 13, 0), None);
    assert!(send(&mut s, 7, MULTIMEDIA_CARD_SELECT_ARGUMENT).is_some());
    assert_eq!(send(&mut s, 13, 0), Some(TRANSFER_READY));
}

/// `CMD18` keeps the card sending and `CMD25` receiving until `CMD12`, which a status in between
/// reports, so a driver that never stops a multiple-block transfer is caught.
#[test]
fn a_multiple_block_command_holds_the_card_in_a_data_state_until_the_stop() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    selected(&mut s);
    for (index, data_state) in [(18, 5), (25, 6)] {
        assert_eq!(send(&mut s, index, 0), Some(TRANSFER_READY));
        assert_eq!(card_state(&mut s), Some(data_state), "after CMD{index}");
        assert_eq!(send(&mut s, 12, 0), Some(data_state << 9 | 1 << 8));
        assert_eq!(card_state(&mut s), Some(4), "after CMD{index} and CMD12");
    }
}

/// The capacity bit (30) of the operation conditions is set only for a high capacity card, and for
/// an SD card only when the host's `ACMD41` asked for it (`HCS`); an eMMC reports it to `CMD1`.
#[test]
fn only_a_card_asked_for_high_capacity_reports_it() {
    let power_up = |kind, argument| {
        let mut s = simulator(kind, 32);
        send(&mut s, 0, 0);
        let operation_conditions = if kind == CardKind::MultimediaCard {
            send(&mut s, 1, argument)
        } else {
            send(&mut s, 55, 0);
            send(&mut s, 41, argument)
        };
        operation_conditions.unwrap()
    };
    let high_capacity = |ocr: u32| {
        assert_ne!(ocr & 1 << 31, 0, "power-up never finished");
        ocr & 1 << 30 != 0
    };
    assert!(high_capacity(power_up(CardKind::SdHighCapacity, 1 << 30)));
    assert!(!high_capacity(power_up(CardKind::SdHighCapacity, 0)));
    assert!(!high_capacity(power_up(CardKind::SdVersion2, 1 << 30)));
    assert!(high_capacity(power_up(CardKind::MultimediaCard, 0)));
}

/// An SDHC card's CSD is structure version 2.0, whose `C_SIZE` (bits 69:48) counts 512 KiB less one.
#[test]
fn a_high_capacity_card_reports_a_version_2_card_specific_data() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    s.storage = vec![[0; 512]; 8192];
    let select_argument = standby(&mut s);
    command(&mut s, 9, select_argument, COMMAND_LONG_RESPONSE);
    let words = [
        RESPONSE_0_REGISTER,
        RESPONSE_1_REGISTER,
        RESPONSE_2_REGISTER,
        RESPONSE_3_REGISTER,
    ]
    .map(|r| s.peek(r));
    assert_eq!(words[3] >> 30, 1);
    assert_eq!(words[1] >> 16 | (words[2] & 0x3f) << 16, 8192 / 1024 - 1);
}

/// A wrong echo must differ from the check pattern whatever the pattern, or a test of the
/// driver's refusal would pass for the wrong reason.
#[test]
fn an_injected_wrong_echo_differs_from_every_check_pattern() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    s.faults = (0..3).map(|n| (8, n, Fault::EchoWrong)).collect();
    for pattern in [0x00, 0xaa, 0xff] {
        send(&mut s, 0, 0);
        let echo = send(&mut s, 8, 0x100 | pattern).unwrap();
        assert_ne!(echo & 0xff, pattern);
    }
}

/// With no lateness set the answer is in place as the command is written; with some, it lands on
/// exactly the reading of `RINTSTS` promised.
#[test]
fn a_late_answer_lands_on_the_reading_promised_and_a_prompt_one_at_once() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    s.write(
        COMMAND_REGISTER,
        COMMAND_START | COMMAND_RESPONSE_EXPECT | 55,
    );
    assert_ne!(
        s.peek(RAW_INTERRUPT_STATUS_REGISTER) & RAW_INTERRUPT_COMMAND_DONE,
        0
    );
    assert_eq!(s.peek(RESPONSE_0_REGISTER), 1 << 8 | 1 << 5);
    s.late_answer = 2;
    s.write(RAW_INTERRUPT_STATUS_REGISTER, RAW_INTERRUPT_ALL_MASK);
    s.write(
        COMMAND_REGISTER,
        COMMAND_START | COMMAND_RESPONSE_EXPECT | 13,
    );
    assert_eq!(s.read(RAW_INTERRUPT_STATUS_REGISTER), 0);
    assert_eq!(
        s.read(RAW_INTERRUPT_STATUS_REGISTER),
        RAW_INTERRUPT_COMMAND_DONE | RAW_INTERRUPT_RESPONSE_TIMEOUT
    );
}

/// The automatic `CMD12` returns the card to the transfer state, and its `ACD` lands as many
/// readings after `DTO` as the test asked.
#[test]
fn an_automatic_stop_lands_its_done_as_late_as_asked_and_ends_the_data_state() {
    let mut s = simulator(CardKind::SdHighCapacity, 256);
    selected(&mut s);
    s.late_automatic_command_done = 2;
    let done = RAW_INTERRUPT_AUTOMATIC_COMMAND_DONE;
    let status = start_read(&mut s, 1024, COMMAND_SEND_AUTOMATIC_STOP);
    assert_eq!(
        status & (RAW_INTERRUPT_DATA_TRANSFER_OVER | done),
        RAW_INTERRUPT_DATA_TRANSFER_OVER
    );
    assert_eq!(s.read(RAW_INTERRUPT_STATUS_REGISTER) & done, 0);
    assert_eq!(s.read(RAW_INTERRUPT_STATUS_REGISTER) & done, done);
    assert_eq!(s.commands.last(), Some(&(12, 0)));
    assert_eq!(drain(&mut s, 256).len(), 256);
    assert_eq!(card_state(&mut s), Some(4));
}

/// The FIFO holds at most its depth, a read shorter than the FIFO lands whole with `DTO`, the card
/// sends at most what a stall allows, and a stalled card raises neither `RXDR` with nothing to
/// read nor `DTO` unless it ends the transfer early.
#[test]
fn the_receive_queue_holds_only_what_its_depth_and_the_card_allow() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    selected(&mut s);
    start_read(&mut s, 1024, 0);
    assert_eq!(queue_count(s.read(STATUS_REGISTER)), 32);
    let mut s = simulator(CardKind::SdHighCapacity, 256);
    selected(&mut s);
    let status = start_read(&mut s, 512, 0);
    assert_ne!(status & RAW_INTERRUPT_DATA_TRANSFER_OVER, 0);
    assert_eq!(queue_count(s.read(STATUS_REGISTER)), 128);
    assert_eq!(drain(&mut s, 128), block_words(&s, 0));
    for (end_early, stall) in [(false, 64), (true, 64), (false, 0)] {
        let mut s = simulator(CardKind::SdHighCapacity, 32);
        selected(&mut s);
        s.stall_after = Some(stall);
        s.end_early = end_early;
        let status = start_read(&mut s, 512, 0);
        assert_eq!(queue_count(s.read(STATUS_REGISTER)) as usize, stall / 4);
        let over = status & RAW_INTERRUPT_DATA_TRANSFER_OVER != 0;
        assert_eq!(over, end_early, "end_early {end_early}, stall at {stall}");
        let request = status & RAW_INTERRUPT_RECEIVE_QUEUE_DATA_REQUEST != 0;
        assert_eq!(request, stall > 0, "stall at {stall}");
    }
}

/// `CARDTHRCTL` enabled above the FIFO's size in bytes ends the read in an overrun; at or below
/// it, disabled, or on a release whose FIFO sits where `CARDTHRCTL` would be, nothing happens.
#[test]
fn a_read_threshold_beyond_the_queue_overruns_and_no_other_does() {
    let overruns = |threshold: u32, enable: u32, version_identification| {
        let mut s = simulator(CardKind::SdHighCapacity, 32)
            .with_version_identification(version_identification);
        selected(&mut s);
        s.poke(CARD_THRESHOLD_CONTROL_REGISTER, threshold << 16 | enable);
        let status = start_read(&mut s, 512, 0);
        let both = RAW_INTERRUPT_QUEUE_UNDERRUN_OR_OVERRUN | RAW_INTERRUPT_DATA_TRANSFER_OVER;
        match status & both {
            0 => false,
            b if b == both => true,
            _ => panic!("an overrun raises both FRUN and DTO, got {status:#x}"),
        }
    };
    let enabled = CARD_THRESHOLD_CONTROL_READ_THRESHOLD_ENABLE;
    let current = SIMULATED_VERSION_IDENTIFICATION;
    // 32 words are 128 bytes.
    assert!(overruns(132, enabled, current));
    assert!(!overruns(128, enabled, current));
    assert!(!overruns(64, enabled, current));
    assert!(!overruns(512, 0, current));
    assert!(!overruns(512, enabled, 0x5342_230a));
}

/// A card failed into read-only mode keeps only its first writes, and a mangled write hits only
/// the write command it names.
#[test]
fn written_blocks_are_stored_lost_or_mangled_by_write_command() {
    let data: [u8; 512] = core::array::from_fn(|i| 0xff - i as u8);
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    selected(&mut s);
    s.writes_stored = Some(1);
    let lost = s.storage[4];
    write_block(&mut s, 3, &data);
    write_block(&mut s, 4, &data);
    assert_eq!(s.written, [3, 4]);
    assert_eq!((s.storage[3], s.storage[4]), (data, lost));
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    selected(&mut s);
    s.mangle_write = Some((1, 4));
    write_block(&mut s, 4, &data);
    assert_eq!(s.storage[4], data);
    write_block(&mut s, 4, &data);
    assert_eq!(s.storage[4][0], data[0] ^ 0xff);
    assert_eq!(s.storage[4][1..], data[1..]);
}

/// An eMMC drives the width its last `CMD6` set, back to one bit too; a controller listening on
/// another width reads every byte wrong; and a `CMD6` that is not a byte write is refused.
#[test]
fn an_embedded_multimedia_card_reads_true_only_on_the_width_its_switch_set() {
    let mut s = simulator(CardKind::MultimediaCard, 32);
    selected(&mut s);
    let switch_error = |s: &mut SimulatedHostController, argument| {
        assert_eq!(send(s, 6, argument), Some(TRANSFER_READY));
        send(s, 13, 0).unwrap() & 1 << 7 != 0
    };
    let read = |s: &mut SimulatedHostController, card_type| {
        s.write(CARD_TYPE_REGISTER, card_type);
        start_read(s, 512, 0);
        drain(s, 128)
    };
    let want = block_words(&s, 0);
    assert_eq!(read(&mut s, 0), want);
    let wrong = read(&mut s, CARD_TYPE_4_BIT_BUS);
    let bytes = |words: &[u32]| {
        words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>()
    };
    assert!(bytes(&wrong).iter().zip(bytes(&want)).all(|(a, b)| *a != b));
    assert!(!switch_error(&mut s, 0x03b7_0100));
    assert_eq!(read(&mut s, CARD_TYPE_4_BIT_BUS), want);
    assert!(!switch_error(&mut s, 0x03b7_0000));
    assert_eq!(read(&mut s, 0), want);
    // Access 1 sets bits rather than writing the byte.
    assert!(switch_error(&mut s, 0x01b7_0100));
    assert_eq!(read(&mut s, 0), want);
}

/// A memory of two descriptor pages and four data pages behind `s`, the IDMAC pointed at its first
/// descriptor and selected and enabled as `control` and `bus_mode` say.
fn attach(s: &mut SimulatedHostController, control: u32, bus_mode: u32) -> Memory {
    let memory = Memory::new(u64::from(BASE), 6);
    s.memory = Some(memory.clone());
    s.write(DESCRIPTOR_LIST_BASE_PHYSICAL_ADDRESS_REGISTER, BASE);
    s.write(CONTROL_REGISTER, control);
    s.write(BUS_MODE_REGISTER, bus_mode);
    memory
}

const INTERNAL_DIRECT_MEMORY_ACCESS_ON: u32 =
    CONTROL_USE_INTERNAL_DIRECT_MEMORY_ACCESS_CONTROLLER | CONTROL_DIRECT_MEMORY_ACCESS_ENABLE;

/// Write the descriptor at byte `at` of `memory`: `word_0`, a buffer of `bytes` at byte `buffer`,
/// and the next descriptor at byte `next`.
fn put_descriptor(memory: &Memory, at: usize, word_0: u32, bytes: u32, buffer: u32, next: u32) {
    let words = [word_0, bytes, BASE + buffer, BASE + next];
    let mut bus = memory.bus();
    for (i, w) in words.iter().enumerate() {
        bus.bytes[at + 4 * i..at + 4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
}

fn memory_words(memory: &Memory, at: usize, words: usize) -> Vec<u32> {
    let bus = memory.bus();
    let (chunks, _) = bus.bytes[at..at + 4 * words].as_chunks::<4>();
    chunks.iter().map(|w| u32::from_le_bytes(*w)).collect()
}

/// Let the simulated hardware run: the IDMAC moves words on every register reading.
fn run(s: &mut SimulatedHostController) {
    for _ in 0..64 {
        s.read(RAW_INTERRUPT_STATUS_REGISTER);
    }
}

const ONE_BLOCK: u32 = DESCRIPTOR_OWNED_BY_CONTROLLER | DESCRIPTOR_FIRST | DESCRIPTOR_LAST;

#[test]
fn the_internal_direct_memory_access_engine_walks_only_when_control_selects_and_enables_it_and_bus_mode_enables_it()
 {
    let enable = BUS_MODE_INTERNAL_DIRECT_MEMORY_ACCESS_ENABLE;
    for (control, bus_mode, walks) in [
        (INTERNAL_DIRECT_MEMORY_ACCESS_ON, enable, true),
        (
            CONTROL_USE_INTERNAL_DIRECT_MEMORY_ACCESS_CONTROLLER,
            enable,
            false,
        ),
        (CONTROL_DIRECT_MEMORY_ACCESS_ENABLE, enable, false),
        (INTERNAL_DIRECT_MEMORY_ACCESS_ON, 0, false),
        (0, enable, false),
        (0, 0, false),
    ] {
        let mut s = simulator(CardKind::SdHighCapacity, 32);
        selected(&mut s);
        let memory = attach(&mut s, control, bus_mode);
        put_descriptor(&memory, 0, ONE_BLOCK, 512, 0x2000, 0);
        start_read(&mut s, 512, 0);
        run(&mut s);
        let written = memory.bus().device_written;
        assert_eq!(
            written,
            if walks { 512 } else { 0 },
            "{control:#x} {bus_mode:#x}"
        );
        if walks {
            assert_eq!(memory_words(&memory, 0x2000, 128), block_words(&s, 0));
        }
    }
}

/// An idle simulator in the transfer state, its IDMAC on, and the read of one block started.
fn reading_by_internal_direct_memory_access(
    configure: impl FnOnce(&mut SimulatedHostController),
    word_0: u32,
) -> (SimulatedHostController, Memory) {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    selected(&mut s);
    configure(&mut s);
    let memory = attach(
        &mut s,
        INTERNAL_DIRECT_MEMORY_ACCESS_ON,
        BUS_MODE_INTERNAL_DIRECT_MEMORY_ACCESS_ENABLE,
    );
    put_descriptor(&memory, 0, word_0, 512, 0x2000, 0);
    start_read(&mut s, 512, 0);
    run(&mut s);
    (s, memory)
}

fn status_of_internal_direct_memory_access(s: &SimulatedHostController) -> u32 {
    s.peek(INTERNAL_DIRECT_MEMORY_ACCESS_STATUS_REGISTER)
}

/// A descriptor the CPU still owns suspends the walk (`DU`) before a word moves. Only a poll
/// demand resumes it: other writes to `BUS_MODE` and `CONTROL` that neither reset the IDMAC nor
/// newly enable it leave it suspended. The finished descriptor goes back to the CPU and raises
/// `RI` with `NIS`.
#[test]
fn a_descriptor_the_controller_does_not_own_suspends_the_walk_until_a_poll_demand() {
    let (mut s, memory) =
        reading_by_internal_direct_memory_access(|_| {}, DESCRIPTOR_FIRST | DESCRIPTOR_LAST);
    let unavailable = INTERNAL_DIRECT_MEMORY_ACCESS_STATUS_DESCRIPTOR_UNAVAILABLE
        | INTERNAL_DIRECT_MEMORY_ACCESS_STATUS_ABNORMAL_INTERRUPT_SUMMARY;
    assert_eq!(status_of_internal_direct_memory_access(&s), unavailable);
    assert_eq!(memory.bus().device_written, 0);
    put_descriptor(&memory, 0, ONE_BLOCK, 512, 0x2000, 0);
    s.write(
        BUS_MODE_REGISTER,
        BUS_MODE_INTERNAL_DIRECT_MEMORY_ACCESS_ENABLE | BUS_MODE_FIXED_BURST,
    );
    s.write(
        CONTROL_REGISTER,
        INTERNAL_DIRECT_MEMORY_ACCESS_ON | CONTROL_INTERRUPT_ENABLE,
    );
    run(&mut s);
    assert_eq!(memory.bus().device_written, 0);
    s.write(POLL_DEMAND_REGISTER, 1);
    run(&mut s);
    assert_eq!(memory_words(&memory, 0x2000, 128), block_words(&s, 0));
    assert_eq!(
        memory_words(&memory, 0, 1),
        [ONE_BLOCK & !DESCRIPTOR_OWNED_BY_CONTROLLER]
    );
    let received = INTERNAL_DIRECT_MEMORY_ACCESS_STATUS_RECEIVE_INTERRUPT
        | INTERNAL_DIRECT_MEMORY_ACCESS_STATUS_NORMAL_INTERRUPT_SUMMARY;
    assert_eq!(
        status_of_internal_direct_memory_access(&s),
        unavailable | received
    );
}

/// The faults a test places on a descriptor strike before its first word moves: one read as the
/// CPU's suspends the walk, and a bus error halts it for good.
#[test]
fn a_descriptor_marked_unowned_or_refused_stops_the_walk_before_a_word_moves() {
    let (_, memory) =
        reading_by_internal_direct_memory_access(|s| s.unowned_at = Some(0), ONE_BLOCK);
    assert_eq!(memory.bus().device_written, 0);
    let (mut s, memory) =
        reading_by_internal_direct_memory_access(|s| s.bus_error_at = Some(0), ONE_BLOCK);
    let fatal = INTERNAL_DIRECT_MEMORY_ACCESS_STATUS_FATAL_BUS_ERROR
        | INTERNAL_DIRECT_MEMORY_ACCESS_STATUS_ABNORMAL_INTERRUPT_SUMMARY;
    assert_eq!(status_of_internal_direct_memory_access(&s), fatal);
    s.write(POLL_DEMAND_REGISTER, 1);
    run(&mut s);
    assert_eq!(memory.bus().device_written, 0);
}

/// A descriptor with `DIC` closes without an interrupt, and a corrupting bus flips bit 0 of every
/// word, whatever the bit was.
#[test]
fn a_descriptor_closes_silently_with_interrupts_disabled_and_a_corrupt_bus_flips_bit_0() {
    let (s, memory) = reading_by_internal_direct_memory_access(
        |s| {
            s.direct_memory_access_corrupts = true;
            // Word `k` is four bytes of `k`, so bit 0 is set in every other word.
            s.storage[0] = core::array::from_fn(|i| (i / 4) as u8);
        },
        ONE_BLOCK | DESCRIPTOR_DISABLE_INTERRUPT_ON_COMPLETION,
    );
    assert_eq!(status_of_internal_direct_memory_access(&s), 0);
    let flipped: Vec<u32> = block_words(&s, 0).iter().map(|w| w ^ 1).collect();
    assert_eq!(memory_words(&memory, 0x2000, 128), flipped);
}

/// Without `CH` the next descriptor is the one 16 bytes on (a ring); with it, the one the
/// descriptor names (a chain), whatever sits 16 bytes on.
#[test]
fn a_ring_walks_to_the_adjacent_descriptor_and_a_chain_follows_its_pointer() {
    let first = DESCRIPTOR_OWNED_BY_CONTROLLER | DESCRIPTOR_FIRST;
    let last = DESCRIPTOR_OWNED_BY_CONTROLLER | DESCRIPTOR_LAST;
    for chained in [false, true] {
        let mut s = simulator(CardKind::SdHighCapacity, 32);
        selected(&mut s);
        let memory = attach(
            &mut s,
            INTERNAL_DIRECT_MEMORY_ACCESS_ON,
            BUS_MODE_INTERNAL_DIRECT_MEMORY_ACCESS_ENABLE,
        );
        if chained {
            put_descriptor(&memory, 0, first | DESCRIPTOR_CHAINED, 512, 0x2000, 0x40);
            put_descriptor(&memory, 0x40, last, 512, 0x2200, 0);
        } else {
            put_descriptor(&memory, 0, first, 512, 0x2000, 0x40);
            put_descriptor(&memory, 0x10, last, 512, 0x2200, 0);
        }
        start_read(&mut s, 1024, 0);
        run(&mut s);
        let mut want = block_words(&s, 0);
        want.extend(block_words(&s, 1));
        assert_eq!(
            memory_words(&memory, 0x2000, 256),
            want,
            "chained {chained}"
        );
    }
}

/// An IDMAC write never fills the FIFO past its depth.
#[test]
fn the_internal_direct_memory_access_engine_fills_the_transmit_queue_only_to_its_depth() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    selected(&mut s);
    let memory = attach(
        &mut s,
        INTERNAL_DIRECT_MEMORY_ACCESS_ON,
        BUS_MODE_INTERNAL_DIRECT_MEMORY_ACCESS_ENABLE,
    );
    put_descriptor(&memory, 0, ONE_BLOCK, 1024, 0x2000, 0);
    let data: Vec<u8> = (0..1024).map(|i| (i % 253) as u8).collect();
    memory.bus().bytes[0x2000..0x2400].copy_from_slice(&data);
    s.write(BLOCK_SIZE_BYTES_REGISTER, 512);
    s.write(BYTE_COUNT_REGISTER, 1024);
    command(&mut s, 25, 0, COMMAND_DATA_EXPECTED | COMMAND_WRITE);
    for _ in 0..16 {
        assert!(queue_count(s.read(STATUS_REGISTER)) <= 32);
    }
    assert_eq!(s.storage[..2].concat(), data);
}

/// A finished walk starts again from `DBADDR` after a write to it, a soft reset, an IDMAC reset,
/// or `BUS_MODE` newly enabling the IDMAC.
#[test]
fn a_finished_walk_starts_again_after_a_new_base_a_reset_or_a_fresh_enable() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    selected(&mut s);
    let enable = BUS_MODE_INTERNAL_DIRECT_MEMORY_ACCESS_ENABLE;
    let memory = attach(&mut s, INTERNAL_DIRECT_MEMORY_ACCESS_ON, enable);
    let restarts: [&dyn Fn(&mut SimulatedHostController); 4] = [
        &|s| s.write(DESCRIPTOR_LIST_BASE_PHYSICAL_ADDRESS_REGISTER, BASE),
        &|s| s.write(BUS_MODE_REGISTER, enable | BUS_MODE_SOFTWARE_RESET),
        &|s| {
            s.write(
                CONTROL_REGISTER,
                INTERNAL_DIRECT_MEMORY_ACCESS_ON | CONTROL_DIRECT_MEMORY_ACCESS_RESET,
            );
        },
        &|s| {
            s.write(BUS_MODE_REGISTER, 0);
            s.write(BUS_MODE_REGISTER, enable);
        },
    ];
    let read_block = |s: &mut SimulatedHostController, n: usize| {
        put_descriptor(&memory, 0, ONE_BLOCK, 512, 0x2000, 0);
        s.write(BLOCK_SIZE_BYTES_REGISTER, 512);
        s.write(BYTE_COUNT_REGISTER, 512);
        command(s, 17, n as u32, COMMAND_DATA_EXPECTED);
        run(s);
        assert_eq!(
            memory_words(&memory, 0x2000, 128),
            block_words(s, n),
            "block {n}"
        );
    };
    read_block(&mut s, 0);
    for (n, restart) in restarts.iter().enumerate() {
        restart(&mut s);
        read_block(&mut s, n + 1);
    }
}

/// A reset budget counts resets: reading `BUS_MODE` with no soft reset in it, or writing
/// `CONTROL` without a reset bit, spends none of it.
#[test]
fn a_reset_budget_counts_resets_not_readings_or_other_writes() {
    let mut s = SimulatedHostController::new(CardKind::SdHighCapacity, 8, 32);
    s.bus_mode_resets_cleared = Some(1);
    s.control_resets_cleared = Some(1);
    for _ in 0..3 {
        s.read(BUS_MODE_REGISTER);
        s.write(CONTROL_REGISTER, CONTROL_INTERRUPT_ENABLE);
    }
    for clears in [true, false] {
        s.write(BUS_MODE_REGISTER, BUS_MODE_SOFTWARE_RESET);
        s.write(CONTROL_REGISTER, CONTROL_CONTROLLER_RESET);
        for _ in 0..2 {
            s.read(BUS_MODE_REGISTER);
            s.read(CONTROL_REGISTER);
        }
        assert_eq!(
            s.peek(BUS_MODE_REGISTER) & BUS_MODE_SOFTWARE_RESET == 0,
            clears
        );
        assert_eq!(
            s.peek(CONTROL_REGISTER) & CONTROL_CONTROLLER_RESET == 0,
            clears
        );
    }
}

/// An IDMAC reset leaves the FIFO alone, a FIFO reset empties it and the transfer refills it, and
/// only a controller reset drops the transfer.
#[test]
fn a_queue_reset_empties_the_queue_and_only_a_controller_reset_drops_the_transfer() {
    let mut s = simulator(CardKind::SdHighCapacity, 32);
    selected(&mut s);
    start_read(&mut s, 1024, 0);
    let mut want = block_words(&s, 0);
    want.extend(block_words(&s, 1));
    s.write(CONTROL_REGISTER, CONTROL_DIRECT_MEMORY_ACCESS_RESET);
    assert_eq!(drain(&mut s, 8), want[..8]);
    s.write(CONTROL_REGISTER, CONTROL_QUEUE_RESET);
    assert_eq!(queue_count(s.read(STATUS_REGISTER)), 32);
    assert_eq!(drain(&mut s, 1), want[40..41]);
    s.write(
        CONTROL_REGISTER,
        CONTROL_CONTROLLER_RESET | CONTROL_QUEUE_RESET,
    );
    run(&mut s);
    assert_eq!(queue_count(s.read(STATUS_REGISTER)), 0);
    assert_eq!(
        s.read(RAW_INTERRUPT_STATUS_REGISTER) & RAW_INTERRUPT_DATA_TRANSFER_OVER,
        0
    );
}
