//! The part of the simulated controller and card that a caller's own tests may use, behind the
//! `simulator` feature. The simulation itself stays private to the crate, with every knob the
//! crate's own tests turn; this file wraps it and makes public only what a program that drives
//! this crate needs to test its own code: a card of a given kind and size, its blocks, one fault,
//! and the memory the internal direct memory access controller (IDMAC) reaches.

use core::fmt;

use crate::host_controller::RegisterWindowAndClock;
use crate::internal_direct_memory_access::DirectMemoryAccessRegion;
use crate::sd::{BLOCK_BYTES, CardKind};
use crate::simulator;

/// The FIFO depth the simulated controller has, in 32-bit words: the JH7110's.
const QUEUE_DEPTH_WORDS: usize = 32;

/// The largest standard-capacity SD card the simulated card-specific data (CSD) register can
/// describe, in blocks: 4096 units of 512 blocks, one gibibyte.
const STANDARD_CAPACITY_MAXIMUM_BLOCKS: usize = 4096 * 512;

/// **A simulated controller with a card in its slot**, for a caller's host tests. Hand it to
/// [`HostController::new`](crate::host_controller::HostController::new) in place of a register
/// window, then run the code under test against it.
///
/// Available with the `simulator` feature, which needs `std`. It models the contract the driver
/// relies on, as the controller's databook describes it, and nothing more: it is not a model of
/// silicon timing. Its clock advances one microsecond each time the driver reads it, so a sequence
/// that would hang on a board ends in the driver's own timeout error here. Its FIFO is 32 words
/// deep, as on the JH7110; pass `Some(32)` or `None` for the depth, and any base clock.
///
/// ```
/// use designware_mobile_storage::SimulatedHostController;
/// use designware_mobile_storage::host_controller::HostController;
/// use designware_mobile_storage::sd::{self, BLOCK_BYTES, CardKind};
///
/// let mut simulator = SimulatedHostController::new(CardKind::SdHighCapacity, 8192);
/// simulator.card_blocks_mut()[7] = [0x5a; BLOCK_BYTES];
/// let (mut host, _) = HostController::new(simulator, 50_000_000, None).unwrap();
/// let card = sd::identify(&mut host, 4).unwrap();
/// let mut block = [0u8; BLOCK_BYTES];
/// sd::read_blocks(&mut host, &card, 7, &mut block).unwrap();
/// assert_eq!(block, [0x5a; BLOCK_BYTES]);
/// ```
pub struct SimulatedHostController(simulator::SimulatedHostController);

impl SimulatedHostController {
    /// **A controller with a card of `kind` holding `blocks` blocks of zero.** An SD card answers
    /// as the SD specification describes, an eMMC as the eMMC standard does, in sector mode.
    ///
    /// # Panics
    ///
    /// When `blocks` is not a nonzero multiple of 1024, or is above 2,097,152 (one gibibyte) for a
    /// standard-capacity SD card, since the card's registers could not describe that size.
    #[must_use]
    pub fn new(kind: CardKind, blocks: usize) -> SimulatedHostController {
        assert!(
            blocks > 0 && blocks.is_multiple_of(1024),
            "a simulated card holds a nonzero multiple of 1024 blocks, not {blocks}"
        );
        let simulated_kind = match kind {
            CardKind::SdVersion1 => simulator::CardKind::SdVersion1,
            CardKind::SdVersion2 => simulator::CardKind::SdVersion2,
            CardKind::SdHighCapacity => simulator::CardKind::SdHighCapacity,
            CardKind::MultimediaCard => simulator::CardKind::MultimediaCard,
        };
        if matches!(kind, CardKind::SdVersion1 | CardKind::SdVersion2) {
            assert!(
                blocks <= STANDARD_CAPACITY_MAXIMUM_BLOCKS,
                "a simulated standard-capacity SD card holds at most \
                 {STANDARD_CAPACITY_MAXIMUM_BLOCKS} blocks, not {blocks}"
            );
        }
        SimulatedHostController(simulator::SimulatedHostController::new(
            simulated_kind,
            blocks,
            QUEUE_DEPTH_WORDS,
        ))
    }

    /// **A controller with nothing in its slot**: its card-detect register says so, and no command
    /// is answered.
    #[must_use]
    pub fn empty_slot() -> SimulatedHostController {
        SimulatedHostController(simulator::SimulatedHostController::new(
            simulator::CardKind::Empty,
            0,
            QUEUE_DEPTH_WORDS,
        ))
    }

    /// The card's blocks, as the driver has left them. Empty for an empty slot.
    #[must_use]
    pub fn card_blocks(&self) -> &[[u8; BLOCK_BYTES]] {
        &self.0.storage
    }

    /// The card's blocks, to fill before a test or change between its steps.
    #[must_use]
    pub fn card_blocks_mut(&mut self) -> &mut [[u8; BLOCK_BYTES]] {
        &mut self.0.storage
    }

    /// **Fail the next block read**: its data phase ends with a data cyclic redundancy check (CRC)
    /// error, which the driver reports as
    /// [`BlockTransferError::Host`](crate::sd::BlockTransferError::Host). Later reads succeed.
    pub fn fail_next_block_read(&mut self) {
        self.0.fail_next_read_crc = true;
    }

    /// **Give the controller's IDMAC `pages` pages of memory** at `direct_memory_access_bus_address`,
    /// zero, and return the region the driver writes it through. Pass the region to
    /// [`InternalDirectMemoryAccess::new`](crate::internal_direct_memory_access::InternalDirectMemoryAccess::new).
    /// A later call replaces the memory the controller reaches.
    #[must_use]
    pub fn attach_direct_memory_access_region(
        &mut self,
        direct_memory_access_bus_address: u64,
        pages: usize,
    ) -> SimulatedDirectMemoryAccessRegion {
        let memory = simulator::Memory::new(direct_memory_access_bus_address, pages);
        self.0.memory = Some(memory.clone());
        SimulatedDirectMemoryAccessRegion(memory)
    }
}

impl fmt::Debug for SimulatedHostController {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimulatedHostController")
            .field("card_blocks", &self.0.storage.len())
            .finish_non_exhaustive()
    }
}

impl RegisterWindowAndClock for SimulatedHostController {
    fn read(&mut self, offset: u32) -> u32 {
        self.0.read(offset)
    }
    fn write(&mut self, offset: u32, value: u32) {
        self.0.write(offset, value);
    }
    fn now_microseconds(&mut self) -> u64 {
        self.0.now_microseconds()
    }
}

/// **Memory the simulated IDMAC reaches**, as the driver sees it. Made by
/// [`SimulatedHostController::attach_direct_memory_access_region`]; the controller holds the other
/// handle on the same bytes.
#[derive(Debug)]
pub struct SimulatedDirectMemoryAccessRegion(simulator::Memory);

impl DirectMemoryAccessRegion for SimulatedDirectMemoryAccessRegion {
    fn direct_memory_access_bus_address(&self) -> u64 {
        self.0.direct_memory_access_bus_address()
    }
    fn bytes(&self) -> u32 {
        self.0.bytes()
    }
    fn read_u32(&self, offset: u32) -> u32 {
        self.0.read_u32(offset)
    }
    fn write_u32(&mut self, offset: u32, value: u32) {
        self.0.write_u32(offset, value);
    }
    fn read_bytes(&self, offset: u32, out: &mut [u8]) {
        self.0.read_bytes(offset, out);
    }
    fn write_bytes(&mut self, offset: u32, source: &[u8]) {
        self.0.write_bytes(offset, source);
    }
    fn barrier(&mut self) {
        self.0.barrier();
    }
}
