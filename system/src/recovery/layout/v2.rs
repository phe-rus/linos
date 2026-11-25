//! Layout 2 geometry only. No device access, formatting or boot publication.
use std::fmt;

const MIB: u64 = 1024 * 1024;
pub const MIN_DISK_BYTES: u64 = 32 * 1024 * MIB;
const RESERVED_BYTES: u64 = 16 * MIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Partition {
    pub name: &'static str,
    pub first_lba: u64,
    pub last_lba: u64,
    pub gpt_type: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geometry {
    pub sector_bytes: u64,
    pub partitions: [Partition; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    UnsupportedSectorSize,
    IncompleteSector,
    BelowMinimum,
    InvalidGeometry,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedSectorSize => {
                "Layout 2 planning supports 512 or 4096 byte logical sectors"
            }
            Self::IncompleteSector => "Disk capacity must contain complete logical sectors",
            Self::BelowMinimum => "Layout 2 needs at least 32 GiB for development virtual disks",
            Self::InvalidGeometry => "Layout 2 geometry cannot fit this disk",
        })
    }
}

impl std::error::Error for Error {}

/// Derive inclusive GPT sector ranges from the accepted byte sizes.
/// Reserve 1 MiB before EFI and 15 MiB after Linos, plus any partial MiB tail.
/// Sector support here is a geometry property, not installation qualification.
pub fn server(disk_bytes: u64, sector_bytes: u64) -> Result<Geometry, Error> {
    if !matches!(sector_bytes, 512 | 4096) {
        return Err(Error::UnsupportedSectorSize);
    }
    if !disk_bytes.is_multiple_of(sector_bytes) {
        return Err(Error::IncompleteSector);
    }
    if disk_bytes < MIN_DISK_BYTES {
        return Err(Error::BelowMinimum);
    }

    let aligned_bytes = disk_bytes / MIB * MIB;
    let linos_end = aligned_bytes
        .checked_sub(RESERVED_BYTES - MIB)
        .ok_or(Error::InvalidGeometry)?;
    let efi_start = MIB;
    let recovery_start = efi_start + 1024 * MIB;
    let linos_start = recovery_start + 4096 * MIB;
    if linos_end <= linos_start {
        return Err(Error::InvalidGeometry);
    }
    let partition = |name, start, end, gpt_type| Partition {
        name,
        first_lba: start / sector_bytes,
        last_lba: end / sector_bytes - 1,
        gpt_type,
    };
    Ok(Geometry {
        sector_bytes,
        partitions: [
            partition("EFI", efi_start, recovery_start, "ef00"),
            partition("Recovery", recovery_start, linos_start, "8300"),
            partition("Linos", linos_start, linos_end, "8300"),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_undersized_partial_and_unsupported_sectors() {
        assert_eq!(server(MIN_DISK_BYTES - 512, 512), Err(Error::BelowMinimum));
        assert_eq!(
            server(MIN_DISK_BYTES + 1, 512),
            Err(Error::IncompleteSector)
        );
        for sector in [0, 1, 511, 520, 1024, 8192, u64::MAX] {
            assert_eq!(
                server(MIN_DISK_BYTES, sector),
                Err(Error::UnsupportedSectorSize)
            );
        }
    }

    #[test]
    fn minimum_layout_is_aligned_disjoint_and_reserves_gpt_space() {
        for sector in [512, 4096] {
            let geometry = server(MIN_DISK_BYTES, sector).unwrap();
            let p = geometry.partitions;
            assert_eq!(p.map(|part| part.name), ["EFI", "Recovery", "Linos"]);
            assert_eq!(p.map(|part| part.gpt_type), ["ef00", "8300", "8300"]);
            assert_eq!(p[0].first_lba * sector, MIB);
            assert_eq!((p[0].last_lba + 1 - p[0].first_lba) * sector, 1024 * MIB);
            assert_eq!((p[1].last_lba + 1 - p[1].first_lba) * sector, 4096 * MIB);
            for (i, part) in p.iter().enumerate() {
                assert!(part.last_lba >= part.first_lba);
                assert_eq!(part.first_lba * sector % MIB, 0);
                assert_eq!((part.last_lba + 1) * sector % MIB, 0);
                if i > 0 {
                    assert_eq!(p[i - 1].last_lba + 1, part.first_lba);
                }
            }
            let used = p
                .iter()
                .map(|part| (part.last_lba + 1 - part.first_lba) * sector)
                .sum::<u64>();
            assert_eq!(MIN_DISK_BYTES - used, RESERVED_BYTES);
            assert!(p[2].last_lba < MIN_DISK_BYTES / sector - 1);
        }
    }

    #[test]
    fn growth_changes_only_linos_and_rounds_down_a_partial_mib() {
        let small = server(MIN_DISK_BYTES, 512).unwrap();
        let large = server(64 * 1024 * MIB, 512).unwrap();
        assert_eq!(small.partitions[..2], large.partitions[..2]);
        assert_eq!(small.partitions[2].first_lba, large.partitions[2].first_lba);
        assert_eq!(
            (large.partitions[2].last_lba - small.partitions[2].last_lba) * 512,
            MIN_DISK_BYTES
        );
        assert_eq!(server(MIN_DISK_BYTES + 512, 512).unwrap(), small);
    }

    #[test]
    fn largest_sector_aligned_capacity_does_not_overflow() {
        for sector in [512, 4096] {
            let bytes = u64::MAX / sector * sector;
            let geometry = server(bytes, sector).unwrap();
            let last = geometry.partitions[2].last_lba;
            assert!((last + 1) * sector < bytes);
            assert!(bytes - (last + 1) * sector >= 15 * MIB);
        }
    }
}
