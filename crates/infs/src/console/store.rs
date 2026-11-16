//! Linus store: read-only dpkg inventory and APT previews.
use std::process::Command;

fn run(program: &str, args: &[&str]) -> String {
    match Command::new(program).args(args).output() {
        Ok(result) => {
            let bytes = if result.status.success() {
                &result.stdout
            } else {
                &result.stderr
            };
            String::from_utf8_lossy(bytes)
                .chars()
                .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
                .take(16_000)
                .collect()
        }
        Err(error) => format!("Package query failed: {error}"),
    }
}

pub fn valid_package(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"+.-".contains(&c))
}

fn inventory() -> Vec<(u64, String, String)> {
    let result = run(
        "/usr/bin/dpkg-query",
        &[
            "-W",
            "-f=${db:Status-Abbrev}\t${Installed-Size}\t${binary:Package}\t${Version}\n",
        ],
    );
    let mut packages: Vec<_> = result
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let status = columns.next()?;
            let size = columns.next()?.parse::<u64>().ok()?;
            let name = columns.next()?.to_owned();
            let version = columns.next()?.to_owned();
            status.starts_with("ii").then_some((size, name, version))
        })
        .collect();
    packages.sort_unstable_by_key(|item| std::cmp::Reverse(item.0));
    packages
}

pub fn overview() -> String {
    let packages = inventory();
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

pub fn installed() -> String {
    let packages = inventory();
    let mut out = format!("Linus store  /  Installed packages: {}\n\n", packages.len());
    for (size, name, version) in packages.iter().take(100) {
        out.push_str(&format!("{name:<36} {:>8} KiB  {version}\n", size));
    }
    out.push_str("\nLargest 100 packages shown. Sizes are installed-size metadata.");
    out
}

pub fn search(query: &str) -> String {
    if query.is_empty() || query.len() > 128 || query.chars().any(char::is_control) {
        return "Usage: /store search QUERY".into();
    }
    let results = run("/usr/bin/apt-cache", &["search", "--names-only", query]);
    format!(
        "Linus store / Search: {query}\n\n{}\n\nRead-only repository query.",
        results.lines().take(70).collect::<Vec<_>>().join("\n")
    )
}

pub fn details(package: &str) -> String {
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
    );
    let available = run("/usr/bin/apt-cache", &["show", package]);
    format!(
        "Linus store / {package}\n\nINSTALLED\n{installed}\n\nREPOSITORY\n{}\n\nRead-only details.",
        available.lines().take(30).collect::<Vec<_>>().join("\n")
    )
}

pub fn preview(operation: &str, package: &str) -> String {
    if !valid_package(package) || (operation != "install" && operation != "remove") {
        return "Invalid package operation.".into();
    }
    let output = run(
        "/usr/bin/apt-get",
        &["--simulate", operation, "--", package],
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
}
