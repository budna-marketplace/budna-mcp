#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ToolCapability {
    PublicExplore,
}

/// The concrete capability profile exposed by a server instance.
///
/// Additional profiles must use their own router and authorization boundary;
/// they must not disable individual tools behind an already-advertised static
/// router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ToolPolicy {
    PublicExplore,
}

impl ToolPolicy {
    pub const fn public_explore() -> Self {
        Self::PublicExplore
    }

    pub const fn allows(&self, capability: ToolCapability) -> bool {
        matches!(
            (self, capability),
            (Self::PublicExplore, ToolCapability::PublicExplore)
        )
    }
}

impl Default for ToolPolicy {
    fn default() -> Self {
        Self::public_explore()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_policy_enables_public_exploration() {
        let policy = ToolPolicy::public_explore();

        assert!(policy.allows(ToolCapability::PublicExplore));
    }
}
