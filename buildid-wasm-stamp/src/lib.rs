//! Deterministically stamp a WebAssembly module or component with a build ID.

use std::fmt;
use std::ops::Range;

use buildid::wasm_stamp::{
    CUSTOM_SECTION_NAME, ID_LEN, MODE_HASH, MODE_MODULE, MODE_TOP_LEVEL, MODE_UNSTAMPED,
    SLOT_PREFIX, SLOT_SUFFIX, UNSTAMPED_ID, UNSTAMPED_LEN,
};
use sha2::{Digest, Sha256};
use wasmparser::{Encoding, Parser, Payload};

const WASM_MAGIC: &[u8; 4] = b"\0asm";
const WASM_HEADER_LEN: usize = 8;

/// The result of stamping a WebAssembly binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stamped {
    /// Rewritten WebAssembly bytes.
    pub wasm: Vec<u8>,
    /// One ID per core module containing slots, in file order. IDs may repeat.
    pub ids: Vec<Vec<u8>>,
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

/// Copy each containing core module's existing build ID into its guest slots.
///
/// Components are traversed recursively. Only modules containing stamp slots
/// need a `build_id` section. All custom sections, including outer component
/// IDs and the original module IDs, are preserved byte for byte.
pub fn stamp_modules(wasm: &[u8]) -> Result<Stamped, Error> {
    let layout = parse_layout(wasm)?;
    let mut output = wasm.to_vec();
    let mut ids = Vec::new();
    for (index, module) in layout.modules.iter().enumerate() {
        if module.slots.is_empty() {
            continue;
        }
        let id = existing_id(&module.build_ids, format_args!("core module {index}"))?;
        write_slots(&mut output, &module.slots, MODE_MODULE, id)?;
        ids.push(id.to_vec());
    }
    Ok(Stamped { wasm: output, ids })
}

/// Stamp `wasm`, replacing any top-level `build_id` custom sections.
///
/// The ID is SHA-256 over a canonical form of the input with top-level
/// `build_id` custom sections removed and the guest-visible ID slot reset to
/// its unstamped value.
/// The same ID is written into that slot and a conventional `build_id` custom
/// section. Re-stamping unchanged input produces identical output.
pub fn stamp(wasm: &[u8]) -> Result<Stamped, Error> {
    let layout = parse_layout(wasm)?;
    let id = canonical_id(wasm, &layout);

    // Allocate only the returned binary. Canonicalization and section removal
    // are streamed directly from the original input, without a staging copy.
    let removed: usize = layout
        .outer_id_sections
        .iter()
        .map(|range| range.len())
        .sum();
    let mut output =
        Vec::with_capacity(wasm.len() - removed + CUSTOM_SECTION_NAME.len() + ID_LEN + 8);
    content_chunks(wasm, &layout, MODE_HASH, ID_LEN as u8, &id, |chunk| {
        output.extend_from_slice(chunk)
    });
    append_build_id_section(&mut output, &id);

    Ok(Stamped {
        wasm: output,
        ids: layout.ids_for_modules(&id),
    })
}

/// Stamp guest slots with the input binary's top-level `build_id` section.
///
/// For a core module this is the module's own ID. For a component, every slot
/// in every embedded core module receives the outer component ID. All slots
/// are marked with top-level provenance.
pub fn stamp_top_level(wasm: &[u8]) -> Result<Stamped, Error> {
    let layout = parse_layout(wasm)?;
    let id = existing_id(&layout.outer_build_ids, "top-level")?;
    let slots = layout.slots();
    let mut output = wasm.to_vec();
    write_slots(&mut output, slots, MODE_TOP_LEVEL, id)?;
    Ok(Stamped {
        wasm: output,
        ids: layout.ids_for_modules(id),
    })
}

