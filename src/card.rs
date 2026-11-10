//! **What a card says about itself**: the OCR, the card identification data (CID), the CSD, the R1 status word and the eMMC
//! `EXT_CSD`, decoded. Bit positions are the SD Physical Layer Simplified Specification's (version
//! 9.10, sections 5.1 to 5.3 and 4.10.1) and `JEDEC JESD84-B51`'s for eMMC, in the spec's own
//! numbering, which is also Linux's `unstuff_bits` convention (`drivers/mmc/core/mmc_ops.h`): a long response is held as four
//! words `[RESP0, RESP1, RESP2, RESP3]`, bit 0 of `RESP0` being the register's bit 0 (the always-one
//! end bit, with the 7-bit cyclic redundancy check (CRC) in bits 7:1).
//!
//! Nothing here can fail on hostile input. A card is a device on the far side of a bus and its
//! registers are data, so every decoder is total and every capacity is computed without overflow.

/// The operation conditions register's (OCR) power-up-complete bit: clear while the card is still
/// busy initializing.
#[doc(alias = "ocr")]
pub(crate) const OPERATION_CONDITIONS_BUSY_DONE: u32 = 1 << 31;
/// The OCR's card capacity status (SD) or access mode (on eMMC, bits 30:29 = `0b10`): set on a
/// high-capacity card, which is addressed in 512-byte blocks rather than bytes.
#[doc(alias = "ocr")]
pub(crate) const OPERATION_CONDITIONS_HIGH_CAPACITY: u32 = 1 << 30;
/// The voltage window this host offers, 3.2 to 3.4 V: OpenBSD's `dwmmc_host_ocr`. The JH7110's
/// card slot is wired for 3.3 V and nothing here switches it.
#[doc(alias = "ocr")]
pub(crate) const OPERATION_CONDITIONS_VOLTAGE_WINDOW: u32 = (1 << 20) | (1 << 21);
/// `ACMD41`'s argument: high capacity supported, and the window.
#[doc(alias = "ocr")]
pub(crate) const SD_OPERATION_CONDITIONS_ARGUMENT: u32 =
    OPERATION_CONDITIONS_HIGH_CAPACITY | OPERATION_CONDITIONS_VOLTAGE_WINDOW;
/// eMMC `CMD1`'s argument: sector access mode (bits 30:29 = `0b10`) and 2.7 to 3.6 V plus the
/// 1.70 to 1.95 V bit, the value Linux and U-Boot both send.
#[doc(alias = "ocr")]
pub(crate) const MULTIMEDIA_CARD_OPERATION_CONDITIONS_ARGUMENT: u32 = 0x40ff_8080;
/// `CMD8` (`SEND_IF_COND`)'s interface condition argument: 2.7 to 3.6 V (`0x1`) and the check
/// pattern `0xaa`. A card that understands it echoes both.
#[doc(alias = "if_cond")]
pub(crate) const INTERFACE_CONDITION_ARGUMENT: u32 = 0x1aa;

/// **Card status errors** (SD section 4.10.1): the R1 bits that report a failure, Linux's error
/// mask: out of range, address, block length, erase sequence and parameter, write-protect
/// violation, lock failure, CRC, illegal command, error correction code (ECC), controller error, unknown error, CSD
/// overwrite, write-protect erase skip, and the authentication sequence error. `CARD_IS_LOCKED`
/// (bit 25) is a state, not an error, and is not in the mask.
#[doc(alias = "card status errors")]
pub const R1_ERROR_MASK: u32 = 0xfdf9_8008;
/// **Ready for data, card status bit 8** (Linux's `R1_READY_FOR_DATA`, SD section 4.10.1): the R1
/// bit saying the card is ready for data.
#[doc(alias = "ready for data")]
pub const R1_READY_FOR_DATA: u32 = 1 << 8;

