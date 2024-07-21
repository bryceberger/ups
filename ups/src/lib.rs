use std::ops::ControlFlow;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Missing 'UPS1' header at start of patch")]
    MissingHeader,
    #[error("Input patch malformed")]
    MalformedPatch,
    #[error("CRC mismatch in original file")]
    CrcMismatchOriginal,
    #[error("CRC mismatch in patch file")]
    CrcMismatchPatch,
    #[error("CRC mismatch in output file")]
    CrcMismatchTarget,
}

#[derive(Default)]
pub struct Options {
    pub skip_crc: bool,
}

pub fn apply_patch(source: Vec<u8>, patch: &[u8]) -> Result<Vec<u8>, Error> {
    apply_patch_with(Default::default(), source, patch)
}

pub fn apply_patch_with(options: Options, source: Vec<u8>, patch: &[u8]) -> Result<Vec<u8>, Error> {
    let (p, it) = parse_patch(patch)?;

    if !options.skip_crc {
        verify_crc(&source, p.source_crc).map_err(|_| Error::CrcMismatchOriginal)?;
        let patch_crc_data = &patch[..patch.len() - 4];
        verify_crc(patch_crc_data, p.patch_crc).map_err(|_| Error::CrcMismatchPatch)?;
    }

    let mut target = source;
    target.resize(p.source_size.max(p.target_size) as _, 0);

    it.fold(0, |write_offset, it| {
        xor_slice(&mut target[write_offset + it.take..], it.xor);
        write_offset + it.take + it.xor.len()
    });

    if !options.skip_crc {
        verify_crc(&target, p.target_crc).map_err(|_| Error::CrcMismatchTarget)?;
    }

    Ok(target)
}

pub struct UpsPatch {
    pub source_size: usize,
    pub target_size: usize,
    pub source_crc: u32,
    pub target_crc: u32,
    pub patch_crc: u32,
}

pub fn parse_patch(patch: &[u8]) -> Result<(UpsPatch, UpsSectionIter<'_>), Error> {
    let Some(b"UPS1") = patch.get(..4) else {
        return Err(Error::MissingHeader);
    };

    let (s_used, source_size) = read_vuint(&patch[4..]).ok_or(Error::MalformedPatch)?;
    let (t_used, target_size) = read_vuint(&patch[4 + s_used..]).ok_or(Error::MalformedPatch)?;
    let offset = 4 + s_used + t_used;

    if patch.len() < offset + 12 {
        return Err(Error::MalformedPatch);
    }

    let get_crc = |o| u32::from_le_bytes(patch[o..o + 4].try_into().unwrap());
    let ups_patch = UpsPatch {
        source_size,
        target_size,
        source_crc: get_crc(patch.len() - 12),
        target_crc: get_crc(patch.len() - 8),
        patch_crc: get_crc(patch.len() - 4),
    };
    let it = UpsSectionIter::new(&patch[offset..patch.len() - 12]);
    Ok((ups_patch, it))
}

/// -> (consumed bytes, value)
fn read_vuint(input: &[u8]) -> Option<(usize, usize)> {
    let val = input.iter().enumerate().try_fold(0, |acc, (idx, x)| {
        let shift = idx * 7;
        if x & 0x80 != 0 {
            ControlFlow::Break((idx + 1, acc + ((*x as usize & 0x7f) << shift)))
        } else {
            ControlFlow::Continue(acc + ((*x as usize | 0x80) << shift))
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

fn xor_slice(left: &mut [u8], right: &[u8]) {
    left.iter_mut().zip(right).for_each(|(l, r)| *l ^= r);
}

pub struct UpsSectionIter<'d> {
    data: &'d [u8],
    offset: usize,
}

pub struct UpsSection<'d> {
    /// Number of bytes to take directly from the source.
    pub take: usize,
    /// Take `xor.len()` from the source, then elementwise xor.
    pub xor: &'d [u8],
}

impl<'d> UpsSectionIter<'d> {
    /// data _without_ header and footer (i.e. no "UPS1", no file size, no crc)
    const fn new(data: &'d [u8]) -> Self {
        Self { data, offset: 0 }
    }
}

impl<'d> Iterator for UpsSectionIter<'d> {
    type Item = UpsSection<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        let Self { data, offset } = self;
        let (consumed, take) = read_vuint(&data[*offset..])?;
        *offset += consumed;

        let begin = *offset;
        while *offset < data.len() {
            *offset += 1;
            if data[*offset - 1] == 0 {
                let xor = &data[begin..*offset];
                return Some(UpsSection { take, xor });
            }
        }

        None
    }
}