/// Validate every guest ID against the custom section selected by its mode.
///
/// Stamper-generated IDs are also checked for freshness. An imported ID can
/// only be checked for consistency because its producer defines its meaning.
/// Returns one ID per core module containing slots, in file order.
pub fn check(wasm: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    let layout = parse_layout(wasm)?;
    let mut expected_hash = None;
    let mut ids = Vec::new();
    for (index, module) in layout.modules.iter().enumerate() {
        let Some(first) = module.slots.first() else {
            continue;
        };
        let mode = wasm[first.mode];
        let id = match mode {
            MODE_MODULE => existing_id(&module.build_ids, format_args!("core module {index}"))?,
            MODE_HASH | MODE_TOP_LEVEL => existing_id(&layout.outer_build_ids, "top-level")?,
            _ => return Err(Error::new(format!("guest stamp has invalid mode {mode}"))),
        };
        for slot in &module.slots {
            if wasm[slot.mode] != mode {
                return Err(Error::new(
                    "guest stamps in one core module have mixed modes",
                ));
            }
            let len = usize::from(wasm[slot.len]);
            if len == 0 || len > ID_LEN {
                return Err(Error::new(format!(
                    "guest stamp has invalid build-ID length {len}"
                )));
            }
            if &wasm[slot.id.start..slot.id.start + len] != id {
                return Err(Error::new(
                    "guest stamp and build_id custom section do not match",
                ));
            }
        }
        if mode == MODE_HASH {
            if id.len() != ID_LEN {
                return Err(Error::new(
                    "stamper-generated build ID must contain 32 bytes",
                ));
            }
            let hash = match expected_hash {
                Some(hash) => hash,
                None => {
                    let hash = canonical_id(wasm, &layout);
                    expected_hash = Some(hash);
                    hash
                }
            };
            if id != hash.as_slice() {
                return Err(Error::new("build ID is stale for this WebAssembly binary"));
            }
        }
        ids.push(id.to_vec());
    }
    Ok(ids)
}

fn existing_id<'a>(payloads: &[&'a [u8]], scope: impl fmt::Display) -> Result<&'a [u8], Error> {
    if payloads.len() != 1 {
        return Err(Error::new(format!(
            "expected exactly one {scope} build_id custom section, found {}; \
             use --id-source content to generate an outer ID",
            payloads.len()
        )));
    }
    let id = decode_build_id(payloads[0])?;
    validate_id_length(id)?;
    Ok(id)
}

#[derive(Clone, Debug)]
struct Slot {
    mode: usize,
    len: usize,
    id: Range<usize>,
}

#[derive(Default)]
struct Module<'a> {
    build_ids: Vec<&'a [u8]>,
    slots: Vec<Slot>,
}

#[derive(Default)]
struct Layout<'a> {
    outer_build_ids: Vec<&'a [u8]>,
    outer_id_sections: Vec<Range<usize>>,
    modules: Vec<Module<'a>>,
}

impl Layout<'_> {
    fn slots(&self) -> impl Iterator<Item = &Slot> {
        self.modules.iter().flat_map(|module| &module.slots)
    }

    fn ids_for_modules(&self, id: &[u8]) -> Vec<Vec<u8>> {
        self.modules
            .iter()
            .filter(|module| !module.slots.is_empty())
            .map(|_| id.to_vec())
            .collect()
    }
}

fn parse_layout(wasm: &[u8]) -> Result<Layout<'_>, Error> {
    if wasm.len() < WASM_HEADER_LEN || &wasm[..WASM_MAGIC.len()] != WASM_MAGIC {
        return Err(Error::new("input is not a WebAssembly binary"));
    }
    let mut layout = Layout::default();
    struct Scope {
        module: Option<usize>,
        next_section: usize,
    }
    let mut stack: Vec<Scope> = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload.map_err(|error| Error::new(error.to_string()))?;
        // as_section() excludes the section header. Track the preceding
        // section's end to retain the actual header, including padded LEBs.
        let section_start = if let Some((_, range)) = payload.as_section() {
            let scope = stack
                .last_mut()
                .ok_or_else(|| Error::new("section outside a binary"))?;
            let start = scope.next_section;
            scope.next_section = range.end;
            start
        } else {
            0
        };
        match payload {
            Payload::Version {
                encoding, range, ..
            } => {
                let module = if encoding == Encoding::Module {
                    layout.modules.push(Module::default());
                    Some(layout.modules.len() - 1)
                } else {
                    None
                };
                stack.push(Scope {
                    module,
                    next_section: range.end,
                });
            }
            Payload::CustomSection(section) if section.name().as_bytes() == CUSTOM_SECTION_NAME => {
                if stack.len() == 1 {
                    layout.outer_build_ids.push(section.data());
                    layout
                        .outer_id_sections
                        .push(section_start..section.range().end);
                }
                if let Some(index) = stack.last().and_then(|scope| scope.module) {
                    layout.modules[index].build_ids.push(section.data());
                }
            }
            Payload::DataSection(data) => {
                let index = stack
                    .last()
                    .and_then(|scope| scope.module)
                    .ok_or_else(|| Error::new("data section outside a core module"))?;
                for segment in data {
                    let segment = segment.map_err(|error| Error::new(error.to_string()))?;
                    let start = segment.range.end - segment.data.len();
                    let slots = find_data_slots(segment.data, start);
                    if slots.windows(2).any(|pair| {
                        pair[1].mode - SLOT_PREFIX.len() < pair[0].id.end + SLOT_SUFFIX.len()
                    }) {
                        return Err(Error::new("overlapping buildid Wasm stamp slots"));
                    }
                    layout.modules[index].slots.extend(slots);
                }
            }
            Payload::End(_) => {
                stack.pop();
            }
            _ => {}
        }
    }
    if layout.modules.iter().all(|module| module.slots.is_empty()) {
        return Err(slot_not_found());
    }
    Ok(layout)
}

