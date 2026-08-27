//! Deterministically stamp a WebAssembly module or component with a build ID.

use std::fmt;
use std::ops::Range;

use sha2::{Digest, Sha256};

const WASM_MAGIC: &[u8; 4] = b"\0asm";
const WASM_HEADER_LEN: usize = 8;
const CUSTOM_SECTION_NAME: &[u8] = b"build_id";
const ID_LEN: usize = 32;
const SLOT_PREFIX: &[u8; 16] = b"buildid-wasm-v1:";
const SLOT_SUFFIX: &[u8; 16] = b":buildid-wasm-v1";
const UNSTAMPED_ID: &[u8; ID_LEN] = b"unstamped-buildid-wasm-v1-000000";
const MODE_UNSTAMPED: u8 = 0xff;
const UNSTAMPED_LEN: u8 = 0xff;
const MODE_HASH: u8 = 1;
const MODE_TOP_LEVEL: u8 = 2;

/// The result of stamping a WebAssembly binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stamped {
    /// Rewritten WebAssembly bytes.
    pub wasm: Vec<u8>,
    /// Build ID written to the guest slot and custom section.
    pub id: Vec<u8>,
}

/// Error returned for an invalid or unstamped WebAssembly binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error(String);

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Stamp `wasm`, replacing any top-level `build_id` custom sections.
///
/// The ID is SHA-256 over a canonical form of the input with top-level
/// `build_id` custom sections removed and the guest-visible ID slot reset to
/// its unstamped value.
/// The same ID is written into that slot and a conventional `build_id` custom
/// section. Re-stamping unchanged input produces identical output.
pub fn stamp(wasm: &[u8]) -> Result<Stamped, Error> {
    let stripped = strip_build_id_sections(wasm)?;
    let slots = find_stamp_slots(&stripped.wasm)?;
    let id = canonical_id(&stripped.wasm, &slots);

    let mut output = stripped.wasm;
    write_slots(&mut output, &slots, MODE_HASH, &id)?;
    append_build_id_section(&mut output, &id);

    Ok(Stamped {
        wasm: output,
        id: id.to_vec(),
    })
}

/// Stamp guest slots with the input binary's top-level `build_id` section.
///
/// For a core module this is the module's own ID. For a component, every slot
/// in every embedded core module receives the outer component ID. All slots
/// are marked with top-level provenance.
pub fn stamp_top_level(wasm: &[u8]) -> Result<Stamped, Error> {
    let stripped = strip_build_id_sections(wasm)?;
    if stripped.build_ids.len() != 1 {
        return Err(Error::new(format!(
            "expected exactly one top-level build_id custom section, found {}",
            stripped.build_ids.len()
        )));
    }

    let id = decode_build_id(&stripped.build_ids[0])?.to_vec();
    validate_id_length(&id)?;
    let slots = find_stamp_slots(&stripped.wasm)?;

    let mut output = stripped.wasm;
    write_slots(&mut output, &slots, MODE_TOP_LEVEL, &id)?;
    append_build_id_section(&mut output, &id);

    Ok(Stamped { wasm: output, id })
}

/// Validate that `wasm` has matching guest and custom-section IDs.
///
/// Stamper-generated IDs are also checked for freshness. An imported ID can
/// only be checked for consistency because its producer defines its meaning.
pub fn check(wasm: &[u8]) -> Result<Vec<u8>, Error> {
    let stripped = strip_build_id_sections(wasm)?;
    if stripped.build_ids.len() != 1 {
        return Err(Error::new(format!(
            "expected exactly one top-level build_id custom section, found {}",
            stripped.build_ids.len()
        )));
    }

    let custom_id = decode_build_id(&stripped.build_ids[0])?;
    validate_id_length(custom_id)?;

    let slots = find_stamp_slots(&stripped.wasm)?;
    let expected_hash = canonical_id(&stripped.wasm, &slots);
    for slot in &slots {
        let mode = stripped.wasm[slot.mode];
        let slot_len = usize::from(stripped.wasm[slot.len]);
        if slot_len == 0 || slot_len > ID_LEN {
            return Err(Error::new(format!(
                "guest stamp has invalid build-ID length {slot_len}"
            )));
        }
        let slot_id = &stripped.wasm[slot.id.start..slot.id.start + slot_len];
        if slot_id != custom_id {
            return Err(Error::new(
                "guest stamp and build_id custom section do not match",
            ));
        }

        match mode {
            MODE_HASH => {
                if slot_len != ID_LEN {
                    return Err(Error::new(format!(
                        "stamper-generated build ID has length {slot_len}, expected {ID_LEN}"
                    )));
                }
                if slot_id != expected_hash {
                    return Err(Error::new("build ID is stale for this WebAssembly binary"));
                }
            }
            MODE_TOP_LEVEL => {}
            _ => return Err(Error::new(format!("guest stamp has invalid mode {mode}"))),
        }
    }

    Ok(custom_id.to_vec())
}

