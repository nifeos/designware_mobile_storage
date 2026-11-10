# Bench steps: an eMMC on a wider bus and at high speed

`sd::switch_multimedia_card_bus_width` and `sd::switch_multimedia_card_to_high_speed` have run only
against the simulated card in the host tests. These steps confirm them on a board with an eMMC
module fitted. Until every step passes, the README's BUGS section keeps its 1-bit, 25 megahertz
statement for every eMMC.

Keep unit-specific details (serial numbers, addresses, where the board sits) out of this
repository. A transcript filed here names the module only by the manufacturer and product name
in its card identification (CID) register.

## Before the session

- A `VisionFive` 2 or another JH7110 board, booting from its microSD card as usual.
- An eMMC module in the eMMC socket: slot 0, whose tree node has `bus-width = <8>`.
- A program that drives this crate on the board and prints each step's result. Print an error
  with `{}`, so the transcript carries the step that failed.

## Steps

1. **Bring the card up.** `jh7110::Slot::from_node` for slot 0, then `HostController::new` and
   `sd::identify(&mut host, slot.bus_width)`. Record `card.kind`, `card.blocks`,
   `card.bus_width` (1) and `card.clock_hertz` (25 megahertz). Check `card.blocks` times 512
   against the module's stated capacity.
2. **Read at 1 bit.** Read block 0, the last block (`card.blocks - 1`) and one scratch block: a
   block `MasterBootRecord::scratch_sector_range` names, or any block on a module that holds no
   data. Keep a copy of each.
3. **Switch to 4 bits.** `sd::switch_multimedia_card_bus_width(&mut host, &mut card, 4)` must
   return `Ok(4)`. Read the same blocks again and compare them with the 1-bit copies. Write a
   pattern to one scratch block, read it back, then write the saved copy back.
4. **Switch to 8 bits.** `sd::identify` again (it returns the card to 1 bit), then
   `sd::switch_multimedia_card_bus_width(&mut host, &mut card, 8)` must return `Ok(8)`. Repeat
   step 3's reads, comparison, write and restore.
5. **Switch to high speed.** With the card at 8 bits,
   `sd::switch_multimedia_card_to_high_speed(&mut host, &mut card)` returns the clock: 50
   megahertz from the 50 megahertz base for a card whose `EXT_CSD` card type offers 52
   megahertz. Repeat step 3's reads, comparison, write and restore.
6. **Capacity at the last width.** `card.blocks` is the same as in step 1. A read of the last
   block succeeds, and `read_blocks` refuses block `card.blocks` with `BlockTransferError::Range`
   before it sends anything.

## Recording the result

For each step, record whether it passed and, on a failure, the error as `{}` prints it. A failure
goes to an issue with the transcript. When every step passes, the README's BUGS section can say
that an eMMC has run at that width and speed, naming the module by manufacturer and product.