// Search each parsed data payload separately: never patch custom sections,
// instructions, initialization expressions, or frames spanning two segments.
fn find_data_slots(data: &[u8], offset: usize) -> Vec<Slot> {
    let frame_len = SLOT_PREFIX.len() + 2 + ID_LEN + SLOT_SUFFIX.len();
    let mut found = Vec::new();

    let Some(last_prefix_start) = data.len().checked_sub(frame_len) else {
        return found;
    };

    for prefix_start in 0..=last_prefix_start {
        if data[prefix_start] != SLOT_PREFIX[0]
            || &data[prefix_start..prefix_start + SLOT_PREFIX.len()] != SLOT_PREFIX
        {
            continue;
        }

        let mode = prefix_start + SLOT_PREFIX.len();
        let len = mode + 1;
        let id_start = len + 1;
        let id_end = id_start + ID_LEN;
        let suffix_end = id_end + SLOT_SUFFIX.len();
        if &data[id_end..suffix_end] != SLOT_SUFFIX {
            continue;
        }

        found.push(Slot {
            mode: offset + mode,
            len: offset + len,
            id: offset + id_start..offset + id_end,
        });
    }

    found
}

fn slot_not_found() -> Error {
    Error::new("buildid Wasm stamp slot not found; link the module with the buildid crate first")
}

fn canonical_id(wasm: &[u8], layout: &Layout<'_>) -> [u8; ID_LEN] {
    let mut hash = Sha256::new();
    content_chunks(
        wasm,
        layout,
        MODE_UNSTAMPED,
        UNSTAMPED_LEN,
        UNSTAMPED_ID,
        |chunk| hash.update(chunk),
    );
    hash.finalize().into()
}

// Both iterators are in file order and their ranges cannot overlap: slots
// belong to core data payloads, while removed sections belong to the outer
// binary. Hashing and output use the same stream, with different slot values.
fn content_chunks(
    wasm: &[u8],
    layout: &Layout<'_>,
    mode: u8,
    len: u8,
    id: &[u8; ID_LEN],
    mut emit: impl FnMut(&[u8]),
) {
    let mut sections = layout.outer_id_sections.iter().peekable();
    let mut slots = layout.slots().peekable();
    let mut position = 0;
    while sections.peek().is_some() || slots.peek().is_some() {
        let remove_section = match (sections.peek(), slots.peek()) {
            (Some(section), Some(slot)) => section.start < slot.mode,
            (Some(_), None) => true,
            _ => false,
        };
        if remove_section {
            let section = sections.next().unwrap();
            emit(&wasm[position..section.start]);
            position = section.end;
        } else {
            let slot = slots.next().unwrap();
            emit(&wasm[position..slot.mode]);
            emit(&[mode, len]);
            emit(id);
            position = slot.id.end;
        }
    }
    emit(&wasm[position..]);
}

