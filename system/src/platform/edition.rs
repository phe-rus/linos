//! Shared edition identity for the Linos family, independent of UI toolkit.
//! Never infer a platform edition from CPU type or whether DISPLAY exists.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    Server,
    Desktop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryInterface {
    Console,
    Graphical,
}

impl Edition {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "server" => Some(Self::Server),
            "desktop" => Some(Self::Desktop),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Desktop => "desktop",
        }
    }

    pub const fn primary_interface(self) -> PrimaryInterface {
        match self {
            Self::Server => PrimaryInterface::Console,
            Self::Desktop => PrimaryInterface::Graphical,
        }
    }

    pub const fn recovery_accessible_without_graphics(self) -> bool {
        // Desktop may show a GUI, but the rescue console remains available.
        let _ = self;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_implicit_or_unsupported_editions() {
        assert_eq!(Edition::from_name("server"), Some(Edition::Server));
        assert_eq!(Edition::from_name("desktop"), Some(Edition::Desktop));
        assert_eq!(Edition::from_name("mobile"), None);
        assert_eq!(Edition::from_name("Server"), None);
        assert_eq!(Edition::from_name(""), None);
    }

    #[test]
    fn recovery_is_available_without_a_graphical_stack() {
        assert_eq!(
            Edition::Server.primary_interface(),
            PrimaryInterface::Console
        );
        assert_eq!(
            Edition::Desktop.primary_interface(),
            PrimaryInterface::Graphical
        );
        assert!(Edition::Desktop.recovery_accessible_without_graphics());
        assert!(Edition::Server.recovery_accessible_without_graphics());
    }
}