struct Stripped {
    wasm: Vec<u8>,
    build_ids: Vec<Vec<u8>>,
}

fn strip_build_id_sections(wasm: &[u8]) -> Result<Stripped, Error> {
    if wasm.len() < WASM_HEADER_LEN || &wasm[..WASM_MAGIC.len()] != WASM_MAGIC {
        return Err(Error::new("input is not a WebAssembly binary"));
    }

    let mut output = Vec::with_capacity(wasm.len());
    output.extend_from_slice(&wasm[..WASM_HEADER_LEN]);

    let mut build_ids = Vec::new();
    let mut position = WASM_HEADER_LEN;
    while position < wasm.len() {
        let section_start = position;
        let section_id = wasm[position];
        position += 1;
        let section_len = read_u32_leb(wasm, &mut position)? as usize;
        let section_end = position
            .checked_add(section_len)
            .filter(|end| *end <= wasm.len())
            .ok_or_else(|| Error::new("WebAssembly section extends past end of file"))?;

        let mut is_build_id = false;
        if section_id == 0 {
            let mut custom_position = position;
            let name_len = read_u32_leb_before(wasm, &mut custom_position, section_end)? as usize;
            let name_end = custom_position
                .checked_add(name_len)
                .filter(|end| *end <= section_end)
                .ok_or_else(|| Error::new("custom section name extends past section end"))?;

            if &wasm[custom_position..name_end] == CUSTOM_SECTION_NAME {
                build_ids.push(wasm[name_end..section_end].to_vec());
                is_build_id = true;
            }
        }

        if !is_build_id {
            output.extend_from_slice(&wasm[section_start..section_end]);
        }
        position = section_end;
    }

    Ok(Stripped {
        wasm: output,
        build_ids,
    })
}

#[derive(Clone, Debug)]
struct Slot {
    mode: usize,
    len: usize,
    id: Range<usize>,
}

fn find_stamp_slots(wasm: &[u8]) -> Result<Vec<Slot>, Error> {
    let frame_len = SLOT_PREFIX.len() + 2 + ID_LEN + SLOT_SUFFIX.len();
    let mut found = Vec::new();

    let Some(last_prefix_start) = wasm.len().checked_sub(frame_len) else {
        return Err(slot_not_found());
    };

    for prefix_start in 0..=last_prefix_start {
        if &wasm[prefix_start..prefix_start + SLOT_PREFIX.len()] != SLOT_PREFIX {
            continue;
        }

        let mode = prefix_start + SLOT_PREFIX.len();
        let len = mode + 1;
        let id_start = len + 1;
        let id_end = id_start + ID_LEN;
        let suffix_end = id_end + SLOT_SUFFIX.len();
        if &wasm[id_end..suffix_end] != SLOT_SUFFIX {
            continue;
        }

        found.push(Slot {
            mode,
            len,
            id: id_start..id_end,
        });
    }

    if found.is_empty() {
        Err(slot_not_found())
    } else {
        Ok(found)
    }
}

fn slot_not_found() -> Error {
    Error::new("buildid Wasm stamp slot not found; link the module with the buildid crate first")
}

fn canonical_id(wasm_without_build_id: &[u8], slots: &[Slot]) -> [u8; ID_LEN] {
    let mut canonical = wasm_without_build_id.to_vec();
    for slot in slots {
        canonical[slot.mode] = MODE_UNSTAMPED;
        canonical[slot.len] = UNSTAMPED_LEN;
        canonical[slot.id.clone()].copy_from_slice(UNSTAMPED_ID);
    }
    Sha256::digest(canonical).into()
}

fn write_slots(wasm: &mut [u8], slots: &[Slot], mode: u8, id: &[u8]) -> Result<(), Error> {
    for slot in slots {
        write_slot(wasm, slot, mode, id)?;
    }
    Ok(())
}

fn write_slot(wasm: &mut [u8], slot: &Slot, mode: u8, id: &[u8]) -> Result<(), Error> {
    validate_id_length(id)?;
    wasm[slot.mode] = mode;
    wasm[slot.len] = id.len() as u8;
    wasm[slot.id.clone()].copy_from_slice(UNSTAMPED_ID);
    wasm[slot.id.start..slot.id.start + id.len()].copy_from_slice(id);
    Ok(())
}