/// The R1 word's current state, bits 12:9: 4 is transfer, 5 sending data, 6 receive data, 7
/// programming.
#[must_use]
pub const fn r1_state(r1: u32) -> u32 {
    (r1 >> 9) & 0xf
}

/// The transfer state, the one a selected card idles in.
pub const R1_STATE_TRANSFER: u32 = 4;

/// **Switch error, card status bit 7** (FreeBSD's `R1_SWITCH_ERROR`, `JESD84-B51` section 6.13):
/// set in the status after a `CMD6` the card would not carry out. It is not in [`R1_ERROR_MASK`],
/// because only a status read after a switch reports it.
#[doc(alias = "SWITCH_ERROR")]
pub(crate) const R1_SWITCH_ERROR: u32 = 1 << 7;

/// `CMD6`'s access field for writing one `EXT_CSD` byte, bits 25:24 = `0b11` (OpenBSD's
/// `MMC_SWITCH_MODE_WRITE_BYTE`, FreeBSD's `MMC_SWITCH_FUNC_WR`).
#[doc(alias = "MMC_SWITCH_MODE_WRITE_BYTE")]
pub(crate) const SWITCH_WRITE_BYTE: u32 = 3 << 24;
/// `CMD6`'s command set field, bits 2:0: the normal set, as both BSDs send it
/// (`EXT_CSD_CMD_SET_NORMAL`). A byte write ignores it.
#[doc(alias = "EXT_CSD_CMD_SET_NORMAL")]
pub(crate) const SWITCH_COMMAND_SET_NORMAL: u32 = 1;

/// **`CMD6`'s argument that writes `value` to `EXT_CSD` byte `byte`**: the access field, the byte
/// in bits 23:16, the value in bits 15:8 and the command set, the layout of OpenBSD's
/// `sdmmc_mem_mmc_switch` and FreeBSD's `mmc_switch`.
#[must_use]
pub(crate) const fn switch_argument(byte: u8, value: u8) -> u32 {
    SWITCH_WRITE_BYTE | (byte as u32) << 16 | (value as u32) << 8 | SWITCH_COMMAND_SET_NORMAL
}

