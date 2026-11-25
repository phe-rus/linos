//! Required system account contracts and native database cross references.
//! No account changes, password logging or credential restoration.
use super::{Fields, LIMIT};
use crate::recovery::records::{invalid, state::decimal};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

const MAX_IDS: u64 = u32::MAX as u64 - 1;
const MAX_ACCOUNTS: u64 = 10_000;

fn name(value: &str) -> io::Result<String> {
    if value.len() > 32
        || !value
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_lowercase() || *b == b'_')
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
    {
        return Err(invalid("Unsupported native account or group name"));
    }
    Ok(value.into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub gid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub name: String,
    pub uid: u32,
    pub primary: Group,
    pub supplementary: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contract {
    accounts: Vec<Requirement>,
}

impl Contract {
    /// Baseline generation is restricted to an unclaimed image containing only
    /// system identities. Enrolled owner databases are never image inputs.
    pub fn baseline(current: &NativeAccounts) -> io::Result<Self> {
        if current.has_unlocked_password {
            return Err(invalid("Baseline contains an unlocked account credential"));
        }
        let mut accounts = Vec::new();
        for (name, user) in &current.users {
            if user.uid >= 1000 && user.uid != 65534 {
                return Err(invalid("Baseline contains a non-system account"));
            }
            let primary = current
                .groups
                .iter()
                .find(|(_, group)| group.gid == user.gid)
                .ok_or_else(|| invalid("Missing primary group"))?;
            let supplementary: Vec<_> = current
                .groups
                .iter()
                .filter(|(_, group)| group.gid != user.gid && group.members.contains(name))
                .map(|(name, group)| Group {
                    name: name.clone(),
                    gid: group.gid,
                })
                .collect();
            if supplementary.len() > 128 {
                return Err(invalid(
                    "Baseline supplementary memberships exceed their bound",
                ));
            }
            accounts.push(Requirement {
                name: name.clone(),
                uid: user.uid,
                primary: Group {
                    name: primary.0.clone(),
                    gid: user.gid,
                },
                supplementary,
            });
        }
        Ok(Self { accounts })
    }

    pub fn encode(&self) -> String {
        let mut result = format!(
            "kind=linos-accounts\nformat=1\ncount={}\n",
            self.accounts.len()
        );
        for (index, account) in self.accounts.iter().enumerate() {
            let prefix = format!("account.{index:04}");
            result.push_str(&format!("{prefix}.name={}\n{prefix}.uid={}\n{prefix}.gid={}\n{prefix}.primary_group={}\n{prefix}.supplementary_count={}\n",
                account.name, account.uid, account.primary.gid, account.primary.name, account.supplementary.len()));
            for (group, membership) in account.supplementary.iter().enumerate() {
                result.push_str(&format!("{prefix}.supplementary.{group:04}.name={}\n{prefix}.supplementary.{group:04}.gid={}\n", membership.name, membership.gid));
            }
        }
        result
    }
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut fields = Fields::new(bytes)?;
        if fields.next("kind")? != "linos-accounts" || fields.next("format")? != "1" {
            return Err(invalid("Unsupported accounts metadata"));
        }
        let count = fields.number("count", MAX_ACCOUNTS)?;
        let mut accounts = Vec::new();
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut groups = BTreeMap::new();
        let mut group_ids = BTreeMap::new();
        for index in 0..count {
            let prefix = format!("account.{index:04}");
            let account = name(fields.next(&format!("{prefix}.name"))?)?;
            let uid = fields.number(&format!("{prefix}.uid"), MAX_IDS)? as u32;
            let gid = fields.number(&format!("{prefix}.gid"), MAX_IDS)? as u32;
            let primary = Group {
                name: name(fields.next(&format!("{prefix}.primary_group"))?)?,
                gid,
            };
            let count = fields.number(&format!("{prefix}.supplementary_count"), 128)?;
            let mut supplementary = Vec::new();
            let mut seen = BTreeSet::new();
            for group in 0..count {
                let prefix = format!("{prefix}.supplementary.{group:04}");
                let item = Group {
                    name: name(fields.next(&format!("{prefix}.name"))?)?,
                    gid: fields.number(&format!("{prefix}.gid"), MAX_IDS)? as u32,
                };
                if item.name == primary.name
                    || item.gid == primary.gid
                    || !seen.insert(item.name.clone())
                {
                    return Err(invalid("Duplicate required group membership"));
                }
                supplementary.push(item);
            }
            if !names.insert(account.clone()) || !ids.insert(uid) {
                return Err(invalid("Duplicate required account name or UID"));
            }
            for group in std::iter::once(&primary).chain(&supplementary) {
                if groups
                    .insert(group.name.clone(), group.gid)
                    .is_some_and(|old| old != group.gid)
                    || group_ids
                        .insert(group.gid, group.name.clone())
                        .is_some_and(|old| old != group.name)
                {
                    return Err(invalid("Conflicting required group name or GID"));
                }
            }
            accounts.push(Requirement {
                name: account,
                uid,
                primary,
                supplementary,
            });
        }
        fields.finish()?;
        Ok(Self { accounts })
    }
    pub fn accounts(&self) -> &[Requirement] {
        &self.accounts
    }

    /// Preserve current databases verbatim. Return only genuinely new service
    /// accounts for native creation in an offline prepared configuration.
    pub fn plan(
        &self,
        old_vendor: &Self,
        current: &NativeAccounts,
    ) -> io::Result<Vec<Requirement>> {
        let mut additions = Vec::new();
        for required in &self.accounts {
            match current.users.get(&required.name) {
                Some(user) => {
                    if user.uid != required.uid || user.gid != required.primary.gid {
                        return Err(invalid(
                            "Required account conflicts with current numeric identity",
                        ));
                    }
                    current.require_group(&required.primary)?;
                    let desired: BTreeSet<_> = required
                        .supplementary
                        .iter()
                        .map(|group| group.name.as_str())
                        .collect();
                    let actual: BTreeSet<_> = current
                        .groups
                        .iter()
                        .filter(|(_, group)| {
                            group.gid != user.gid && group.members.contains(&required.name)
                        })
                        .map(|(name, _)| name.as_str())
                        .collect();
                    if desired != actual {
                        return Err(invalid(
                            "Required account memberships differ from current authorization",
                        ));
                    }
                    for group in &required.supplementary {
                        current.require_group(group)?;
                    }
                }
                None => {
                    if old_vendor
                        .accounts
                        .iter()
                        .any(|account| account.name == required.name)
                    {
                        return Err(invalid(
                            "Previously required account was removed by the administrator",
                        ));
                    }
                    if current.users.values().any(|user| user.uid == required.uid) {
                        return Err(invalid("New service account would reuse a current UID"));
                    }
                    if old_vendor
                        .accounts
                        .iter()
                        .any(|account| account.uid == required.uid && account.name != required.name)
                    {
                        return Err(invalid("New service account would reuse a reserved UID"));
                    }
                    for group in std::iter::once(&required.primary).chain(&required.supplementary) {
                        if old_vendor
                            .accounts
                            .iter()
                            .flat_map(|account| {
                                std::iter::once(&account.primary).chain(&account.supplementary)
                            })
                            .any(|reserved| {
                                reserved.gid == group.gid && reserved.name != group.name
                            })
                        {
                            return Err(invalid("New service group would reuse a reserved GID"));
                        }
                        if current.groups.contains_key(&group.name) {
                            current.require_group(group)?;
                        } else if current
                            .groups
                            .values()
                            .any(|existing| existing.gid == group.gid)
                        {
                            return Err(invalid("New service group would reuse a current GID"));
                        }
                    }
                    additions.push(required.clone());
                }
            }
        }
        Ok(additions)
    }
}

