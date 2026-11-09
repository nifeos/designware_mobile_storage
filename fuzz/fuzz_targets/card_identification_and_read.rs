//! Fuzz card identification and one block read against a controller whose every register read
//! comes from the fuzz input.
//!
//! The first twelve bytes choose the bus width the slot allows, how many blocks to read, whether
//! the block address is wrapped into the card's capacity, and the address.
//! Every later register read takes a tag byte and then, by the tag, no more bytes (zero or all
//! ones), one byte or a whole word, so the input can say "the bit cleared" cheaply and still give
//! any word. Once the input runs out, reads return zero. Register writes are ignored. The clock
//! advances on every call, as the trait requires, so every wait ends at its deadline.
//!
//! `identify` and then `read_blocks` must return, with an error or a card, and never panic. A card
//! that identified must use a bus width the slot allowed.

#![no_main]

use designware_mobile_storage::host_controller::{HostController, RegisterWindowAndClock};
use designware_mobile_storage::sd::{self, BLOCK_BYTES};
use libfuzzer_sys::fuzz_target;

const HEADER_BYTES: usize = 12;
const BASE_HERTZ: u32 = 50_000_000;
const QUEUE_DEPTH_WORDS: u32 = 32;
const MAXIMUM_BLOCKS_PER_READ: usize = 4;
const CLOCK_STEP_MICROSECONDS: u64 = 1_000;

struct ByteDrivenWindow<'a> {
    input: &'a [u8],
    now: u64,
}

impl ByteDrivenWindow<'_> {
    fn next_byte(&mut self) -> u8 {
        let Some((&byte, rest)) = self.input.split_first() else {
            return 0;
        };
        self.input = rest;
        byte
    }
}

impl RegisterWindowAndClock for ByteDrivenWindow<'_> {
    fn read(&mut self, _offset: u32) -> u32 {
        match self.next_byte() & 3 {
            0 => 0,
            1 => u32::from_le_bytes([
                self.next_byte(),
                self.next_byte(),
                self.next_byte(),
                self.next_byte(),
            ]),
            2 => u32::from(self.next_byte()),
            _ => u32::MAX,
        }
    }

    fn write(&mut self, _offset: u32, _value: u32) {}

    fn now_microseconds(&mut self) -> u64 {
        self.now = self.now.wrapping_add(CLOCK_STEP_MICROSECONDS);
        self.now
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((header, rest)) = data.split_at_checked(HEADER_BYTES) else {
        return;
    };
    let maximum_bus_width = [1u8, 4, 8, header[0]][usize::from(header[1] & 3)];
    let blocks_to_read = 1 + usize::from(header[2]) % MAXIMUM_BLOCKS_PER_READ;
    let wrap_into_card = header[3] & 1 != 0;
    let mut address_bytes = [0u8; 8];
    address_bytes.copy_from_slice(&header[4..12]);
    let address = u64::from_le_bytes(address_bytes);

    let window = ByteDrivenWindow {
        input: rest,
        now: 0,
    };
    let Ok((mut host, _identity)) =
        HostController::new(window, BASE_HERTZ, Some(QUEUE_DEPTH_WORDS))
    else {
        return;
    };
    let Ok(card) = sd::identify(&mut host, maximum_bus_width) else {
        return;
    };
    assert!(card.bus_width <= maximum_bus_width.max(1));

    let logical_block_address = if wrap_into_card {
        address % card.blocks.max(1)
    } else {
        address
    };
    let mut buffer = [0u8; MAXIMUM_BLOCKS_PER_READ * BLOCK_BYTES];
    let _ = sd::read_blocks(
        &mut host,
        &card,
        logical_block_address,
        &mut buffer[..blocks_to_read * BLOCK_BYTES],
    );
});