/// `EXT_CSD`'s `BUS_WIDTH`, byte 183 (`JESD84-B51` section 7.4): the data bus width the card
/// drives. The standard makes it write-only, so a read returns no proof that a write took.
#[doc(alias = "BUS_WIDTH")]
#[doc(alias = "EXT_CSD_BUS_WIDTH")]
pub(crate) const EXTENDED_CARD_SPECIFIC_DATA_BUS_WIDTH_BYTE: u8 = 183;
/// `BUS_WIDTH`'s value for a 4-bit bus (`EXT_CSD_BUS_WIDTH_4` in both BSDs).
#[doc(alias = "EXT_CSD_BUS_WIDTH_4")]
pub(crate) const EXTENDED_CARD_SPECIFIC_DATA_BUS_WIDTH_4_BIT: u8 = 1;
/// `BUS_WIDTH`'s value for an 8-bit bus (`EXT_CSD_BUS_WIDTH_8` in both BSDs).
#[doc(alias = "EXT_CSD_BUS_WIDTH_8")]
pub(crate) const EXTENDED_CARD_SPECIFIC_DATA_BUS_WIDTH_8_BIT: u8 = 2;
/// `EXT_CSD`'s `HS_TIMING`, byte 185 (`JESD84-B51` section 7.4): the card's interface timing,
/// readable, so a read after the switch shows whether it took.
#[doc(alias = "HS_TIMING")]
#[doc(alias = "EXT_CSD_HS_TIMING")]
pub(crate) const EXTENDED_CARD_SPECIFIC_DATA_HIGH_SPEED_TIMING_BYTE: u8 = 185;
/// `HS_TIMING`'s value for high speed (`EXT_CSD_HS_TIMING_HS` in both BSDs).
#[doc(alias = "EXT_CSD_HS_TIMING_HS")]
pub(crate) const EXTENDED_CARD_SPECIFIC_DATA_HIGH_SPEED_TIMING_HIGH_SPEED: u8 = 1;
/// `EXT_CSD`'s card type, byte 196 (`EXT_CSD_CARD_TYPE` in both BSDs; `JESD84-B51` calls it
/// `DEVICE_TYPE`): the timings the card supports. Not the controller's card type register.
#[doc(alias = "EXT_CSD_CARD_TYPE")]
#[doc(alias = "DEVICE_TYPE")]
pub(crate) const EXTENDED_CARD_SPECIFIC_DATA_CARD_TYPE_BYTE: usize = 196;
/// The card type bit for high speed at up to 26 megahertz (FreeBSD's `EXT_CSD_CARD_TYPE_HS_26`).
#[doc(alias = "EXT_CSD_CARD_TYPE_HS_26")]
pub(crate) const EXTENDED_CARD_SPECIFIC_DATA_CARD_TYPE_HIGH_SPEED_26_MEGAHERTZ: u8 = 1 << 0;
/// The card type bit for high speed at up to 52 megahertz (FreeBSD's `EXT_CSD_CARD_TYPE_HS_52`).
#[doc(alias = "EXT_CSD_CARD_TYPE_HS_52")]
pub(crate) const EXTENDED_CARD_SPECIFIC_DATA_CARD_TYPE_HIGH_SPEED_52_MEGAHERTZ: u8 = 1 << 1;
/// **The read-only `EXT_CSD` bytes compared across a bus width change**: `EXT_CSD_REV` (192),
/// `CSD_STRUCTURE` (194), the card type (196), `SEC_COUNT` (212 to 215), `HC_WP_GRP_SIZE` (221)
/// and `HC_ERASE_GRP_SIZE` (224). A card that did not take the new width answers on fewer lines
/// than the controller listens on, so these come back changed or the read fails.
pub(crate) const EXTENDED_CARD_SPECIFIC_DATA_READ_ONLY_BYTES: [usize; 9] =
    [192, 194, 196, 212, 213, 214, 215, 221, 224];

/// **Bits `[start, start + width_bits)` of a 128-bit register** held as four words, low word first.
/// `width_bits` above 32 is clamped to 32; positions past bit 127 read as zero.
#[must_use]
pub(crate) const fn extract_bits(register: &[u32; 4], start: u32, width_bits: u32) -> u32 {
    let width_bits = if width_bits > 32 { 32 } else { width_bits };
    if width_bits == 0 || start >= 128 {
        return 0;
    }
    let word = (start / 32) as usize;
    let shift = start % 32;
    let mut v = (register[word] as u64) >> shift;
    if shift + width_bits > 32 && word + 1 < 4 {
        v |= (register[word + 1] as u64) << (32 - shift);
    }
    let mask = if width_bits == 32 {
        0xffff_ffff
    } else {
        (1u64 << width_bits) - 1
    };
    (v & mask) as u32
}

/// **The card-specific data register (CSD)**, decoded to the two facts this driver uses: SD
/// Physical Layer Simplified Specification section 5.3, `JESD84-B51` section 7.3 for eMMC.
#[doc(alias = "csd")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CardSpecificData {
    /// The structure version, bits 127:126: 0 standard capacity, 1 high or extended capacity
    /// (SDHC and `SDXC`), 2 ultra capacity (`SDUC`). For eMMC it is a different field with different
    /// meaning; see [`CardSpecificData::from_sd_register`]'s caller.
    pub(crate) structure: u32,
    /// Capacity in 512-byte blocks, or 0 when the structure is one this driver does not decode.
    pub(crate) blocks: u64,
}

