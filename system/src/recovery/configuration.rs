//! Three input configuration merge. Current account/authorization databases and
//! host keys remain current; new service accounts require separate native tools.
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

fn invalid(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub mod generation;
pub mod tree;

#[derive(Clone, PartialEq, Eq)]
pub enum Content {
    Directory,
    File(Vec<u8>),
    Symlink(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct Object {
    pub content: Content,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}

pub type Tree = BTreeMap<String, Object>;

fn current_only(path: &str) -> bool {
    matches!(
        path,
        "passwd" | "shadow" | "group" | "gshadow" | "subuid" | "subgid" | "sudoers" | "machine-id"
    ) || path == "sudoers.d"
        || path.starts_with("sudoers.d/")
        || path == "linos"
        || path.starts_with("linos/")
        || path
            .strip_prefix("ssh/")
            .is_some_and(|name| name.starts_with("ssh_host_"))
}

fn validate(tree: &Tree) -> io::Result<()> {
    for (path, object) in tree {
        if path == "." {
            if !matches!(object.content, Content::Directory) || object.mode & !0o7777 != 0 {
                return Err(invalid("Invalid configuration root metadata"));
            }
            continue;
        }
        if path.len() > 4096
            || path.split('/').any(|part| {
                part.is_empty() || matches!(part, "." | "..") || part.contains(['\0', '\n', '\r'])
            })
        {
            return Err(invalid("Configuration path is not canonical and relative"));
        }
        if object.mode & !0o7777 != 0 {
            return Err(invalid("Invalid configuration permission bits"));
        }
        if let Some((parent, _)) = path.rsplit_once('/')
            && !tree
                .get(parent)
                .is_some_and(|parent| matches!(parent.content, Content::Directory))
        {
            return Err(invalid(
                "Configuration ancestor deletion or type change would discard descendants",
            ));
        }
        if matches!(path.as_str(), "passwd" | "shadow" | "group" | "gshadow")
            && !matches!(object.content, Content::File(_))
        {
            return Err(invalid("Native account databases must be regular files"));
        }
    }
    Ok(())
}

pub fn merge(base: &Tree, current: &Tree, target: &Tree) -> io::Result<Tree> {
    for tree in [base, current, target] {
        validate(tree)?;
    }
    let paths: BTreeSet<_> = base
        .keys()
        .chain(current.keys())
        .chain(target.keys())
        .collect();
    let mut merged = Tree::new();
    for path in paths {
        let (b, c, n) = (base.get(path), current.get(path), target.get(path));
        let choice = if current_only(path) {
            c
        } else if c == b {
            n
        } else if n == b || c == n {
            c
        } else {
            return Err(invalid(format!("Configuration merge conflict at {path}")));
        };
        if let Some(object) = choice {
            merged.insert(path.clone(), object.clone());
        }
    }
    validate(&merged)?;
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(value: &[u8]) -> Object {
        Object {
            content: Content::File(value.into()),
            uid: 0,
            gid: 0,
            mode: 0o600,
        }
    }
    #[test]
    fn three_input_merge_preserves_current_credentials_deletions_and_host_keys() {
        let base = Tree::from([
            ("ordinary".into(), file(b"old")),
            ("shadow".into(), file(b"old password")),
            ("deleted".into(), file(b"delete locally")),
        ]);
        let mut current = base.clone();
        current.insert("shadow".into(), file(b"new password"));
        current.remove("deleted");
        let mut target = base.clone();
        target.insert("ordinary".into(), file(b"new vendor"));
        target.insert("shadow".into(), file(b"locked vendor"));
        let merged = merge(&base, &current, &target).unwrap();
        assert!(merged.get("shadow") == current.get("shadow"));
        assert!(merged.get("ordinary") == target.get("ordinary"));
        assert!(!merged.contains_key("deleted"));
        target.insert("deleted".into(), file(b"vendor edit"));
        assert!(merge(&base, &current, &target).is_err());
    }
    #[test]
    fn metadata_edits_and_ancestor_type_changes_are_conflicts() {
        let directory = Object {
            content: Content::Directory,
            uid: 0,
            gid: 0,
            mode: 0o755,
        };
        let base = Tree::from([
            ("dir".into(), directory.clone()),
            ("dir/file".into(), file(b"old")),
        ]);
        let mut current = base.clone();
        current.insert("dir/local".into(), file(b"keep"));
        let target = Tree::new();
        assert!(merge(&base, &current, &target).is_err());
        let mut target = base.clone();
        target.get_mut("dir/file").unwrap().mode = 0o644;
        let mut current = base.clone();
        current.get_mut("dir/file").unwrap().gid = 1;
        assert!(merge(&base, &current, &target).is_err());
        let mut bad = base.clone();
        bad.insert(
            "dir".into(),
            Object {
                content: Content::Symlink("/outside".into()),
                ..directory
            },
        );
        assert!(merge(&base, &current, &bad).is_err());
    }
}
