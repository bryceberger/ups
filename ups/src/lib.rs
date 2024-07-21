use std::ops::ControlFlow;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Missing 'UPS1' header at start of patch")]
    MissingHeader,
    #[error("Input patch malformed")]
    MalformedPatch,
    #[error("CRC mismatch ({0:?})")]
    CrcMismatch(CrcKind),
}

#[derive(Debug)]
pub enum CrcKind {
    Original,
    Patch,
    Combined,
}

#[derive(Default)]
pub struct Options {
    pub skip_crc: bool,
}

pub fn apply_patch(source: Vec<u8>, patch: &[u8]) -> Result<Vec<u8>, Error> {
    apply_patch_with(Default::default(), source, patch)
}

pub fn apply_patch_with(
    options: Options,
    mut source: Vec<u8>,
    patch: &[u8],
) -> Result<Vec<u8>, Error> {
    let (p, mut patch_offset) = parse_patch(patch)?;

    if !options.skip_crc {
        verify_crc(&source, p.source_crc).map_err(|_| Error::CrcMismatch(CrcKind::Original))?;
        verify_crc(&patch[..patch.len() - 4], p.patch_crc)
            .map_err(|_| Error::CrcMismatch(CrcKind::Patch))?;
    }

    source.resize(p.source_size.max(p.target_size) as _, 0);

    let mut write_offset = 0;
    while patch_offset < patch.len() - 12 {
        let (consumed, skip_len) =
            read_vuint(&patch[patch_offset..]).ok_or(Error::MalformedPatch)?;
        patch_offset += consumed;
        write_offset += skip_len as usize;

        let w = write_offset;
        for x in &patch[patch_offset..] {
            source[write_offset] ^= x;
            write_offset += 1;
            if *x == 0 {
                break;
            }
        }
        patch_offset += write_offset - w;
    }

    if !options.skip_crc {
        verify_crc(&source, p.target_crc).map_err(|_| Error::CrcMismatch(CrcKind::Combined))?;
    }

    Ok(source)
}

struct UpsPatch {
    source_size: u64,
    target_size: u64,
    source_crc: u32,
    target_crc: u32,
    patch_crc: u32,
}

/// -> (patch, consumed)
fn parse_patch(patch: &[u8]) -> Result<(UpsPatch, usize), Error> {
    let Some(b"UPS1") = patch.get(..4) else {
        return Err(Error::MissingHeader);
    };

    let mut offset = 4;
    let (consumed, source_size) = read_vuint(&patch[offset..]).ok_or(Error::MalformedPatch)?;
    offset += consumed;
    let (consumed, target_size) = read_vuint(&patch[offset..]).ok_or(Error::MalformedPatch)?;
    offset += consumed;

    if patch.len() < offset + 12 {
        return Err(Error::MalformedPatch);
    }

    let get_crc = |o| u32::from_le_bytes(patch[o..o + 4].try_into().unwrap());
    let crc_offset = patch.len() - 12;
    let source_crc = get_crc(crc_offset);
    let target_crc = get_crc(crc_offset + 4);
    let patch_crc = get_crc(crc_offset + 8);
    let ups_patch = UpsPatch {
        source_size,
        target_size,
        source_crc,
        target_crc,
        patch_crc,
    };
    Ok((ups_patch, offset))
}

/// -> (consumed bytes, value)
fn read_vuint(input: &[u8]) -> Option<(usize, u64)> {
    let val = input.iter().enumerate().try_fold(0, |acc, (idx, x)| {
        if x & 0x80 != 0 {
            ControlFlow::Break((idx + 1, acc + ((*x as u64 & 0x7f) << idx * 7)))
        } else {
            ControlFlow::Continue(acc + ((*x as u64 | 0x80) << idx * 7))
        }
    });
    match val {
        ControlFlow::Continue(_) => None,
        ControlFlow::Break(x) => Some(x),
    }
}

fn verify_crc(data: &[u8], expected: u32) -> Result<(), ()> {
    const ALG: crc::Algorithm<u32> = crc::CRC_32_ISO_HDLC;
    (crc::Crc::<u32>::new(&ALG).checksum(data) == expected)
        .then_some(())
        .ok_or(())
}
