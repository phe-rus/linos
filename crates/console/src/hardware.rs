//! Read-only hardware identification using Linux sysfs and DMI.
use std::{fs, path::Path};

fn read(path: &str) -> Option<String> {
    let result = fs::read_to_string(path).ok()?;
    let value = result.trim_matches(['\0', '\n', '\r', ' ']).to_owned();
    (!value.is_empty() && !value.eq_ignore_ascii_case("none")).then_some(value)
}

fn field(path: &str) -> String {
    read(path).unwrap_or_else(|| "Unknown".into())
}

fn classify(vendor: &str, model: &str, battery: bool) -> &'static str {
    let info = format!("{vendor} {model}").to_ascii_lowercase();
    if info.contains("qemu") || info.contains("kvm") || info.contains("vmware")
        || info.contains("virtualbox") || info.contains("virtual machine") {
        "Virtual machine"
    } else if info.contains("raspberry pi") || info.contains("orange pi")
        || info.contains("rockchip") || info.contains("radxa") {
        "Single-board computer"
    } else if battery {
        "Laptop or portable computer"
    } else if info.contains("server") || info.contains("poweredge")
        || info.contains("proliant") || info.contains("thinksystem") {
        "Physical server"
    } else {
        "Physical computer (type unconfirmed)"
    }
}

pub fn report() -> String {
    let vendor = read("/sys/class/dmi/id/sys_vendor")
        .or_else(|| read("/proc/device-tree/vendor"))
        .unwrap_or_else(|| "Unknown".into());
    let model = read("/sys/class/dmi/id/product_name")
        .or_else(|| read("/proc/device-tree/model"))
        .unwrap_or_else(|| "Unknown".into());
    let battery = Path::new("/sys/class/power_supply/BAT0").exists()
        || Path::new("/sys/class/power_supply/BAT1").exists();
    let kind = classify(&vendor, &model, battery);
    let architecture = std::env::consts::ARCH;
    let cpu = read("/proc/cpuinfo")
        .and_then(|s| s.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "model name" || key.trim() == "Hardware")
                .then(|| value.trim().to_owned())
        })).unwrap_or_else(|| "Not reported".into());
    let processors = fs::read_to_string("/proc/stat")
        .map(|s| s.lines().filter(|line| {
            line.strip_prefix("cpu").is_some_and(|rest| {
                rest.chars().next().is_some_and(|c| c.is_ascii_digit())
            })
        }).count()).unwrap_or(0);
    let memory = read("/proc/meminfo")
        .and_then(|s| s.lines().find(|line| line.starts_with("MemTotal:"))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|num| num.parse::<u64>().ok())
        ).map(|size| format!("{} MiB", size / 1024))
        .unwrap_or_else(|| "Unknown".into());
    let firmware = field("/sys/class/dmi/id/bios_version");
    let kernel = read("/proc/sys/kernel/osrelease").unwrap_or_else(|| "Unknown".into());
    format!(
        "Hardware\n\nType           {kind}\nVendor         {vendor}\nModel          {model}\nArchitecture   {architecture}\nCPU            {cpu}\nLogical CPUs   {processors}\nMemory         {memory}\nFirmware       {firmware}\nLinux kernel   {kernel}\nBattery        {}\n\nDetection uses Linux-reported properties. Unknown hardware is never assumed to be a server.\nResource balancing policies are not yet active.",
        if battery { "Detected" } else { "Not detected" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_classification_is_conservative() {
        assert_eq!(classify("QEMU", "Standard PC", false), "Virtual machine");
        assert_eq!(classify("Unknown", "Raspberry Pi 5", false), "Single-board computer");
        assert_eq!(classify("Dell", "Latitude", true), "Laptop or portable computer");
        assert_eq!(classify("Unknown", "Unknown", false), "Physical computer (type unconfirmed)");
    }
}
