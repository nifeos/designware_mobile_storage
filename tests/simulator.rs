//! The `simulator` feature as a caller uses it: through the crate's public interface only, from
//! outside the crate. `first_sector` stands for the caller's own code, the function the README's
//! first example shows; the tests drive it and the driver against the exported simulator.

use designware_mobile_storage::host_controller::{HostController, RegisterWindowAndClock};
use designware_mobile_storage::internal_direct_memory_access::{
    InternalDirectMemoryAccess, PAGE_BYTES,
};
use designware_mobile_storage::sd::{self, BLOCK_BYTES, BlockTransferError, CardKind};
use designware_mobile_storage::{SimulatedDirectMemoryAccessRegion, SimulatedHostController};

fn first_sector<R: RegisterWindowAndClock>(registers: R) -> Option<[u8; 512]> {
    let (mut host, _id) = HostController::new(registers, 50_000_000, Some(32)).ok()?;
    let card = sd::identify(&mut host, 4).ok()?; // at most a 4-bit bus
    let mut buffer = [0u8; 512];
    sd::read_blocks(&mut host, &card, 0, &mut buffer).ok()?;
    Some(buffer)
}

#[test]
fn first_sector_reads_what_the_card_holds() {
    let mut simulator = SimulatedHostController::new(CardKind::SdHighCapacity, 8192);
    simulator.card_blocks_mut()[0][510..].copy_from_slice(&[0x55, 0xaa]);
    let sector = first_sector(simulator).expect("the simulated card reads");
    assert_eq!(sector[510..], [0x55, 0xaa]);
}

#[test]
fn first_sector_finds_no_card_in_an_empty_slot() {
    assert_eq!(first_sector(SimulatedHostController::empty_slot()), None);
}

#[test]
fn every_card_kind_is_identified_and_written_and_read_back() {
    for kind in [
        CardKind::SdVersion1,
        CardKind::SdVersion2,
        CardKind::SdHighCapacity,
        CardKind::MultimediaCard,
    ] {
        let simulator = SimulatedHostController::new(kind, 4096);
        let (mut host, _) = HostController::new(simulator, 50_000_000, None).unwrap();
        let card = sd::identify(&mut host, 4).unwrap();
        assert_eq!(card.kind, kind);
        assert_eq!(card.blocks, 4096);
        let data: Vec<u8> = (0..3 * BLOCK_BYTES).map(|i| (i % 251) as u8).collect();
        sd::write_blocks(&mut host, &card, 9, &data).unwrap();
        let mut back = vec![0u8; data.len()];
        sd::read_blocks(&mut host, &card, 9, &mut back).unwrap();
        assert_eq!(back, data);
        let simulator = host.into_registers();
        assert_eq!(
            simulator.card_blocks()[10][..],
            data[BLOCK_BYTES..2 * BLOCK_BYTES]
        );
    }
}

#[test]
fn a_failed_block_read_is_reported_and_the_next_one_succeeds() {
    let mut simulator = SimulatedHostController::new(CardKind::SdHighCapacity, 8192);
    simulator.card_blocks_mut()[3] = [0x3c; BLOCK_BYTES];
    let (mut host, _) = HostController::new(simulator, 50_000_000, None).unwrap();
    let card = sd::identify(&mut host, 4).unwrap();
    host.registers_mut().fail_next_block_read();
    let mut block = [0u8; BLOCK_BYTES];
    assert!(matches!(
        sd::read_blocks(&mut host, &card, 3, &mut block),
        Err(BlockTransferError::Host(_))
    ));
    sd::read_blocks(&mut host, &card, 3, &mut block).unwrap();
    assert_eq!(block, [0x3c; BLOCK_BYTES]);
}

#[test]
fn blocks_move_through_the_internal_direct_memory_access_controller() {
    let mut simulator = SimulatedHostController::new(CardKind::SdHighCapacity, 8192);
    let region: SimulatedDirectMemoryAccessRegion =
        simulator.attach_direct_memory_access_region(u64::from(PAGE_BYTES) * 16, 4);
    let (host, id) = HostController::new(simulator, 50_000_000, None).unwrap();
    let mut host = host.with_internal_direct_memory_access(
        InternalDirectMemoryAccess::new(region, &id).expect("the region fits the controller"),
    );
    let card = sd::identify(&mut host, 4).unwrap();
    let data: Vec<u8> = (0..2 * BLOCK_BYTES).map(|i| (i % 241) as u8).collect();
    sd::write_blocks(&mut host, &card, 100, &data).unwrap();
    let mut back = vec![0u8; data.len()];
    sd::read_blocks(&mut host, &card, 100, &mut back).unwrap();
    assert_eq!(back, data);
    assert_eq!(
        host.into_registers().card_blocks()[101][..],
        data[BLOCK_BYTES..]
    );
}

/// The README's example is the test above that drives `first_sector`, word for word, so the
/// example a stranger copies is one that runs.
#[test]
fn the_readme_example_is_a_test_in_this_file() {
    let this = include_str!("simulator.rs");
    let start = this
        .find("#[test]\nfn first_sector_reads_what_the_card_holds")
        .unwrap();
    let end = start + this[start..].find("\n}\n").unwrap() + "\n}\n".len();
    assert!(include_str!("../README.md").contains(&this[start..end]));
}

#[test]
#[should_panic(expected = "a nonzero multiple of 1024 blocks")]
fn a_card_size_its_registers_cannot_describe_is_refused() {
    let _ = SimulatedHostController::new(CardKind::SdHighCapacity, 1000);
}

#[test]
fn a_standard_capacity_card_may_be_larger_than_a_few_megabytes() {
    // 8192 blocks is 4 MiB, well inside the one gibibyte a standard-capacity CSD can describe.
    let simulator = SimulatedHostController::new(CardKind::SdVersion2, 8192);
    let (mut host, _) = HostController::new(simulator, 50_000_000, None).unwrap();
    assert_eq!(sd::identify(&mut host, 4).unwrap().blocks, 8192);
}

#[test]
fn the_debug_form_names_the_card_size() {
    let simulator = SimulatedHostController::new(CardKind::SdHighCapacity, 8192);
    assert_eq!(
        format!("{simulator:?}"),
        "SimulatedHostController { card_blocks: 8192, .. }"
    );
}

#[test]
fn the_clock_advances_one_microsecond_a_reading() {
    let mut simulator = SimulatedHostController::new(CardKind::SdHighCapacity, 8192);
    let first = simulator.now_microseconds();
    assert_eq!(simulator.now_microseconds(), first + 1);
    assert_eq!(simulator.now_microseconds(), first + 2);
}

#[test]
fn the_region_reads_back_the_words_written_to_it() {
    use designware_mobile_storage::internal_direct_memory_access::DirectMemoryAccessRegion;
    let mut simulator = SimulatedHostController::new(CardKind::SdHighCapacity, 8192);
    let mut region = simulator.attach_direct_memory_access_region(u64::from(PAGE_BYTES) * 16, 1);
    region.write_u32(8, 0x1234_5678);
    region.write_u32(12, 0x9abc_def0);
    assert_eq!(
        (region.read_u32(8), region.read_u32(12)),
        (0x1234_5678, 0x9abc_def0)
    );
}