impl CardSpecificData {
    /// **Decode an SD card's CSD.**
    #[must_use]
    pub(crate) const fn from_sd_register(register: &[u32; 4]) -> CardSpecificData {
        let structure = extract_bits(register, 126, 2);
        let blocks = match structure {
            // (C_SIZE + 1) * 2^(C_SIZE_MULT + 2) blocks of 2^READ_BL_LEN bytes. READ_BL_LEN is 9,
            // 10 or 11 on every card made; anything from 9 to 15 is decoded and a smaller one
            // refused rather than divided.
            0 => {
                let device_size = extract_bits(register, 62, 12) as u64;
                let size_multiplier = extract_bits(register, 47, 3);
                let read_block_length_exponent = extract_bits(register, 80, 4);
                if read_block_length_exponent < 9 {
                    0
                } else {
                    (device_size + 1) << (size_multiplier + 2 + read_block_length_exponent - 9)
                }
            }
            // (C_SIZE + 1) * 512 KiB, C_SIZE 22 bits.
            1 => (extract_bits(register, 48, 22) as u64 + 1) * 1024,
            // SDUC: the same, C_SIZE 28 bits.
            2 => (extract_bits(register, 48, 28) as u64 + 1) * 1024,
            _ => 0,
        };
        CardSpecificData { structure, blocks }
    }

    /// **Decode an eMMC's CSD**, which gives a capacity only up to 2 gigabytes. Above that `C_SIZE` reads
    /// `0xfff` and the true count is `EXT_CSD`'s [`extended_card_specific_data_sectors`]. The
    /// layout of the size fields is the same as SD's version 1.
    #[must_use]
    pub(crate) const fn multimedia_card(register: &[u32; 4]) -> CardSpecificData {
        let mut card_specific_data = CardSpecificData::from_sd_register(register);
        let device_size = extract_bits(register, 62, 12) as u64;
        let size_multiplier = extract_bits(register, 47, 3);
        let read_block_length_exponent = extract_bits(register, 80, 4);
        card_specific_data.blocks = if read_block_length_exponent < 9 {
            0
        } else {
            (device_size + 1) << (size_multiplier + 2 + read_block_length_exponent - 9)
        };
        card_specific_data
    }
}

/// **The card identification register (CID)**, decoded to what a transcript should name: SD
/// Physical Layer Simplified Specification section 5.2, `JESD84-B51` section 7.2 for eMMC.
#[doc(alias = "cid")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardIdentification {
    /// Manufacturer identifier, bits 127:120.
    pub manufacturer: u8,
    /// Product name: five ASCII bytes for SD (bits 103:64), six for eMMC (bits 103:56), padded with
    /// zero.
    pub name: [u8; 6],
    /// Product serial number, bits 55:24 (SD) or 47:16 (eMMC).
    pub serial: u32,
}

impl CardIdentification {
    /// Decode an SD card's CID.
    #[must_use]
    pub(crate) const fn from_sd_register(register: &[u32; 4]) -> CardIdentification {
        let mut name = [0u8; 6];
        let mut i = 0;
        while i < 5 {
            name[i] = extract_bits(register, 96 - 8 * i as u32, 8) as u8;
            i += 1;
        }
        CardIdentification {
            manufacturer: extract_bits(register, 120, 8) as u8,
            name,
            serial: extract_bits(register, 24, 32),
        }
    }

    /// Decode an eMMC's CID.
    #[must_use]
    pub(crate) const fn multimedia_card(register: &[u32; 4]) -> CardIdentification {
        let mut name = [0u8; 6];
        let mut i = 0;
        while i < 6 {
            name[i] = extract_bits(register, 96 - 8 * i as u32, 8) as u8;
            i += 1;
        }
        CardIdentification {
            manufacturer: extract_bits(register, 120, 8) as u8,
            name,
            serial: extract_bits(register, 16, 32),
        }
    }
}