fn write_slots<'a>(
    wasm: &mut [u8],
    slots: impl IntoIterator<Item = &'a Slot>,
    mode: u8,
    id: &[u8],
) -> Result<(), Error> {
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

    // Copying reference retained only in tests to check streaming compatibility.
    struct Stripped {
        wasm: Vec<u8>,
    }

    fn strip_build_id_sections(wasm: &[u8]) -> Result<Stripped, Error> {
        if wasm.len() < WASM_HEADER_LEN || &wasm[..WASM_MAGIC.len()] != WASM_MAGIC {
            return Err(Error::new("input is not a WebAssembly binary"));
        }

        let mut output = Vec::with_capacity(wasm.len());
        output.extend_from_slice(&wasm[..WASM_HEADER_LEN]);

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
                let name_len =
                    read_u32_leb_before(wasm, &mut custom_position, section_end)? as usize;
                let name_end = custom_position
                    .checked_add(name_len)
                    .filter(|end| *end <= section_end)
                    .ok_or_else(|| Error::new("custom section name extends past section end"))?;

                if &wasm[custom_position..name_end] == CUSTOM_SECTION_NAME {
                    is_build_id = true;
                }
            }

            if !is_build_id {
                output.extend_from_slice(&wasm[section_start..section_end]);
            }
            position = section_end;
        }

        Ok(Stripped { wasm: output })
    }

    fn frame() -> Vec<u8> {
        [
            SLOT_PREFIX.as_slice(),
            &[MODE_UNSTAMPED, UNSTAMPED_LEN],
            UNSTAMPED_ID,
            SLOT_SUFFIX,
        ]
        .concat()
    }

    fn data_module(segments: &[(u8, &[u8])]) -> Vec<u8> {
        let mut wasm = b"\0asm\x01\0\0\0\x05\x03\x01\x00\x01".to_vec();
        let mut payload = Vec::new();
        write_u32_leb(segments.len() as u32, &mut payload);
        for &(flags, bytes) in segments {
            payload.push(flags);
            if flags == 2 {
                payload.push(0); // explicit memory index
            }
            if flags != 1 {
                payload.extend_from_slice(&[0x41, 0, 0x0b]); // i32.const 0; end
            }
            write_u32_leb(bytes.len() as u32, &mut payload);
            payload.extend_from_slice(bytes);
        }
        wasm.push(11);
        write_u32_leb(payload.len() as u32, &mut wasm);
        wasm.extend_from_slice(&payload);
        wasm
    }

    fn unstamped(extra: &[u8]) -> Vec<u8> {
        data_module(&[(0, &[extra, &frame()].concat())])
    }

    fn with_custom(mut wasm: Vec<u8>, name: &[u8], data: &[u8]) -> Vec<u8> {
        let mut payload = Vec::new();
        write_u32_leb(name.len() as u32, &mut payload);
        payload.extend_from_slice(name);
        payload.extend_from_slice(data);
        wasm.push(0);
        write_u32_leb(payload.len() as u32, &mut wasm);
        wasm.extend_from_slice(&payload);
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
        assert_ne!(first.ids[0], vec![0; ID_LEN]);
        assert_eq!(check(&first.wasm).unwrap(), first.ids);

        let second = stamp(&first.wasm).unwrap();
        assert_eq!(second, first);
    }

    #[test]
    fn different_input_gets_a_different_id() {
        let first = stamp(&unstamped(b"first")).unwrap();
        let second = stamp(&unstamped(b"second")).unwrap();
        assert_ne!(first.ids, second.ids);
    }

    #[test]
    fn copies_the_top_level_build_id_into_a_core_module() {
        let id = b"linker-build-id!";
        let first = stamp_top_level(&with_build_id(unstamped(b"content"), id)).unwrap();
        assert_eq!(first.ids, vec![id.to_vec()]);
        assert_eq!(check(&first.wasm).unwrap(), first.ids);

        let stripped = strip_build_id_sections(&first.wasm).unwrap();
        let slots = parse_layout(&stripped.wasm)
            .unwrap()
            .slots()
            .cloned()
            .collect::<Vec<_>>();
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
        assert_eq!(stamped.ids, vec![outer_id.to_vec(); 2]);
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.ids);

        let stripped = strip_build_id_sections(&stamped.wasm).unwrap();
        let slots = parse_layout(&stripped.wasm)
            .unwrap()
            .slots()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(slots.len(), 2);
        assert!(slots
            .iter()
            .all(|slot| stripped.wasm[slot.mode] == MODE_TOP_LEVEL));
    }

    #[test]
    fn top_level_mode_replaces_content_provenance() {
        let generated = stamp(&unstamped(b"content")).unwrap();
        let imported = stamp_top_level(&generated.wasm).unwrap();

        assert_eq!(imported.ids, generated.ids);
        assert_eq!(check(&imported.wasm).unwrap(), generated.ids);
        let stripped = strip_build_id_sections(&imported.wasm).unwrap();
        let slots = parse_layout(&stripped.wasm)
            .unwrap()
            .slots()
            .cloned()
            .collect::<Vec<_>>();
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
        let stamped = stamp(&unstamped(b"content")).unwrap().wasm;
        let stamped = with_custom(stamped, b"metadata", b"changed");
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
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.ids);
        let stripped = strip_build_id_sections(&stamped.wasm).unwrap();
        let slots = parse_layout(&stripped.wasm)
            .unwrap()
            .slots()
            .cloned()
            .collect::<Vec<_>>();
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
        let component = component_with_core_module(&unstamped(b"component"));
        let stamped = stamp(&component).unwrap();
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.ids);
    }

    #[test]
    fn module_mode_preserves_distinct_ids_through_nested_components() {
        let a = with_build_id(unstamped(b"first"), b"module-a");
        let b = with_build_id(unstamped(b"second"), b"module-b");
        let nested = component_with_core_module(&b);
        let mut component = component_with_core_module(&a);
        component.push(4); // nested component
        write_u32_leb(nested.len() as u32, &mut component);
        component.extend_from_slice(&nested);
        // An adapter without a slot does not need a build ID.
        component.push(1);
        component.push(8);
        component.extend_from_slice(b"\0asm\x01\0\0\0");
        let component = with_build_id(component, b"unrelated-outer-id");

        let stamped = stamp_modules(&component).unwrap();
        assert_eq!(
            stamped.ids,
            vec![b"module-a".to_vec(), b"module-b".to_vec()]
        );
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.ids);
        assert_eq!(stamp_modules(&stamped.wasm).unwrap(), stamped);

        let layout = parse_layout(&component).unwrap();
        let slots = layout.slots().cloned().collect::<Vec<_>>();
        for (offset, (&before, &after)) in component.iter().zip(&stamped.wasm).enumerate() {
            if !slots
                .iter()
                .any(|slot| (slot.mode..slot.id.end).contains(&offset))
            {
                assert_eq!(before, after, "modified byte outside a slot at {offset}");
            }
        }
        assert_eq!(component.len(), stamped.wasm.len());
        assert!(slots
            .iter()
            .all(|slot| stamped.wasm[slot.mode] == MODE_MODULE));
    }

    #[test]
    fn module_mode_requires_its_own_single_sized_id() {
        let module = unstamped(b"content");
        let component = with_build_id(component_with_core_module(&module), b"outer");
        assert!(stamp_modules(&component)
            .unwrap_err()
            .to_string()
            .contains("core module 0"));
        assert!(stamp_modules(&component)
            .unwrap_err()
            .to_string()
            .contains("--id-source content"));
        let duplicate = with_build_id(with_build_id(module.clone(), b"a"), b"b");
        assert!(stamp_modules(&duplicate)
            .unwrap_err()
            .to_string()
            .contains("found 2"));
        for id in [&[][..], &[7; ID_LEN + 1][..]] {
            assert!(stamp_modules(&with_build_id(module.clone(), id))
                .unwrap_err()
                .to_string()
                .contains("between 1 and 32"));
        }
    }

    #[test]
    fn check_uses_each_modules_id_and_checks_every_slot() {
        let input = with_build_id(data_module(&[(0, &frame()), (1, &frame())]), b"module-id");
        let mut stamped = stamp_modules(&input).unwrap().wasm;
        let slots = parse_layout(&stamped)
            .unwrap()
            .slots()
            .cloned()
            .collect::<Vec<_>>();
        stamped[slots[1].id.start] ^= 1;
        assert!(check(&stamped)
            .unwrap_err()
            .to_string()
            .contains("do not match"));
        stamped[slots[1].id.start] ^= 1;
        stamped[slots[1].mode] = MODE_TOP_LEVEL;
        assert!(check(&stamped)
            .unwrap_err()
            .to_string()
            .contains("mixed modes"));
    }

    #[test]
    fn slot_markers_in_custom_sections_are_not_slots() {
        let custom_only = with_custom(b"\0asm\x01\0\0\0".to_vec(), b"debug", &frame());
        for result in [
            stamp(&custom_only),
            stamp_modules(&custom_only),
            stamp_top_level(&custom_only),
        ] {
            assert!(result.unwrap_err().to_string().contains("slot not found"));
        }
        assert!(check(&custom_only)
            .unwrap_err()
            .to_string()
            .contains("slot not found"));
        let mut component = b"\0asm\x0d\0\x01\0".to_vec();
        component = with_custom(component, b"debug", &frame());
        assert!(stamp(&component)
            .unwrap_err()
            .to_string()
            .contains("slot not found"));
    }

    #[test]
    fn preserves_custom_section_markers_beside_real_data_slots() {
        let original = with_custom(unstamped(b"data"), b"debug", &frame());
        let stamped = stamp(&original).unwrap();
        let section = Parser::new(0)
            .parse_all(&stamped.wasm)
            .find_map(|payload| match payload.unwrap() {
                Payload::CustomSection(section) if section.name() == "debug" => {
                    Some(section.data())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(section, frame());
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.ids);
        // Metadata affects the content ID even if it happens to contain a slot frame.
        let changed = with_custom(unstamped(b"data"), b"debug", &{
            let mut data = frame();
            data[SLOT_PREFIX.len() + 2] ^= 1;
            data
        });
        assert_ne!(stamp(&changed).unwrap().ids, stamped.ids);
    }

    #[test]
    fn handles_active_passive_and_explicit_memory_data_segments() {
        let input = with_build_id(
            data_module(&[(0, &frame()), (1, &frame()), (2, &frame())]),
            b"id",
        );
        let stamped = stamp_modules(&input).unwrap();
        assert_eq!(parse_layout(&stamped.wasm).unwrap().slots().count(), 3);
        assert_eq!(check(&stamped.wasm).unwrap(), vec![b"id".to_vec()]);
    }

    #[test]
    fn does_not_join_separate_data_payloads() {
        let frame = frame();
        let module = data_module(&[(0, &frame[..25]), (1, &frame[25..])]);
        assert!(stamp(&module)
            .unwrap_err()
            .to_string()
            .contains("slot not found"));
    }

    #[test]
    fn rejects_malformed_data_segments_and_nested_modules() {
        let mut module = unstamped(b"data");
        // Corrupt the first segment's byte length without changing section framing.
        // Header (8), memory section (5), data header (2), count (1), flag/expr (4).
        module[20] = 0x7f;
        assert!(stamp(&module).is_err());
        assert!(stamp(&component_with_core_module(&module)).is_err());
        let mut unsupported = unstamped(b"data");
        unsupported[4] = 2;
        assert!(stamp(&unsupported).is_err());
    }

    #[test]
    fn whole_content_mode_is_idempotent_for_multiple_nested_modules() {
        let a = with_build_id(unstamped(b"first"), b"module-a");
        let b = with_build_id(unstamped(b"second"), b"module-b");
        let mut component = component_with_core_module(&a);
        component.push(1);
        write_u32_leb(b.len() as u32, &mut component);
        component.extend_from_slice(&b);
        let stamped = stamp(&component).unwrap();
        assert_eq!(stamped.ids.len(), 2);
        assert_eq!(stamped.ids[0], stamped.ids[1]);
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.ids);
        assert_eq!(stamp(&stamped.wasm).unwrap(), stamped);
        // Switching back to module mode restores the original, distinct IDs.
        assert_eq!(
            stamp_modules(&stamped.wasm).unwrap().ids,
            vec![b"module-a".to_vec(), b"module-b".to_vec()]
        );
    }

    // Independent, deliberately copying implementation of the original
    // canonical hash. It must not share the optimized chunk iterator.
    fn reference_stamp(wasm: &[u8]) -> Stamped {
        let mut output = strip_build_id_sections(wasm).unwrap().wasm;
        let (slots, count) = {
            let layout = parse_layout(&output).unwrap();
            (
                layout.slots().cloned().collect::<Vec<_>>(),
                layout
                    .modules
                    .iter()
                    .filter(|module| !module.slots.is_empty())
                    .count(),
            )
        };
        let mut canonical = output.clone();
        for slot in &slots {
            canonical[slot.mode] = MODE_UNSTAMPED;
            canonical[slot.len] = UNSTAMPED_LEN;
            canonical[slot.id.clone()].copy_from_slice(UNSTAMPED_ID);
        }
        let id: [u8; ID_LEN] = Sha256::digest(canonical).into();
        write_slots(&mut output, &slots, MODE_HASH, &id).unwrap();
        append_build_id_section(&mut output, &id);
        Stamped {
            wasm: output,
            ids: vec![id.to_vec(); count],
        }
    }

    #[test]
    fn streaming_matches_original_with_interleaved_and_padded_sections() {
        let module = with_build_id(unstamped(b"module"), b"module-id");
        let module = with_custom(module, b"debug", &frame());
        let nested = component_with_core_module(&module);
        let mut component = with_build_id(b"\0asm\x0d\0\x01\0".to_vec(), b"first");
        component.push(4);
        write_u32_leb(nested.len() as u32, &mut component);
        component.extend_from_slice(&nested);
        component = with_build_id(component, b"middle");
        component.push(1);
        write_u32_leb(module.len() as u32, &mut component);
        component.extend_from_slice(&module);
        // Valid, non-minimal section-length and name-length LEB encodings.
        component.extend_from_slice(b"\0\x8f\0\x88\0build_id\x04last");
        let stamped = stamp(&component).unwrap();
        assert_eq!(stamped, reference_stamp(&component));
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.ids);
        assert_eq!(stamp(&stamped.wasm).unwrap(), stamped);
    }

    #[test]
    fn streaming_tracks_custom_sections_after_code_and_empty_sections() {
        let data = unstamped(b"data");
        let mut module = data[..8].to_vec();
        module.extend_from_slice(&[1, 4, 1, 0x60, 0, 0]); // () -> ()
        module.extend_from_slice(&[3, 2, 1, 0]); // one function
        module.extend_from_slice(&data[8..13]); // memory
        module.extend_from_slice(&[10, 4, 1, 2, 0, 0x0b]); // function body
        module = with_build_id(module, b"after-code");
        module = with_custom(module, b"empty", b"");
        module.extend_from_slice(&data[13..]); // data
        module = with_build_id(module, b"after-data");
        let stamped = stamp(&module).unwrap();
        assert_eq!(stamped, reference_stamp(&module));
        assert_eq!(check(&stamped.wasm).unwrap(), stamped.ids);
    }

    #[test]
    fn streaming_matches_reference_over_varied_payload_and_section_sizes() {
        for size in [0, 1, 63, 127, 128, 16383, 16384, 65536] {
            let module = unstamped(&vec![0xa5; size]);
            let component = component_with_core_module(&module);
            let component = with_custom(component, b"debug", &vec![0x5a; size]);
            let component = with_build_id(component, b"outer");
            assert_eq!(stamp(&component).unwrap(), reference_stamp(&component));
        }
        let prefix_rich = unstamped(&vec![SLOT_PREFIX[0]; 65536]);
        assert_eq!(stamp(&prefix_rich).unwrap(), reference_stamp(&prefix_rich));
    }

    #[test]
    fn ambiguous_overlapping_frames_return_an_error() {
        let mut data = vec![0xa5; 84];
        data[..16].copy_from_slice(SLOT_PREFIX);
        data[18..34].copy_from_slice(SLOT_PREFIX);
        data[50..66].copy_from_slice(SLOT_SUFFIX);
        data[68..84].copy_from_slice(SLOT_SUFFIX);
        let wasm = with_build_id(data_module(&[(0, &data)]), b"id");
        for result in [stamp(&wasm), stamp_modules(&wasm), stamp_top_level(&wasm)] {
            assert!(result.unwrap_err().to_string().contains("overlapping"));
        }
        assert!(check(&wasm)
            .unwrap_err()
            .to_string()
            .contains("overlapping"));
    }
}
