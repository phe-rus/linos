pub struct PartitionPlan {
    pub label: &'static str,
    pub size_mib: u64,
}

pub fn server(disk_mib: u64) -> Option<Vec<PartitionPlan>> {
    const ESP_MIB: u64 = 512;
    const RECOVERY_MIB: u64 = 2048;
    const SYSTEM_MIB: u64 = 4096;
    const MIN_DISK_MIB: u64 = 16 * 1024;
    // Reserve space for GPT metadata, alignment and filesystem constraints.
    const RESERVED_MIB: u64 = 16;

    if disk_mib < MIN_DISK_MIB {
        return None;
    }

    let fixed = ESP_MIB + RECOVERY_MIB + 2 * SYSTEM_MIB + RESERVED_MIB;
    let data = disk_mib.checked_sub(fixed)?;
    Some(vec![
        PartitionPlan {
            label: "Linos EFI",
            size_mib: ESP_MIB,
        },
        PartitionPlan {
            label: "Linos Recovery",
            size_mib: RECOVERY_MIB,
        },
        PartitionPlan {
            label: "Linos System A",
            size_mib: SYSTEM_MIB,
        },
        PartitionPlan {
            label: "Linos System B",
            size_mib: SYSTEM_MIB,
        },
        PartitionPlan {
            label: "Linos Data (remainder)",
            size_mib: data,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_disks_below_provisional_minimum() {
        assert!(server(16 * 1024 - 1).is_none());
    }

    #[test]
    fn preserves_space_for_data_and_alignment() {
        let plan = server(16 * 1024).expect("16 GiB layout");
        assert_eq!(plan.len(), 5);
        assert_eq!(plan[0].size_mib, 512);
        assert_eq!(plan[1].size_mib, 2048);
        assert_eq!(plan[2].size_mib, 4096);
        assert_eq!(plan[3].size_mib, 4096);
        assert!(plan[4].size_mib > 1024);
        let reserved = 16 * 1024 - plan.iter().map(|p| p.size_mib).sum::<u64>();
        assert_eq!(reserved, 16);
    }

    #[test]
    fn larger_disk_grows_only_data_partition() {
        let small = server(16 * 1024).unwrap();
        let large = server(64 * 1024).unwrap();
        assert_eq!(small[2].size_mib, large[2].size_mib);
        assert_eq!(large[4].size_mib - small[4].size_mib, 48 * 1024);
    }
}
