//! Linus store: read-only dpkg inventory and APT previews.
use super::query::{self, Cancellation};

fn run(program: &str, args: &[&str], cancel: &Cancellation) -> String {
    query::text(program, args, cancel)
        .unwrap_or_else(|error| format!("Package query failed: {error}"))
}

pub fn valid_package(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"+.-".contains(&c))
}

fn inventory(cancel: &Cancellation) -> Result<Vec<(u64, String, String)>, String> {
    let result = query::text(
        "/usr/bin/dpkg-query",
        &[
            "-W",
            "-f=${db:Status-Abbrev}\t${Installed-Size}\t${binary:Package}\t${Version}\n",
        ],
        cancel,
    )?;
    parse_inventory(&result)
}

fn parse_inventory(result: &str) -> Result<Vec<(u64, String, String)>, String> {
    let mut packages = Vec::new();
    for line in result.lines() {
        let mut columns = line.split('\t');
        if !columns
            .next()
            .is_some_and(|status| status.starts_with("ii"))
        {
            continue;
        }
        let invalid = || "Installed package inventory is incomplete or invalid".to_owned();
        let size = columns
            .next()
            .ok_or_else(invalid)?
            .parse::<u64>()
            .map_err(|_| invalid())?;
        let name = columns
            .next()
            .filter(|name| !name.is_empty())
            .ok_or_else(invalid)?
            .to_owned();
        let version = columns
            .next()
            .filter(|version| !version.is_empty())
            .ok_or_else(invalid)?
            .to_owned();
        packages.push((size, name, version));
    }
    packages.sort_unstable_by_key(|item| std::cmp::Reverse(item.0));
    Ok(packages)
}

pub fn overview(cancel: &Cancellation) -> String {
    let packages = match inventory(cancel) {
        Ok(packages) => packages,
        Err(error) => {
            return format!(
                "Linus store\n\nInventory unavailable: {error}\nNo package count or size could be established."
            );
        }
    };
    let total_kib: u64 = packages.iter().map(|item| item.0).sum();
    let mut out = format!(
        "Linus store  /  Installed\n\nPackages: {}    Estimated installed size: {} MiB\n\nLARGEST PACKAGES\n",
        packages.len(),
        total_kib / 1024
    );
    for (size, name, version) in packages.iter().take(25) {
        out.push_str(&format!("{name:<36} {:>5} MiB  {version}\n", size / 1024));
    }
    out.push_str("\n/store installed       Installed package inventory\n/store search QUERY    Find packages\n/store info PACKAGE    Package details\n/store install PACKAGE  Simulate installation\n/store remove PACKAGE   Simulate removal\n\nAll Store actions are currently read-only. Installation needs admin authorization.");
    out
}

pub fn installed(cancel: &Cancellation) -> String {
    let packages = match inventory(cancel) {
        Ok(packages) => packages,
        Err(error) => return format!("Inventory unavailable: {error}"),
    };
    let mut out = format!("Linus store  /  Installed packages: {}\n\n", packages.len());
    for (size, name, version) in packages.iter().take(100) {
        out.push_str(&format!("{name:<36} {:>8} KiB  {version}\n", size));
    }
    out.push_str("\nLargest 100 packages shown. Sizes are installed-size metadata.");
    out
}

pub fn search(query: &str, cancel: &Cancellation) -> String {
    if query.is_empty() || query.len() > 128 || query.chars().any(char::is_control) {
        return "Usage: /store search QUERY".into();
    }
    let results = run(
        "/usr/bin/apt-cache",
        &["search", "--names-only", query],
        cancel,
    );
    let results = if results.trim().is_empty() {
        "No matching packages in the local cache. If repository lists have not been downloaded, use sudo apt-get update in native Shell when online.".to_owned()
    } else {
        results.lines().take(70).collect::<Vec<_>>().join("\n")
    };
    format!("Linus store / Search: {query}\n\n{results}\n\nRead-only repository query.")
}

pub fn details(package: &str, cancel: &Cancellation) -> String {
    if !valid_package(package) {
        return "Invalid package identifier.".into();
    }
    let installed = run(
        "/usr/bin/dpkg-query",
        &[
            "-W",
            "-f=Installed-size: ${Installed-Size} KiB\nVersion: ${Version}\n",
            package,
        ],
        cancel,
    );
    let available = run("/usr/bin/apt-cache", &["show", package], cancel);
    format!(
        "Linus store / {package}\n\nINSTALLED\n{installed}\n\nREPOSITORY\n{}\n\nRead-only details.",
        available.lines().take(30).collect::<Vec<_>>().join("\n")
    )
}

pub fn preview(operation: &str, package: &str, cancel: &Cancellation) -> String {
    if !valid_package(package) || (operation != "install" && operation != "remove") {
        return "Invalid package operation.".into();
    }
    let output = run(
        "/usr/bin/apt-get",
        &["--simulate", operation, "--", package],
        cancel,
    );
    format!(
        "Linus store / {operation} {package}\n\n{output}\n\nPREVIEW ONLY. No system changes.\nTo apply in native Shell (Ctrl+T): sudo apt-get {operation} {package}\nA managed confirmation and transaction UI is not implemented yet."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn apt_identifiers_are_checked() {
        assert!(valid_package("git"));
        assert!(valid_package("libc6-dev"));
        assert!(!valid_package("../etc"));
        assert!(!valid_package("git;id"));
        assert!(!valid_package("--help"));
    }

    #[test]
    fn inventory_is_complete_and_failures_are_not_zero_packages() {
        let rows: String = (0..1000)
            .map(|i| format!("ii \t123\tpackage-{i}\t1.0\n"))
            .collect();
        assert!(rows.len() > 16_000);
        assert_eq!(parse_inventory(&rows).unwrap().len(), 1000);
        assert!(parse_inventory("ii \t123\tcut-off").is_err());
        let cancel = Cancellation::default();
        cancel.cancel();
        let display = overview(&cancel);
        assert!(display.contains("Inventory unavailable"));
        assert!(!display.contains("Packages: 0"));
    }
}
