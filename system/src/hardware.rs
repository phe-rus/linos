use std::{env, fs, path::Path};

#[derive(Debug)]
struct Hardware {
    architecture: String,
    kernel: String,
    model: Option<String>,
    firmware: String,
    memory_kib: Option<u64>,
    cpu_count: usize,
}

impl Hardware {
    fn detect() -> Self {
        Self {
            architecture: env::consts::ARCH.to_owned(),
            kernel: read_trimmed("/proc/sys/kernel/osrelease").unwrap_or_else(|| "unknown".into()),
            model: device_tree_model(),
            firmware: detect_firmware(),
            memory_kib: total_memory(),
            cpu_count: std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(1),
        }
    }
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim_matches(['\0', '\n', ' ']).to_owned())
        .filter(|value| !value.is_empty())
}

fn device_tree_model() -> Option<String> {
    let data = fs::read("/proc/device-tree/model").ok()?;

    let model = String::from_utf8_lossy(&data);

    Some(model.trim_matches('\0').to_owned())
}

fn detect_firmware() -> String {
    if Path::new("/sys/firmware/efi").exists() {
        "UEFI".into()
    } else if Path::new("/proc/device-tree").exists() {
        "Device Tree / platform firmware".into()
    } else if Path::new("/sys/firmware/acpi").exists() {
        "ACPI".into()
    } else {
        "Unknown".into()
    }
}

fn total_memory() -> Option<u64> {
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;

    meminfo.lines().find_map(|line| {
        let value = line.strip_prefix("MemTotal:")?;
        value.split_whitespace().next()?.parse().ok()
    })
}

fn main() {
    let hardware = Hardware::detect();

    println!("Linos Linux | Hardware Discovery");
    println!("---------------------------------");
    println!("Architecture : {}", hardware.architecture);
    println!("Kernel       : {}", hardware.kernel);
    println!(
        "Model        : {}",
        hardware.model.as_deref().unwrap_or("Not reported")
    );
    println!("Firmware     : {}", hardware.firmware);
    println!("Logical CPUs : {}", hardware.cpu_count);

    if let Some(memory) = hardware.memory_kib {
        println!("Memory       : {} MiB", memory / 1024);
    }
}
