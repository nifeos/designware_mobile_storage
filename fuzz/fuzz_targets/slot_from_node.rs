//! Fuzz `Slot::from_node` over any property table.
//!
//! The first nine bytes are the slot's index and the `reg` address and size. Every later property
//! lookup takes a control byte: its low bit makes the lookup fail with an error, its next bit makes
//! the property absent, and otherwise its high nibble is the value's length in bytes (zero to
//! fifteen), taken from the input that follows. When the control byte's third bit is set and the
//! property is `compatible`, the value is replaced by one of the two real bindings, so the input
//! need not guess them to reach the accepting path. Reading the table must not panic, an error
//! must come back as the error the lookup gave, and a slot it returns must keep the rules the type
//! states: the compatible string is one of the two bindings, the `reg` and the index are kept as
//! given, and the card interface clock is never zero.

#![no_main]

use designware_mobile_storage::jh7110::{COMPATIBLE_MAINLINE, COMPATIBLE_VENDOR, Slot};
use libfuzzer_sys::fuzz_target;

const HEADER_BYTES: usize = 17;
const FAILED: u8 = 0xe5;

fuzz_target!(|data: &[u8]| {
    if data.len() < HEADER_BYTES {
        return;
    }
    let (header, mut rest) = data.split_at(HEADER_BYTES);
    let index = header[0];
    let address = u64::from_le_bytes(header[1..9].try_into().unwrap());
    let size = u64::from_le_bytes(header[9..17].try_into().unwrap());

    let result = Slot::from_node(index, address, size, |name: &[u8]| {
        let Some((&control, after)) = rest.split_first() else {
            return Ok(None);
        };
        if control & 1 != 0 {
            rest = after;
            return Err(FAILED);
        }
        if control & 2 != 0 {
            rest = after;
            return Ok(None);
        }
        if control & 4 != 0 && name == b"compatible" {
            rest = after;
            return Ok(Some(if control & 8 != 0 {
                COMPATIBLE_VENDOR
            } else {
                COMPATIBLE_MAINLINE
            }));
        }
        let length = usize::from(control >> 4).min(after.len());
        let (value, after) = after.split_at(length);
        rest = after;
        Ok(Some(value))
    });

    match result {
        Err(error) => assert_eq!(error, FAILED),
        Ok(None) => {}
        Ok(Some(slot)) => {
            assert!(slot.compatible == COMPATIBLE_MAINLINE || slot.compatible == COMPATIBLE_VENDOR);
            assert_eq!(slot.index, index);
            assert_eq!(slot.registers_physical_address, address);
            assert_eq!(slot.registers_size_bytes, size);
            assert_ne!(slot.card_interface_unit_hertz, 0);
        }
    }
});