struct User {
    uid: u32,
    gid: u32,
    interactive: bool,
    unlocked: bool,
}
struct NativeGroup {
    gid: u32,
    members: BTreeSet<String>,
}

/// Does not retain password fields or implement mutations. Callers copy current
/// files with their metadata, then use native tools for the planned additions.
pub struct NativeAccounts {
    users: BTreeMap<String, User>,
    groups: BTreeMap<String, NativeGroup>,
    has_unlocked_password: bool,
}

fn lines(bytes: &[u8], columns: usize) -> io::Result<Vec<Vec<&str>>> {
    if bytes.is_empty()
        || bytes.len() > LIMIT
        || !bytes.ends_with(b"\n")
        || bytes.contains(&0)
        || bytes.contains(&b'\r')
    {
        return Err(invalid(
            "Native account database is missing, truncated or oversized",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("Invalid native database text"))?;
    let mut rows = Vec::new();
    for line in text.lines() {
        if rows.len() == MAX_ACCOUNTS as usize || line.len() > 64 * 1024 {
            return Err(invalid("Native database exceeds its row bound"));
        }
        let fields: Vec<_> = line.split(':').collect();
        if fields.len() != columns {
            return Err(invalid("Malformed native account database row"));
        }
        name(fields[0])?;
        rows.push(fields);
    }
    Ok(rows)
}

fn numeric(value: &str) -> io::Result<u32> {
    let value = decimal(value)?;
    if value > MAX_IDS {
        return Err(invalid("Invalid native UID or GID"));
    }
    Ok(value as u32)
}

fn members(value: &str) -> io::Result<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    if !value.is_empty() {
        for member in value.split(',') {
            if !result.insert(name(member)?) {
                return Err(invalid("Duplicate native group member"));
            }
        }
    }
    Ok(result)
}

impl NativeAccounts {
    pub fn require_administrator(&self, username: &str) -> io::Result<()> {
        if self.users.get(username).is_none_or(|user| {
            user.uid < 1000 || user.uid == 65534 || !user.interactive || !user.unlocked
        }) || self
            .groups
            .get("sudo")
            .is_none_or(|group| !group.members.contains(username))
        {
            return Err(invalid(
                "Enrolled owner is not a usable local administrator",
            ));
        }
        Ok(())
    }
    pub fn owns(&self, uid: u32, gid: u32) -> bool {
        self.users.values().any(|user| user.uid == uid)
            && self.groups.values().any(|group| group.gid == gid)
    }
    pub fn parse(passwd: &[u8], shadow: &[u8], group: &[u8], gshadow: &[u8]) -> io::Result<Self> {
        let mut users = BTreeMap::new();
        let mut uids = BTreeSet::new();
        for row in lines(passwd, 7)? {
            let uid = numeric(row[2])?;
            let gid = numeric(row[3])?;
            if !matches!(row[1], "x" | "*" | "!" | "!!")
                || !row[5].starts_with('/')
                || !row[6].starts_with('/')
                || !uids.insert(uid)
                || users
                    .insert(
                        row[0].into(),
                        User {
                            uid,
                            gid,
                            unlocked: false,
                            interactive: !matches!(
                                row[6],
                                "/usr/sbin/nologin"
                                    | "/sbin/nologin"
                                    | "/bin/false"
                                    | "/usr/bin/false"
                            ),
                        },
                    )
                    .is_some()
            {
                return Err(invalid("Invalid or duplicate current account identity"));
            }
        }
        let mut groups = BTreeMap::new();
        let mut gids = BTreeSet::new();
        for row in lines(group, 4)? {
            let gid = numeric(row[2])?;
            let members = members(row[3])?;
            if !matches!(row[1], "x" | "*" | "!" | "!!")
                || !gids.insert(gid)
                || groups
                    .insert(row[0].into(), NativeGroup { gid, members })
                    .is_some()
            {
                return Err(invalid("Invalid or duplicate current group identity"));
            }
        }
        let mut shadows = BTreeSet::new();
        let mut has_unlocked_password = false;
        for row in lines(shadow, 9)? {
            has_unlocked_password |= !row[1].starts_with(['!', '*']);
            if !users.contains_key(row[0]) || !shadows.insert(row[0]) {
                return Err(invalid("Shadow account reference is missing or duplicated"));
            }
            if let Some(user) = users.get_mut(row[0]) {
                user.unlocked = !row[1].is_empty() && !row[1].starts_with(['!', '*']);
            }
            for value in &row[2..] {
                if !matches!(*value, "" | "-1") {
                    decimal(value)?;
                }
            }
            // Password hashes and aging remain in the original current file.
        }
        let mut group_shadows = BTreeSet::new();
        for row in lines(gshadow, 4)? {
            let existing = groups
                .get(row[0])
                .ok_or_else(|| invalid("Gshadow group does not exist"))?;
            if !group_shadows.insert(row[0])
                || members(row[3])? != existing.members
                || members(row[2])?
                    .iter()
                    .any(|member| !users.contains_key(member))
            {
                return Err(invalid("Gshadow membership is inconsistent"));
            }
        }
        if shadows.len() != users.len()
            || group_shadows.len() != groups.len()
            || users.values().any(|user| !gids.contains(&user.gid))
            || groups.values().any(|group| {
                group
                    .members
                    .iter()
                    .any(|member| !users.contains_key(member))
            })
            || users
                .get("root")
                .is_none_or(|root| root.uid != 0 || root.gid != 0)
            || groups.get("root").is_none_or(|root| root.gid != 0)
        {
            return Err(invalid(
                "Native account databases have missing or conflicting cross references",
            ));
        }
        Ok(Self {
            users,
            groups,
            has_unlocked_password,
        })
    }
    fn require_group(&self, required: &Group) -> io::Result<()> {
        if self
            .groups
            .get(&required.name)
            .is_none_or(|group| group.gid != required.gid)
        {
            return Err(invalid(
                "Required group differs from current numeric identity",
            ));
        }
        Ok(())
    }
}

fn databases(etc: &std::fs::File) -> io::Result<[Vec<u8>; 4]> {
    use rustix::fs::{self, Mode, OFlags};
    use std::{io::Read, os::unix::fs::MetadataExt};
    let mut result = Vec::new();
    for name in ["passwd", "shadow", "group", "gshadow"] {
        let file = std::fs::File::from(fs::openat(
            etc,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.uid() != 0
            || metadata.mode() & 0o022 != 0
        {
            return Err(invalid(
                "Prepared native database is not a protected standalone regular file",
            ));
        }
        let mut bytes = Vec::new();
        file.take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > LIMIT {
            return Err(invalid("Prepared native database exceeds its bound"));
        }
        result.push(bytes);
    }
    result
        .try_into()
        .map_err(|_| invalid("Missing native database"))
}

impl Contract {
    /// Create only planned new system accounts in a private offline root whose
    /// etc directory is the prepared generation. The caller later publishes etc
    /// under State/config; this never targets the running root.
    pub fn create_required(
        &self,
        old_vendor: &Self,
        staging_root: &std::path::Path,
    ) -> io::Result<()> {
        use rustix::fs::{self, Mode, OFlags};
        use std::{
            fs::File,
            os::unix::fs::MetadataExt,
            process::{Command, Stdio},
        };
        if rustix::process::geteuid().as_raw() != 0 || staging_root == std::path::Path::new("/") {
            return Err(invalid(
                "Native account creation requires a private offline root and root privilege",
            ));
        }
        let root = crate::recovery::update::journal::open_directory(staging_root)?;
        if root.metadata()?.mode() & 0o077 != 0 {
            return Err(invalid("Offline account workspace must be private"));
        }
        let etc = File::from(fs::openat(
            &root,
            "etc",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let before = databases(&etc)?;
        let current = NativeAccounts::parse(&before[0], &before[1], &before[2], &before[3])?;
        let additions = self.plan(old_vendor, &current)?;
        let mut created_groups = BTreeSet::new();
        for account in &additions {
            for group in std::iter::once(&account.primary).chain(&account.supplementary) {
                if current.groups.contains_key(&group.name)
                    || !created_groups.insert(group.name.clone())
                {
                    continue;
                }
                let status = Command::new("/usr/sbin/groupadd")
                    .arg("--root")
                    .arg(staging_root)
                    .args(["--system", "--gid", &group.gid.to_string(), &group.name])
                    .env("LC_ALL", "C")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()?;
                if !status.success() {
                    return Err(invalid(
                        "Native service group creation failed in prepared configuration",
                    ));
                }
            }
            let mut command = Command::new("/usr/sbin/useradd");
            command.arg("--root").arg(staging_root).args([
                "--system",
                "--no-create-home",
                "--no-user-group",
                "--no-log-init",
                "--uid",
                &account.uid.to_string(),
                "--gid",
                &account.primary.name,
                "--home-dir",
                "/nonexistent",
                "--shell",
                "/usr/sbin/nologin",
                "--password",
                "!",
            ]);
            if !account.supplementary.is_empty() {
                command.args([
                    "--groups",
                    &account
                        .supplementary
                        .iter()
                        .map(|group| group.name.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                ]);
            }
            let status = command
                .arg(&account.name)
                .env("LC_ALL", "C")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !status.success() {
                return Err(invalid(
                    "Native service account creation failed in prepared configuration",
                ));
            }
        }
        let after = databases(&etc)?;
        let prepared = NativeAccounts::parse(&after[0], &after[1], &after[2], &after[3])?;
        if !self.plan(old_vendor, &prepared)?.is_empty() {
            return Err(invalid(
                "Native account creation did not satisfy the required identities",
            ));
        }
        // Existing account fields include credentials and aging; compare exact
        // records by name, never print their values in a conflict diagnostic.
        for (index, columns) in [(0, 7), (1, 9)] {
            let new: BTreeMap<_, _> = lines(&after[index], columns)?
                .into_iter()
                .map(|row| (row[0], row))
                .collect();
            for old in lines(&before[index], columns)? {
                if new.get(old[0]) != Some(&old) {
                    return Err(invalid(
                        "Native tool changed an existing account or credential",
                    ));
                }
            }
        }
        for index in [2, 3] {
            let new: BTreeMap<_, _> = lines(&after[index], 4)?
                .into_iter()
                .map(|row| (row[0], row))
                .collect();
            for old in lines(&before[index], 4)? {
                if new.get(old[0]).is_none_or(|row| row[..3] != old[..3]) {
                    return Err(invalid(
                        "Native tool changed an existing group identity or authorization policy",
                    ));
                }
            }
        }
        for (name, group) in &current.groups {
            let new = prepared
                .groups
                .get(name)
                .ok_or_else(|| invalid("Native tool removed an existing group"))?;
            if new.gid != group.gid
                || new
                    .members
                    .iter()
                    .filter(|member| current.users.contains_key(*member))
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    != group.members
            {
                return Err(invalid(
                    "Native tool changed existing account authorization",
                ));
            }
        }
        for name in ["passwd", "shadow", "group", "gshadow"] {
            File::from(fs::openat(
                &etc,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?)
            .sync_all()?;
        }
        etc.sync_all()?;
        root.sync_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn current() -> NativeAccounts {
        NativeAccounts::parse(
            b"root:x:0:0:root:/root:/bin/sh\nowner:x:1000:1000::/home/owner:/bin/sh\n",
            b"root:!:20000:0:99999:7:::\nowner:$6$current:20000:0:99999:7:::\n",
            b"root:x:0:\nowner:x:1000:\nsudo:x:27:owner\n",
            b"root:!::\nowner:!::\nsudo:!::owner\n",
        )
        .unwrap()
    }
    fn contract(account: &str, uid: u32, gid: u32, group: &str) -> Contract {
        Contract::parse(format!("kind=linos-accounts\nformat=1\ncount=1\naccount.0000.name={account}\naccount.0000.uid={uid}\naccount.0000.gid={gid}\naccount.0000.primary_group={group}\naccount.0000.supplementary_count=0\n").as_bytes()).unwrap()
    }
    #[test]
    fn native_health_requires_a_current_unlocked_interactive_administrator() {
        let mut accounts = current();
        accounts.require_administrator("owner").unwrap();
        assert!(accounts.require_administrator("root").is_err());
        assert!(accounts.require_administrator("absent").is_err());
        accounts.users.get_mut("owner").unwrap().unlocked = false;
        assert!(accounts.require_administrator("owner").is_err());
        accounts.users.get_mut("owner").unwrap().unlocked = true;
        accounts.users.get_mut("owner").unwrap().interactive = false;
        assert!(accounts.require_administrator("owner").is_err());
        accounts.users.get_mut("owner").unwrap().interactive = true;
        accounts.groups.get_mut("sudo").unwrap().members.clear();
        assert!(accounts.require_administrator("owner").is_err());
    }
    #[test]
    fn additions_refuse_current_reserved_and_deleted_identities() {
        let empty = Contract::parse(b"kind=linos-accounts\nformat=1\ncount=0\n").unwrap();
        let service = contract("svc", 50, 50, "svc");
        assert_eq!(service.plan(&empty, &current()).unwrap().len(), 1);
        assert!(
            contract("svc", 1000, 50, "svc")
                .plan(&empty, &current())
                .is_err()
        );
        assert!(
            contract("svc", 50, 1000, "svc")
                .plan(&empty, &current())
                .is_err()
        );
        assert!(service.plan(&service, &current()).is_err());
        assert!(
            service
                .plan(&contract("removed", 50, 51, "removed"), &current())
                .is_err()
        );
        assert!(
            contract("root", 0, 0, "root")
                .plan(&empty, &current())
                .unwrap()
                .is_empty()
        );
        assert!(Contract::baseline(&current()).is_err());
        assert!(Contract::parse(service.encode().as_bytes()).unwrap() == service);
    }
    #[test]
    fn native_databases_require_shadow_unique_ids_and_consistent_memberships() {
        let passwd = b"root:x:0:0:root:/root:/bin/sh\n";
        let shadow = b"root:!:20000:0:99999:7:::\n";
        let group = b"root:x:0:\n";
        let gshadow = b"root:!::\n";
        let parsed = NativeAccounts::parse(passwd, shadow, group, gshadow).unwrap();
        let baseline = Contract::baseline(&parsed).unwrap();
        assert!(Contract::parse(baseline.encode().as_bytes()).unwrap() == baseline);
        assert!(NativeAccounts::parse(passwd, b"", group, gshadow).is_err());
        assert!(
            NativeAccounts::parse(
                b"root:x:0:0::/root:/bin/sh\nalias:x:0:0::/alias:/bin/sh\n",
                shadow,
                group,
                gshadow
            )
            .is_err()
        );
        assert!(
            NativeAccounts::parse(passwd, shadow, b"root:x:0:missing\n", b"root:!::missing\n")
                .is_err()
        );
        assert!(NativeAccounts::parse(passwd, shadow, b"root:x:0:root\n", gshadow).is_err());
    }
}