fn validate_id_length(id: &[u8]) -> Result<(), Error> {
    if id.is_empty() || id.len() > ID_LEN {
        return Err(Error::new(format!(
            "build ID must contain between 1 and {ID_LEN} bytes, found {}",
            id.len()
        )));
    }
    Ok(())
}

fn append_build_id_section(wasm: &mut Vec<u8>, id: &[u8]) {
    let mut payload = Vec::with_capacity(CUSTOM_SECTION_NAME.len() + ID_LEN + 8);
    write_u32_leb(CUSTOM_SECTION_NAME.len() as u32, &mut payload);
    payload.extend_from_slice(CUSTOM_SECTION_NAME);
    write_u32_leb(id.len() as u32, &mut payload);
    payload.extend_from_slice(id);

    wasm.push(0);
    write_u32_leb(payload.len() as u32, wasm);
    wasm.extend_from_slice(&payload);
}

fn decode_build_id(payload: &[u8]) -> Result<&[u8], Error> {
    let mut position = 0;
    let id_len = read_u32_leb(payload, &mut position)? as usize;
    let id_end = position
        .checked_add(id_len)
        .filter(|end| *end == payload.len())
        .ok_or_else(|| Error::new("malformed build_id custom section payload"))?;
    Ok(&payload[position..id_end])
}

fn read_u32_leb(bytes: &[u8], position: &mut usize) -> Result<u32, Error> {
    read_u32_leb_before(bytes, position, bytes.len())
}

fn read_u32_leb_before(bytes: &[u8], position: &mut usize, end: usize) -> Result<u32, Error> {
    let mut value = 0u32;
    for byte_index in 0..5 {
        let byte = *bytes
            .get(*position)
            .filter(|_| *position < end)
            .ok_or_else(|| Error::new("truncated unsigned LEB128 value"))?;
        *position += 1;

        if byte_index == 4 && byte & 0xf0 != 0 {
            return Err(Error::new("unsigned LEB128 value overflows u32"));
        }
        value |= u32::from(byte & 0x7f) << (byte_index * 7);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }

    Err(Error::new("unsigned LEB128 value is too long"))
}

