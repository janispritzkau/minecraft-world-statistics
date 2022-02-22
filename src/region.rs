use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
};

use byteorder::{ReadBytesExt, BE};
use quartz_nbt::NbtCompound;

pub struct RegionFile {
    file: File,
    offsets: [u32; 1024],
}

impl RegionFile {
    pub fn new(mut file: File) -> Result<Self, io::Error> {
        let mut header_buf = [0; 8192];
        file.read_exact(&mut header_buf)?;

        let mut offsets = [0; 1024];
        for i in 0..1024 {
            offsets[i] = u32::from_be_bytes(header_buf[i * 4..][..4].try_into().unwrap());
        }

        Ok(RegionFile { file, offsets })
    }

    pub fn for_each_chunk(
        &mut self,
        mut func: impl FnMut((usize, &[u8])),
    ) -> Result<(), io::Error> {
        let mut offsets = self.offsets.clone();
        let mut indices: Vec<usize> = (0..1024).collect();
        indices.sort_by_key(|&i| offsets[i]);
        offsets.sort();

        let mut buf = Vec::new();

        let mut i = 0;
        let mut last_sector = 2;
        while i < 1024 {
            if offsets[i] == 0 {
                i += 1;
                continue;
            }

            let sector_start = offsets[i] >> 8;
            let mut j = i + 1;

            while j < 1024 {
                let empty = (offsets[j] >> 8) - ((offsets[j - 1] >> 8) + (offsets[j - 1] & 0xff));
                let sector_count = (offsets[j] >> 8) + (offsets[j] & 0xff) - sector_start;
                if sector_count + empty > 16 {
                    break;
                }
                j += 1;
            }

            let sector_end = (offsets[j - 1] >> 8) + (offsets[j - 1] & 0xff);
            let len = (sector_end - sector_start) as u64 * 4096;

            let sector_offset = sector_start - last_sector;
            if sector_offset > 0 {
                self.file
                    .seek(SeekFrom::Current(sector_offset as i64 * 4096))?;
            }
            last_sector = sector_end;

            buf.clear();
            buf.reserve(len as usize);
            self.file.by_ref().take(len).read_to_end(&mut buf)?;

            for i in i..j {
                let start = ((offsets[i] >> 8) - sector_start) as usize * 4096;
                let mut buf = &buf[start..];
                let len = buf.read_u32::<BE>()? as usize;
                func((indices[i], &buf[..len]));
            }

            i = j;
        }

        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ChunkError {
    #[error("invalid compression type {0}")]
    InvalidCompressionType(u8),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    NbtIo(#[from] quartz_nbt::io::NbtIoError),
}

pub fn read_chunk(mut buf: &[u8]) -> Result<NbtCompound, ChunkError> {
    let compression_type = buf.read_u8()?;
    Ok(quartz_nbt::io::read_nbt(
        &mut buf,
        match compression_type {
            0 => quartz_nbt::io::Flavor::Uncompressed,
            1 => quartz_nbt::io::Flavor::GzCompressed,
            2 => quartz_nbt::io::Flavor::ZlibCompressed,
            t => return Err(ChunkError::InvalidCompressionType(t)),
        },
    )?
    .0)
}