/// The extended card-specific data register's (`EXT_CSD`) `SEC_COUNT`, bytes 212 to 215,
/// little-endian: an eMMC's capacity in 512-byte sectors. Zero for a buffer too short to hold it.
#[doc(alias = "ext_csd")]
#[must_use]
pub(crate) fn extended_card_specific_data_sectors(extended_card_specific_data: &[u8]) -> u64 {
    match extended_card_specific_data.get(212..216) {
        Some(b) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 32 gigabytes SDHC card: structure 1, `C_SIZE` (bits 69:48) `60_947`.
    #[test]
    fn a_32_gigabyte_high_capacity_card_specific_data() {
        let csd = [0, 60_947u32 << 16, 0, 1 << 30];
        assert_eq!(
            CardSpecificData::from_sd_register(&csd).blocks,
            60_948 * 1024
        );
    }

    fn put(r: &mut [u32; 4], start: u32, len: u32, v: u32) {
        for i in 0..len {
            let bit = start + i;
            let w = (bit / 32) as usize;
            let mask = 1u32 << (bit % 32);
            if (v >> i) & 1 == 1 {
                r[w] |= mask;
            } else {
                r[w] &= !mask;
            }
        }
    }

    #[test]
    fn a_field_that_straddles_two_words_is_read_whole() {
        let mut r = [0u32; 4];
        put(&mut r, 62, 12, 0xabc);
        assert_eq!(extract_bits(&r, 62, 12), 0xabc);
        put(&mut r, 120, 8, 0x1b);
        assert_eq!(extract_bits(&r, 120, 8), 0x1b);
        assert_eq!(extract_bits(&r, 128, 8), 0);
        assert_eq!(extract_bits(&r, 0, 0), 0);
    }

    #[test]
    fn a_standard_capacity_card_is_sized_from_three_fields() {
        // A 2 GB card: C_SIZE 4095, C_SIZE_MULT 7, READ_BL_LEN 10.
        let mut r = [0u32; 4];
        put(&mut r, 126, 2, 0);
        put(&mut r, 62, 12, 4095);
        put(&mut r, 47, 3, 7);
        put(&mut r, 80, 4, 10);
        assert_eq!(
            CardSpecificData::from_sd_register(&r).blocks * 512,
            2 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn a_read_block_length_below_512_is_refused_rather_than_shifted_negative() {
        let mut r = [0u32; 4];
        put(&mut r, 80, 4, 3);
        assert_eq!(CardSpecificData::from_sd_register(&r).blocks, 0);
        assert_eq!(CardSpecificData::multimedia_card(&r).blocks, 0);
    }

    #[test]
    fn an_extended_capacity_sd_card_is_sized_in_half_megabytes() {
        let mut r = [0u32; 4];
        put(&mut r, 126, 2, 1);
        put(&mut r, 48, 22, 0x3f_ffff);
        assert_eq!(
            CardSpecificData::from_sd_register(&r).blocks,
            0x40_0000 * 1024
        );
    }

    #[test]
    fn a_card_identification_names_its_maker_and_product() {
        let mut r = [0u32; 4];
        put(&mut r, 120, 8, 0x03);
        for (i, b) in b"SN32G".iter().enumerate() {
            put(&mut r, 96 - 8 * i as u32, 8, u32::from(*b));
        }
        put(&mut r, 24, 32, 0xdead_beef);
        let card_identification = CardIdentification::from_sd_register(&r);
        assert_eq!(card_identification.manufacturer, 3);
        assert_eq!(&card_identification.name[..5], b"SN32G");
        assert_eq!(card_identification.serial, 0xdead_beef);
    }

    #[test]
    fn extended_card_specific_data_sector_count_is_little_endian_and_short_buffers_read_zero() {
        let mut e = [0u8; 512];
        e[212..216].copy_from_slice(&0x01d2_0000u32.to_le_bytes());
        assert_eq!(extended_card_specific_data_sectors(&e), 0x01d2_0000);
        assert_eq!(extended_card_specific_data_sectors(&e[..100]), 0);
    }

    #[test]
    fn r1_state_and_errors() {
        let ready_in_transfer = (R1_STATE_TRANSFER << 9) | R1_READY_FOR_DATA;
        assert_eq!(r1_state(ready_in_transfer), R1_STATE_TRANSFER);
        assert_eq!(ready_in_transfer & R1_ERROR_MASK, 0);
        // CARD_IS_LOCKED is not an error; ILLEGAL_COMMAND is.
        assert_eq!((1 << 25) & R1_ERROR_MASK, 0);
        assert_ne!((1 << 22) & R1_ERROR_MASK, 0);
        assert_eq!(R1_READY_FOR_DATA, 0x100);
    }

    /// The arguments a card is sent, as numbers: what a protocol analyzer on the bus would show.
    #[test]
    fn the_operating_condition_arguments_are_the_specifications() {
        assert_eq!(OPERATION_CONDITIONS_VOLTAGE_WINDOW, 0x0030_0000);
        assert_eq!(SD_OPERATION_CONDITIONS_ARGUMENT, 0x4030_0000);
    }

    #[test]
    fn a_length_above_32_reads_32_bits_and_a_field_past_the_end_reads_zero_there() {
        let mut r = [0u32; 4];
        put(&mut r, 0, 32, 0x8765_4321);
        put(&mut r, 32, 32, 0x0fed_cba9);
        put(&mut r, 96, 32, 0xa5c3_0f1e);
        assert_eq!(extract_bits(&r, 0, 64), 0x8765_4321);
        // Shift 1, length 32: the field's top bit is the next word's bit 0.
        assert_eq!(
            extract_bits(&r, 1, 32),
            0x8765_4321 >> 1 | 0x0fed_cba9 << 31
        );
        // Bits 120 to 135: the eight that exist, and nothing read past the last word.
        assert_eq!(extract_bits(&r, 120, 16), 0xa5);
    }

    /// `READ_BL_LEN` 9 (512-byte blocks) is the case where the shift's `- 9` is zero, and
    /// `C_SIZE_MULT` 3 the one where the multiplier's `+ 2` matters: (100 + 1) * 2^(3 + 2) blocks.
    #[test]
    fn a_card_specific_data_with_512_byte_blocks_is_sized_from_its_multiplier_alone() {
        let mut r = [0u32; 4];
        put(&mut r, 62, 12, 100);
        put(&mut r, 47, 3, 3);
        put(&mut r, 80, 4, 9);
        assert_eq!(CardSpecificData::from_sd_register(&r).blocks, 101 << 5);
        assert_eq!(CardSpecificData::multimedia_card(&r).blocks, 101 << 5);
    }

    #[test]
    fn an_ultra_capacity_sd_card_is_sized_from_a_28_bit_count() {
        let mut r = [0u32; 4];
        put(&mut r, 126, 2, 2);
        put(&mut r, 48, 28, 0x0abc_def0);
        let card_specific_data = CardSpecificData::from_sd_register(&r);
        assert_eq!(card_specific_data.structure, 2);
        assert_eq!(card_specific_data.blocks, (0x0abc_def0 + 1) * 1024);
    }

    /// An SD CID has a five-byte name; the byte after it is the product revision, not a sixth
    /// letter. An eMMC's name is six bytes and its serial sits eight bits lower.
    #[test]
    fn an_sd_name_is_five_bytes_and_an_embedded_multimedia_card_name_six() {
        let mut r = [0u32; 4];
        for (i, b) in b"TESTMC".iter().enumerate() {
            put(&mut r, 96 - 8 * i as u32, 8, u32::from(*b));
        }
        put(&mut r, 120, 8, 0x15);
        assert_eq!(CardIdentification::from_sd_register(&r).name, *b"TESTM\0");
        let mut multimedia_card = r;
        put(&mut multimedia_card, 16, 32, 0x1357_9bdf);
        let card_identification = CardIdentification::multimedia_card(&multimedia_card);
        assert_eq!(
            (
                card_identification.manufacturer,
                card_identification.name,
                card_identification.serial
            ),
            (0x15, *b"TESTMC", 0x1357_9bdf)
        );
    }
}
