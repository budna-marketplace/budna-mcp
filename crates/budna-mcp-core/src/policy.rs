#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCapability {
    PublicExplore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolPolicy {
    public_explore: bool,
}

impl ToolPolicy {
    pub const fn public_explore() -> Self {
        Self {
            public_explore: true,
        }
    }

    pub const fn allows(&self, capability: ToolCapability) -> bool {
        match capability {
            ToolCapability::PublicExplore => self.public_explore,
        }
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