fn write_u32_leb(mut value: u32, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unstamped(extra: &[u8]) -> Vec<u8> {
        let mut wasm = b"\0asm\x01\0\0\0".to_vec();

        let mut data = Vec::new();
        data.extend_from_slice(extra);
        data.extend_from_slice(SLOT_PREFIX);
        data.push(MODE_UNSTAMPED);
        data.push(UNSTAMPED_LEN);
        data.extend_from_slice(UNSTAMPED_ID);
        data.extend_from_slice(SLOT_SUFFIX);

        // An opaque custom section is sufficient for testing framing and
        // top-level section rewriting; the stamper deliberately preserves
        // non-build-ID section payloads without interpreting them.
        wasm.push(0);
        write_u32_leb((1 + data.len()) as u32, &mut wasm);
        wasm.push(0); // empty custom-section name
        wasm.extend_from_slice(&data);
        wasm
    }

    fn with_build_id(mut wasm: Vec<u8>, id: &[u8]) -> Vec<u8> {
        append_build_id_section(&mut wasm, id);
        wasm
    }

    fn component_with_core_module(module: &[u8]) -> Vec<u8> {
        let mut component = b"\0asm\x0d\0\x01\0".to_vec();
        component.push(1);
        write_u32_leb(module.len() as u32, &mut component);
        component.extend_from_slice(module);
        component
    }

    #[test]
    fn stamp_is_valid_and_idempotent() {
        let first = stamp(&unstamped(b"first")).unwrap();
        assert_ne!(first.id, vec![0; ID_LEN]);
        assert_eq!(check(&first.wasm).unwrap(), first.id);

        let second = stamp(&first.wasm).unwrap();
        assert_eq!(second, first);
    }

    #[test]
    fn different_input_gets_a_different_id() {
        let first = stamp(&unstamped(b"first")).unwrap();
        let second = stamp(&unstamped(b"second")).unwrap();
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn copies_the_top_level_build_id_into_a_core_module() {
        let id = b"linker-build-id!";
        let first = stamp_top_level(&with_build_id(unstamped(b"content"), id)).unwrap();
        assert_eq!(first.id, id);
        assert_eq!(check(&first.wasm).unwrap(), id);

        let stripped = strip_build_id_sections(&first.wasm).unwrap();
        let slots = find_stamp_slots(&stripped.wasm).unwrap();
        let slot = &slots[0];
        assert_eq!(stripped.wasm[slot.mode], MODE_TOP_LEVEL);
        assert_eq!(usize::from(stripped.wasm[slot.len]), id.len());
        assert_eq!(&stripped.wasm[slot.id.start..slot.id.start + id.len()], id);

        let second = stamp_top_level(&first.wasm).unwrap();
        assert_eq!(second, first);
    }

    #[test]
    fn copies_the_top_level_component_id_into_all_nested_slots() {
        let outer_id = b"outer-build-id!";
        let module_a = unstamped(b"first");
        let module_b = unstamped(b"second");
        let mut component = component_with_core_module(&module_a);
        component.push(1);
        write_u32_leb(module_b.len() as u32, &mut component);
        component.extend_from_slice(&module_b);
        append_build_id_section(&mut component, outer_id);

        let stamped = stamp_top_level(&component).unwrap();
        assert_eq!(stamped.id, outer_id);
        assert_eq!(check(&stamped.wasm).unwrap(), outer_id);

        let stripped = strip_build_id_sections(&stamped.wasm).unwrap();
        let slots = find_stamp_slots(&stripped.wasm).unwrap();
        assert_eq!(slots.len(), 2);
        assert!(slots
            .iter()
            .all(|slot| stripped.wasm[slot.mode] == MODE_TOP_LEVEL));
    }

    #[test]
    fn top_level_mode_replaces_content_provenance() {
        let generated = stamp(&unstamped(b"content")).unwrap();
        let imported = stamp_top_level(&generated.wasm).unwrap();

        assert_eq!(imported.id, generated.id);
        assert_eq!(check(&imported.wasm).unwrap(), generated.id);
        let stripped = strip_build_id_sections(&imported.wasm).unwrap();
        let slots = find_stamp_slots(&stripped.wasm).unwrap();
        assert_eq!(stripped.wasm[slots[0].mode], MODE_TOP_LEVEL);
    }

    #[test]
    fn top_level_build_id_must_be_present_and_fit_the_slot() {
        assert!(stamp_top_level(&unstamped(b"missing"))
            .unwrap_err()
            .to_string()
            .contains("found 0"));

        let oversized = vec![7; ID_LEN + 1];
        assert!(
            stamp_top_level(&with_build_id(unstamped(b"large"), &oversized))
                .unwrap_err()
                .to_string()
                .contains("between 1 and 32 bytes")
        );
    }

    #[test]
    fn check_detects_a_stale_id() {
        let mut stamped = stamp(&unstamped(b"content")).unwrap().wasm;
        stamped[WASM_HEADER_LEN + 3] ^= 1;
        assert!(check(&stamped).unwrap_err().to_string().contains("stale"));
    }

    #[test]
    fn stamp_updates_every_slot_in_one_core_module() {
        let mut extra_slot = Vec::new();
        extra_slot.extend_from_slice(SLOT_PREFIX);
        extra_slot.push(MODE_UNSTAMPED);
        extra_slot.push(UNSTAMPED_LEN);
        extra_slot.extend_from_slice(UNSTAMPED_ID);
        extra_slot.extend_from_slice(SLOT_SUFFIX);
        let duplicate = unstamped(&extra_slot);
        let stamped = stamp(&duplicate).unwrap();
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.id);
        let stripped = strip_build_id_sections(&stamped.wasm).unwrap();
        let slots = find_stamp_slots(&stripped.wasm).unwrap();
        assert_eq!(slots.len(), 2);
        assert!(slots
            .iter()
            .all(|slot| stripped.wasm[slot.mode] == MODE_HASH));
    }

    #[test]
    fn stamp_rejects_non_wasm_input() {
        assert_eq!(
            stamp(b"not wasm").unwrap_err().to_string(),
            "input is not a WebAssembly binary"
        );
    }

    #[test]
    fn stamp_rejects_wasm_too_small_for_a_slot() {
        assert_eq!(
            stamp(b"\0asm\x01\0\0\0").unwrap_err().to_string(),
            "buildid Wasm stamp slot not found; link the module with the buildid crate first"
        );
    }

    #[test]
    fn stamps_component_binary_framing() {
        let mut component = unstamped(b"component");
        component[4..8].copy_from_slice(&[0x0d, 0, 1, 0]);
        let stamped = stamp(&component).unwrap();
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.id);
    }
}
